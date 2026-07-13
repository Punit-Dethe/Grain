//! [GRAIN] Grain Recall — conversational memory retrieval (RECALL-PLAN R1).
//!
//! The user presses the Recall shortcut, speaks a half-remembered fragment, and
//! Grain answers from their saved notes. This module owns the retrieval +
//! synthesis pipeline; the actual LLM call is delegated to the Agent's existing
//! provider/rotation driver (`agent::run_messages`), and the surfaces (pill
//! input, bottom-right panel, follow-ups) are the Agent's, driven in
//! `AgentMode::Recall`. Nothing here assumes a UI is alive.
//!
//! Per turn: hybrid retrieve (FTS ∪ semantic, RRF-fused) → memories block with
//! stable M-ids (unioned across follow-up turns) → tight system prompt →
//! answer + a tolerant trailing `SOURCES:` / `NOT_FOUND` line that we parse and
//! strip before display.

use anyhow::Result;
use tauri::{AppHandle, Manager};

use super::backend::{self, Backend};
use super::capture;
use super::note::Note;
use crate::agent::{AgentMessage, AgentReply, AgentSource};

/// Memories fed to the model per turn (post-fusion). Recall over precision —
/// the model does the final filtering; an extra note costs a few hundred
/// tokens, a missing one costs the answer.
const TOP_K_PER_TURN: usize = 6;
/// Dual-stage retrieval: fuse to a WIDE candidate pool, then rerank down to
/// `TOP_K_PER_TURN`. The semantic leg already returns ~24 and FTS is cheap, so
/// widening to 20 candidates is effectively free (one pass), and the CPU
/// reranker (RRF + term overlap + recency) picks the *relevant* 6, not just the
/// top-6 by raw fused rank.
const CANDIDATE_POOL: usize = 20;
/// Cap on the session's stable memory registry. M-ids never move within a
/// session, so we never evict (that would renumber): once full, new hits for
/// later turns simply aren't added to the block. Personal-scale, short
/// sessions — a handful of distinct memories is ample. This is the RAM/context
/// safety bound now that note bodies are sent in full (no truncation).
const MAX_SESSION_MEMORIES: usize = 12;
/// Max native `search_memory` tool round-trips per turn. Bounds the added
/// latency/embedding work to the minority of turns that actually need to look
/// again; after the cap we force a direct answer. Active-turn only — no idle
/// cost ever.
const MAX_TOOL_HOPS: usize = 3;
/// RRF constant (standard).
const RRF_K: f64 = 60.0;
/// Minimum raw cosine similarity for a semantic hit to count as RELATED. KNN
/// always returns the nearest notes even when nothing matches, so without a
/// floor a tiny corpus (e.g. 4 notes) leaks unrelated notes into the block just
/// to fill the top-K. FTS still contributes exact keyword matches regardless;
/// this only gates the fuzzy semantic leg.
///
/// TUNING KNOB, calibrated against the ASYMMETRIC (instruction-prefixed) query
/// geometry via `embed::tests::query_prefix_separates_related_from_unrelated`:
/// measured unrelated pairs sit ~0.40–0.44, a direct hit ~0.78. 0.50 rejects
/// the measured noise band with margin while leaving half the gap for genuine
/// loose matches (and FTS backs up exact-keyword hits regardless). Re-run that
/// test with `--nocapture` before moving this.
const SEMANTIC_MIN_SIMILARITY: f64 = 0.50;
/// Reranker weights, semantic profile (must sum to 1.0): fused RRF rank,
/// EXACT stored-vector cosine (min-max normalized within the pool), query-term
/// overlap in title/tldr, and recency decay. RRF still leads (robust, blends
/// both legs); the exact cosine is the true semantic evidence the pool ranks
/// only approximate — an FTS-only candidate finally gets scored on meaning,
/// not just its BM25 rank.
const RERANK_W_RRF: f64 = 0.35;
const RERANK_W_SEMANTIC: f64 = 0.25;
const RERANK_W_OVERLAP: f64 = 0.25;
const RERANK_W_RECENCY: f64 = 0.15;
/// Lexical profile (semantic off / model absent / embed failed): the original
/// three-signal blend.
const RERANK_LEX_W_RRF: f64 = 0.5;
const RERANK_LEX_W_OVERLAP: f64 = 0.3;
const RERANK_LEX_W_RECENCY: f64 = 0.2;
/// Bodies at or below this go to the model VERBATIM — the no-truncation
/// philosophy holds for every dictated capture and normal note. Only past it
/// (in practice: long foreign Obsidian documents) does query-aware excerpting
/// kick in, because one 100 KB note would otherwise drown a small edge model's
/// whole context ("lost in the middle").
const FULL_BODY_CHARS: usize = 2800;
/// Excerpt budget for a long note: the best-matching sections, in document
/// order, up to about this many chars (~600 tokens). Sections come from the
/// same markdown chunker the embeddings use, so what recall shows is aligned
/// with what semantic search matched on.
const EXCERPT_BUDGET_CHARS: usize = 2400;

/// Grain Recall session state, held in `AgentState` and cleared on each fresh
/// summon. `ids[i]` is the note id shown as memory `M(i+1)`; the ordering is
/// append-only within a session so source numbering stays stable and additive
/// across follow-up turns.
#[derive(Default)]
pub struct RecallSession {
    ids: Vec<String>,
}

impl RecallSession {
    pub fn clear(&mut self) {
        self.ids.clear();
    }

    /// Register a note id, returning its 1-based M number. Existing ids keep
    /// their number; a new id is appended unless the registry is full (then it
    /// returns `None` and the memory is simply not shown this turn).
    fn register(&mut self, note_id: &str) -> Option<usize> {
        if let Some(pos) = self.ids.iter().position(|x| x == note_id) {
            return Some(pos + 1);
        }
        if self.ids.len() >= MAX_SESSION_MEMORIES {
            return None;
        }
        self.ids.push(note_id.to_string());
        Some(self.ids.len())
    }

    /// The note id behind memory `Mn` (1-based), if any. Source resolution now
    /// happens inline in `run_turn` (with title/date), so this is test-only.
    #[cfg(test)]
    fn note_id_of(&self, m: usize) -> Option<&str> {
        self.ids.get(m.wrapping_sub(1)).map(String::as_str)
    }

    /// All registered ids in M-order (for rebuilding the block each turn).
    fn ordered(&self) -> Vec<String> {
        self.ids.clone()
    }
}

/// The Grain Recall system prompt. Kept tight on purpose: small edge models
/// drift when the prompt is long, so this states the contract once, clearly —
/// the create-vs-edit distinction and the single trailing line are the parts
/// that must never blur.
fn system_prompt(now: &str, weekday: &str) -> String {
    format!(
        "You are Grain, the user's personal memory. Answer ONLY from their saved memories, which \
         are attached to their latest message tagged [Mn] (each shows when it was saved). Today \
         is {now} ({weekday}).\n\
         \n\
         TOOL search_memory(query, minDate?, maxDate?): call it when the attached memories don't \
         hold what you need — the topic changed, they mention something not shown, or they name a \
         time (\"last week\", \"in June\" → pass minDate/maxDate as YYYY-MM-DD). One focused \
         search; never guess before searching.\n\
         \n\
         ANSWER a question: one short, natural sentence (no preamble or lists); trust the newest \
         memory if they disagree. End with one line: `SOURCES: M2, M4` (the memories you used, or \
         `SOURCES: none`). If it truly isn't saved after searching, end with `NOT_FOUND` instead \
         and stop asking questions.\n\
         \n\
         CHANGE memories — end with ONE action line INSTEAD of a SOURCES line:\n\
         - SAVE / CREATE / REMEMBER something new → `ACTION: remember`. This ALWAYS makes a NEW \
         memory. Use it whenever the user says to note, save, or remember something — EVEN IF a \
         related memory already exists. Never merge new information into an old memory here (a new \
         wifi password the user asks to save is a NEW memory, not an edit of the old one).\n\
         - CHANGE or ADD TO something already saved → search_memory first, then: exactly ONE \
         memory matches → `ACTION: update Mn`; zero or several match → ask ONE short question \
         instead (end with SOURCES, no ACTION). Never edit the wrong memory.\n\
         - Tick off todos → `ACTION: complete Mn todos 1,2`. Delete → `ACTION: forget Mn` (ask \
         them to confirm; the app does the final click).\n\
         Phrase remember/update/complete replies as a done-confirmation (e.g. Done — saved your \
         new wifi password.). A plain question ALWAYS ends with SOURCES, never ACTION."
    )
}

// -- retrieval ------------------------------------------------------------------

/// Hybrid retrieve for one query: FTS ∪ semantic, fused with Reciprocal Rank
/// Fusion, top `TOP_K_PER_TURN`. Semantic is used only when enabled AND the
/// model is on disk; otherwise it degrades to FTS-only SILENTLY (recall must
/// work for users who never opted into the model). All store/embed work runs
/// off the async runtime.
/// Dual-stage hybrid retrieve for one query (RECALL SEARCH-OVERHAUL S1): FTS ∪
/// semantic fused with Reciprocal Rank Fusion to a WIDE `CANDIDATE_POOL`, then a
/// CPU reranker narrows to `TOP_K_PER_TURN` the *relevant* memories. `range` is
/// an optional inclusive `(min_ms, max_ms)` timestamp pre-filter (the tool's
/// minDate/maxDate). Semantic is used only when enabled AND on disk; otherwise
/// it degrades to FTS-only SILENTLY. All store/embed work runs off the async
/// runtime.
async fn retrieve(
    app: &AppHandle,
    be: &Backend,
    query: &str,
    range: Option<(i64, i64)>,
) -> Result<Vec<Note>> {
    let semantic_on = {
        let s = crate::settings::get_settings(app);
        s.grain_space_semantic && super::embed::model_on_disk()
    };
    let half_life_days = crate::settings::get_settings(app).grain_space_decay_half_life_days;

    let be_owned = be.clone();
    let query_owned = query.to_string();

    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
        // Natural-language FTS: stopword-filtered OR semantics. A spoken
        // question ("what was the wifi password for the cabin…") almost never
        // matches ALL its words — implicit-AND would zero this leg exactly
        // when recall needs it.
        let fts = backend::search_notes_natural(&be_owned, &query_owned, range)?;

        // The query vector doubles as the reranker's evidence source, so it
        // outlives the KNN leg.
        let mut query_vec: Option<Vec<f32>> = None;
        let semantic = if semantic_on {
            // Re-embed anything stale so semantic results reflect current
            // content (same batch the overlay's search path uses).
            let stale = backend::stale_embed_texts(&be_owned)?;
            if !stale.is_empty() {
                let texts: Vec<String> = stale.iter().map(|(_, t)| t.clone()).collect();
                match super::embed::embed(texts) {
                    Ok(vectors) => {
                        let items: Vec<(String, Vec<f32>)> =
                            stale.into_iter().map(|(id, _)| id).zip(vectors).collect();
                        backend::store_embeddings(&be_owned, &items)?;
                    }
                    Err(e) => {
                        // Embedding failed (e.g. model load error): fall back to
                        // FTS-only for this turn rather than failing the answer.
                        log::warn!("[GRAIN] recall: embed failed ({e:#}); FTS-only this turn");
                        let pool = fuse_scored(fts, Vec::new(), CANDIDATE_POOL);
                        return Ok(rerank(
                            &query_owned,
                            pool,
                            &std::collections::HashMap::new(),
                            half_life_days,
                            TOP_K_PER_TURN,
                        ));
                    }
                }
            }
            // Asymmetric embedding: the query carries BGE's retrieval
            // instruction; stored note vectors are bare (the model's intended
            // geometry — no re-embedding needed).
            match super::embed::embed_query(query_owned.clone()) {
                Ok(q) => {
                    let hits = backend::semantic_search_ranged(
                        &be_owned,
                        &q,
                        half_life_days,
                        range,
                        SEMANTIC_MIN_SIMILARITY,
                    )?;
                    query_vec = Some(q);
                    hits
                }
                Err(e) => {
                    log::warn!("[GRAIN] recall: query embed failed ({e:#}); FTS-only");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let pool = fuse_scored(fts, semantic, CANDIDATE_POOL);

        // Exact re-scoring: true cosine for EVERY pool candidate from its
        // stored chunk vectors — including FTS hits the KNN head never saw.
        // Failure degrades to the lexical rerank profile, never the turn.
        let sims = match &query_vec {
            Some(q) => {
                let ids: Vec<String> = pool.iter().map(|(n, _)| n.id.clone()).collect();
                backend::note_similarities(&be_owned, &ids, q).unwrap_or_else(|e| {
                    log::warn!("[GRAIN] recall: exact re-score failed ({e:#})");
                    std::collections::HashMap::new()
                })
            }
            None => std::collections::HashMap::new(),
        };
        Ok(rerank(
            &query_owned,
            pool,
            &sims,
            half_life_days,
            TOP_K_PER_TURN,
        ))
    })
    .await?
}

/// Reciprocal Rank Fusion of two ranked note lists: `score(id) = Σ 1/(k+rank)`
/// (0-based rank, k=60). Returns the top `k` notes PAIRED WITH their fused
/// score (the reranker needs the raw score). Deterministic: ties break by
/// newest timestamp (HashMap iteration is not ordered).
pub(crate) fn fuse_scored(fts: Vec<Note>, semantic: Vec<Note>, k: usize) -> Vec<(Note, f64)> {
    use std::collections::HashMap;
    let mut score: HashMap<String, f64> = HashMap::new();
    let mut notes: HashMap<String, Note> = HashMap::new();

    for (rank, note) in fts.into_iter().enumerate() {
        *score.entry(note.id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f64);
        notes.entry(note.id.clone()).or_insert(note);
    }
    for (rank, note) in semantic.into_iter().enumerate() {
        *score.entry(note.id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f64);
        notes.entry(note.id.clone()).or_insert(note);
    }

    let mut ranked: Vec<(String, f64)> = score.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ta = notes.get(&a.0).map(|n| n.timestamp).unwrap_or(0);
                let tb = notes.get(&b.0).map(|n| n.timestamp).unwrap_or(0);
                tb.cmp(&ta)
            })
    });
    ranked
        .into_iter()
        .take(k)
        .filter_map(|(id, s)| notes.remove(&id).map(|n| (n, s)))
        .collect()
}

/// Back-compat thin wrapper: RRF fuse to top `k` notes without the scores. Kept
/// for tests / callers that only want the ranked notes.
#[cfg(test)]
fn fuse(fts: Vec<Note>, semantic: Vec<Note>, k: usize) -> Vec<Note> {
    fuse_scored(fts, semantic, k)
        .into_iter()
        .map(|(n, _)| n)
        .collect()
}

/// Stage-2 reranker (SEARCH-OVERHAUL S1, heuristic — no second model). Re-scores
/// the fused candidate pool by a weighted blend of: normalized RRF score, EXACT
/// stored-vector cosine (`sims`, min-max normalized within the pool so it
/// self-calibrates to whatever range the model produces), query-term overlap in
/// title/tldr, and recency decay (`exp(-λΔt)`, pinned notes treated as fresh).
/// With no semantic evidence at all it falls back to the lexical weight
/// profile; a candidate MISSING from a non-empty `sims` (no stored vector yet)
/// scores a neutral 0.5 — absence of evidence is not evidence of irrelevance.
/// Deterministic and testable; ties break toward the newer memory. Top `k`.
fn rerank(
    query: &str,
    pool: Vec<(Note, f64)>,
    sims: &std::collections::HashMap<String, f64>,
    half_life_days: u32,
    k: usize,
) -> Vec<Note> {
    if pool.is_empty() {
        return Vec::new();
    }
    let max_rrf = pool
        .iter()
        .map(|(_, s)| *s)
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);

    let (sim_min, sim_max) = sims
        .values()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &c| (lo.min(c), hi.max(c)));
    let sims_spread = !sims.is_empty() && sim_max > sim_min;
    let (w_rrf, w_sem, w_overlap, w_recency) = if sims.is_empty() {
        (
            RERANK_LEX_W_RRF,
            0.0,
            RERANK_LEX_W_OVERLAP,
            RERANK_LEX_W_RECENCY,
        )
    } else {
        (
            RERANK_W_RRF,
            RERANK_W_SEMANTIC,
            RERANK_W_OVERLAP,
            RERANK_W_RECENCY,
        )
    };

    let terms = query_terms(query);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let lambda_per_ms = if half_life_days == 0 {
        0.0
    } else {
        std::f64::consts::LN_2 / (half_life_days as f64 * 24.0 * 60.0 * 60.0 * 1000.0)
    };

    let mut scored: Vec<(f64, Note)> = pool
        .into_iter()
        .map(|(note, rrf)| {
            let norm_rrf = rrf / max_rrf;
            let semantic = match sims.get(&note.id) {
                Some(c) if sims_spread => (c - sim_min) / (sim_max - sim_min),
                _ => 0.5, // no vector / no spread — neutral
            };
            let overlap = term_overlap(&terms, &note);
            let age_ms = if note.is_pinned {
                0.0
            } else {
                (now_ms - note.timestamp).max(0) as f64
            };
            let recency = (-lambda_per_ms * age_ms).exp();
            let final_score =
                w_rrf * norm_rrf + w_sem * semantic + w_overlap * overlap + w_recency * recency;
            (final_score, note)
        })
        .collect();

    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.timestamp.cmp(&a.1.timestamp))
    });
    scored.into_iter().take(k).map(|(_, n)| n).collect()
}

/// Lowercased alphanumeric query tokens (length ≥ 2) for term-overlap scoring.
fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

/// Fraction of query terms that appear in the note's title + tldr (the
/// high-signal metadata). 0.0 when there are no query terms. Reor-style lexical
/// emphasis without their post-hoc keyword re-score architecture.
fn term_overlap(terms: &[String], note: &Note) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let hay = format!("{} {}", note.title, note.tldr).to_lowercase();
    let hits = terms.iter().filter(|t| hay.contains(t.as_str())).count();
    hits as f64 / terms.len() as f64
}

// -- memories block -------------------------------------------------------------

/// Fraction of query terms present in `text` (lowercased containment).
fn text_overlap(terms: &[String], text: &str) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let hay = text.to_lowercase();
    let hits = terms.iter().filter(|t| hay.contains(t.as_str())).count();
    hits as f64 / terms.len() as f64
}

/// Query-aware excerpt of a LONG body: pick the sections (same markdown
/// chunker the embeddings use) that best match the query terms, within
/// `EXCERPT_BUDGET_CHARS`, re-joined in document order with `[…]` gap markers.
/// The first section gets a small bonus (a note's opening usually carries its
/// identity, and it is the deterministic fallback when no term matches).
/// Returns `None` for bodies within `FULL_BODY_CHARS` — those go verbatim.
fn excerpt_body(body: &str, terms: &[String]) -> Option<String> {
    if body.chars().count() <= FULL_BODY_CHARS {
        return None;
    }
    let chunks = super::vault::chunk_body(body);
    let mut ranked: Vec<(usize, f64)> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let bonus = if i == 0 { 0.15 } else { 0.0 };
            (i, text_overlap(terms, c) + bonus)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut used = 0usize;
    let mut picked: Vec<usize> = Vec::new();
    for (i, _) in &ranked {
        let len = chunks[*i].chars().count();
        if !picked.is_empty() && used + len > EXCERPT_BUDGET_CHARS {
            continue;
        }
        picked.push(*i);
        used += len;
        if used >= EXCERPT_BUDGET_CHARS {
            break;
        }
    }
    picked.sort_unstable();

    let mut out = String::new();
    if picked.first().is_some_and(|&i| i > 0) {
        out.push_str("[…]\n\n");
    }
    let mut prev: Option<usize> = None;
    for i in &picked {
        if let Some(p) = prev {
            out.push_str(if *i > p + 1 { "\n\n[…]\n\n" } else { "\n\n" });
        }
        out.push_str(chunks[*i].trim());
        prev = Some(*i);
    }
    if picked.last().is_some_and(|&i| i + 1 < chunks.len()) {
        out.push_str("\n\n[…]");
    }
    Some(format!(
        "(long note — showing the {} most relevant of {} sections)\n{out}",
        picked.len(),
        chunks.len()
    ))
}

/// Render one memory as an `[Mn] …` block entry with a human-readable
/// saved-age. Bodies within `FULL_BODY_CHARS` are VERBATIM (the no-truncation
/// rule for real captures); longer ones are query-aware excerpts so a single
/// giant vault note can't drown the model's context.
fn render_memory(m: usize, note: &Note, now_ms: i64, terms: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "[M{m}] saved {}",
        saved_line(note.timestamp, now_ms)
    ));
    if note.is_pinned {
        out.push_str(" · pinned");
    }
    out.push('\n');

    let has_meta = !note.title.trim().is_empty() || !note.tldr.trim().is_empty();
    if has_meta {
        let title = if note.title.trim().is_empty() {
            "(untitled)"
        } else {
            note.title.trim()
        };
        out.push_str(&format!("Title: {title}"));
        if !note.tldr.trim().is_empty() {
            out.push_str(&format!(" | Summary: {}", note.tldr.trim()));
        }
        out.push('\n');
    }

    // Todo state inline (the vision's "state, not documents").
    if !note.todo_tags.is_empty() {
        let todos: Vec<String> = note
            .todo_tags
            .iter()
            .map(|t| format!("[{}] {}", if t.done { "x" } else { " " }, t.text))
            .collect();
        out.push_str(&format!("Todos: {}\n", todos.join(", ")));
    }

    // Bodies within `FULL_BODY_CHARS` go to the model in FULL — truncating a
    // capture risks cutting the exact fact being asked about. Past that (long
    // foreign vault notes) the query decides which sections are worth the
    // context; the registry cap (`MAX_SESSION_MEMORIES`) bounds the rest.
    match excerpt_body(note.body.trim(), terms) {
        Some(excerpt) => out.push_str(&excerpt),
        None => out.push_str(note.body.trim()),
    }
    out
}

/// "2026-07-06 14:32 (yesterday)" — absolute plus a relative hint small models
/// read more reliably than raw timestamps.
fn saved_line(ts_ms: i64, now_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    let abs = match Local.timestamp_millis_opt(ts_ms) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => "unknown".to_string(),
    };
    format!("{abs} ({})", relative_age(ts_ms, now_ms))
}

/// Human relative age: "just now", "3 hours ago", "yesterday", "2 weeks ago", …
fn relative_age(ts_ms: i64, now_ms: i64) -> String {
    let diff = (now_ms - ts_ms).max(0);
    let mins = diff / 60_000;
    let hours = diff / 3_600_000;
    let days = diff / 86_400_000;
    if mins < 1 {
        "just now".to_string()
    } else if mins < 60 {
        format!("{mins} minute{} ago", plural(mins))
    } else if hours < 24 {
        format!("{hours} hour{} ago", plural(hours))
    } else if days == 1 {
        "yesterday".to_string()
    } else if days < 7 {
        format!("{days} days ago")
    } else if days < 30 {
        let w = days / 7;
        format!("{w} week{} ago", plural(w))
    } else if days < 365 {
        let mo = days / 30;
        format!("{mo} month{} ago", plural(mo))
    } else {
        let y = days / 365;
        format!("{y} year{} ago", plural(y))
    }
}

fn plural(n: i64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

// -- final-line parsing ---------------------------------------------------------

/// A conversational-write action the model requested via the `ACTION:` line
/// (RECALL-PLAN §7.2). The M-numbers reference memories in the current block.
#[derive(Debug, PartialEq)]
pub enum RecallAction {
    /// Merge the turn text into memory Mn (append/update → reconcile LLM pass).
    Reconcile { m: usize },
    /// Save the turn text as a brand-new memory.
    Remember,
    /// Mark todos (1-based indices) done in memory Mn — no LLM merge needed.
    Complete { m: usize, todos: Vec<usize> },
    /// Delete memory Mn — destructive, confirmed in-panel before it runs.
    Forget { m: usize },
}

/// What the model's trailing convention line told us. `sources`/`not_found` and
/// `action` are mutually exclusive per turn (a turn either answers or acts).
#[derive(Debug, Default, PartialEq)]
pub struct ParsedTail {
    pub sources: Vec<usize>,
    pub not_found: bool,
    pub action: Option<RecallAction>,
}

/// Split the answer's trailing `SOURCES:` / `NOT_FOUND` line off the display
/// text. Tolerant: an absent or malformed line just yields the whole text with
/// no sources and no not-found (never an error, never a retry).
pub fn parse_tail(reply: &str) -> (String, ParsedTail) {
    let trimmed = reply.trim_end();
    let Some(last_break) = trimmed.rfind('\n') else {
        // Single line: could itself be a bare NOT_FOUND (rare) but usually the
        // answer with no convention line — treat as pure answer.
        if trimmed.trim().eq_ignore_ascii_case("not_found") {
            return (
                String::new(),
                ParsedTail {
                    not_found: true,
                    ..Default::default()
                },
            );
        }
        return (trimmed.to_string(), ParsedTail::default());
    };
    let last = trimmed[last_break + 1..].trim();
    let lower = last.to_ascii_lowercase();

    if lower == "not_found" {
        let body = trimmed[..last_break].trim_end().to_string();
        return (
            body,
            ParsedTail {
                not_found: true,
                ..Default::default()
            },
        );
    }
    if let Some(rest) = lower.strip_prefix("action:") {
        let body = trimmed[..last_break].trim_end().to_string();
        return (
            body,
            ParsedTail {
                action: parse_action(rest),
                ..Default::default()
            },
        );
    }
    if let Some(rest) = lower.strip_prefix("sources:") {
        let sources = rest
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter_map(|tok| {
                let t = tok.trim();
                t.strip_prefix('m')
                    .or_else(|| t.strip_prefix('M'))
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .collect();
        let body = trimmed[..last_break].trim_end().to_string();
        return (
            body,
            ParsedTail {
                sources,
                ..Default::default()
            },
        );
    }
    // No recognized convention line — show the whole reply as-is.
    (trimmed.to_string(), ParsedTail::default())
}

/// Parse the payload after `ACTION:` (already lowercased) into a [`RecallAction`].
/// Tolerant of synonyms and phrasing; an unrecognized verb yields `None` (the
/// turn is then treated as a plain answer). Todo indices are read only from the
/// substring after the word "todo(s)" so an `Mn` number is never mistaken for one.
fn parse_action(rest: &str) -> Option<RecallAction> {
    let s = rest.trim();
    let verb = s.split_whitespace().next()?;
    let find_m = |s: &str| -> Option<usize> {
        s.split(|c: char| !c.is_ascii_alphanumeric())
            .find_map(|tok| tok.strip_prefix('m').and_then(|n| n.parse::<usize>().ok()))
    };
    match verb {
        "remember" | "save" | "create" | "new" => Some(RecallAction::Remember),
        "update" | "append" | "edit" | "change" | "add" | "merge" => {
            find_m(s).map(|m| RecallAction::Reconcile { m })
        }
        "forget" | "delete" | "remove" | "drop" => find_m(s).map(|m| RecallAction::Forget { m }),
        "complete" | "done" | "check" | "finish" => {
            let m = find_m(s)?;
            let todos: Vec<usize> = s
                .split("todo")
                .nth(1)
                .unwrap_or("")
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|t| t.parse::<usize>().ok())
                .collect();
            // No explicit indices → let the reconcile pass decide which todos.
            if todos.is_empty() {
                Some(RecallAction::Reconcile { m })
            } else {
                Some(RecallAction::Complete { m, todos })
            }
        }
        _ => None,
    }
}

// -- the turn -------------------------------------------------------------------

/// Run one Grain Recall turn: retrieve, synthesize, parse. Returns the display
/// answer (convention line stripped) plus its evidence sources and the
/// not-found signal (RECALL-PLAN §6) — the panel renders a footer from these.
pub async fn run_turn(app: &AppHandle, messages: &[AgentMessage]) -> Result<AgentReply, String> {
    if !super::is_enabled(app) {
        return Err("Grain Space is disabled".to_string());
    }
    let be = backend::resolve(app)?;

    let latest = messages
        .iter()
        .rev()
        .find(|m| m.role != "assistant")
        .map(|m| m.content.trim().to_string())
        .unwrap_or_default();
    if latest.is_empty() {
        return Err("Nothing was asked.".to_string());
    }

    // Empty-corpus fast path: no LLM call when there's nothing to recall. Uses
    // the WHOLE corpus (for the vault backend this includes the user's own
    // Obsidian notes, not just Grain-owned captures).
    let be_check = be.clone();
    let has_notes = tauri::async_runtime::spawn_blocking(move || backend::has_any_notes(&be_check))
        .await
        .map_err(|e| format!("recall scan join error: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
    if !has_notes {
        return Ok(AgentReply::plain(
            "You haven't saved any memories yet — capture one with your Grain Space shortcut, then ask me again."
                .to_string(),
        ));
    }

    // Initial (first-pass) retrieve — dual-stage 20→6, no date filter — folded
    // into the session registry (stable M-ids, unioned across follow-up turns).
    let hits = retrieve(app, &be, &latest, None)
        .await
        .map_err(|e| format!("{e:#}"))?;
    register_hits(app, &hits);

    // Build the first-pass memories block from the FULL session registry (fresh
    // read so earlier conversational edits are reflected).
    let registry = session_registry(app);
    let (block, _) = build_block_and_meta(&be, registry, &latest).await?;

    // Assemble the tool-enabled conversation: the memories are prepended to the
    // LATEST user message (NOT the system prompt) so the model attends to them
    // directly ("lost in the middle" mitigation). search_memory lets it look
    // again mid-turn if this first pass is wrong.
    let now = chrono::Local::now();
    let entries = build_entries(
        &now.format("%Y-%m-%d %H:%M").to_string(),
        &now.format("%A").to_string(),
        &block,
        messages,
    );

    let raw = run_tool_loop(app, &be, entries).await?;
    let (display, tail) = parse_tail(&raw);

    // Rebuild the source map from the FINAL registry (search_memory hops may
    // have added memories) so SOURCES / ACTION M-numbers resolve to notes.
    let final_registry = session_registry(app);
    let (_, source_meta) = build_block_and_meta(&be, final_registry, &latest).await?;

    log::info!(
        "[GRAIN] recall: answered ({} memories registered, sources={:?}, not_found={}, action={:?})",
        source_meta.len(),
        tail.sources,
        tail.not_found,
        tail.action
    );

    // An ACTION turn edits memory instead of answering; SOURCES and ACTION are
    // mutually exclusive, so we resolve one or the other. `forget` is the only
    // deferred case: it hands the panel a note to confirm before deletion.
    let mut sources: Vec<AgentSource> = Vec::new();
    let mut confirm_delete: Option<AgentSource> = None;
    if let Some(action) = &tail.action {
        // The previous Grain answer is context for anaphora ("the first two").
        let convo_context = messages
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        match action {
            RecallAction::Remember => {
                // Recall's "remember" doesn't auto-categorize (no folder list).
                let (note, _) = capture::compose_note(app, &latest, None, &[]).await;
                persist(app, &be, note).await;
            }
            RecallAction::Reconcile { m } => {
                if let Some(src) = source_meta.get(m) {
                    if let Some(current) = read_note(&be, &src.note_id).await {
                        let merged =
                            capture::reconcile_note(app, &current, &latest, &convo_context).await;
                        persist(app, &be, merged).await;
                    }
                }
            }
            RecallAction::Complete { m, todos } => {
                if let Some(src) = source_meta.get(m) {
                    if let Some(mut current) = read_note(&be, &src.note_id).await {
                        for i in todos {
                            if *i >= 1 {
                                if let Some(t) = current.todo_tags.get_mut(i - 1) {
                                    t.done = true;
                                }
                            }
                        }
                        persist(app, &be, current).await;
                    }
                }
            }
            RecallAction::Forget { m } => {
                // Destructive — do NOT delete here. Hand the memory to the panel
                // for an explicit in-place confirmation (RECALL-PLAN §7.2).
                confirm_delete = source_meta.get(m).cloned();
            }
        }
    } else {
        // Resolve cited M-numbers to evidence sources; unknown ids are dropped
        // (RECALL-PLAN §10) so a stray citation never fails the turn.
        sources = tail
            .sources
            .iter()
            .filter_map(|m| source_meta.get(m).cloned())
            .collect();
    }

    let text = if display.trim().is_empty() {
        // A bare NOT_FOUND (or empty action confirmation) — give a sentence.
        "I don't have a memory about that.".to_string()
    } else {
        display
    };
    Ok(AgentReply {
        text,
        sources,
        not_found: tail.not_found,
        confirm_delete,
    })
}

/// Read one note off the async runtime; `None` (logged) if it's gone/unreadable.
async fn read_note(be: &Backend, id: &str) -> Option<Note> {
    let be = be.clone();
    let id = id.to_string();
    match tauri::async_runtime::spawn_blocking(move || backend::get_note(&be, &id)).await {
        Ok(Ok(note)) => Some(note),
        Ok(Err(e)) => {
            log::warn!("[GRAIN] recall: note read failed: {e:#}");
            None
        }
        Err(e) => {
            log::warn!("[GRAIN] recall: note read join error: {e}");
            None
        }
    }
}

/// Save a note produced by a conversational write, then refresh the surfaces
/// (overlay + settings tab re-render on `notes-changed`; reminders re-sync in
/// case timing changed).
async fn persist(app: &AppHandle, be: &Backend, note: Note) {
    let be = be.clone();
    match tauri::async_runtime::spawn_blocking(move || backend::save_note(&be, &note)).await {
        Ok(Ok(())) => {
            super::emit_notes_changed(app);
            super::reminders::sync(app);
        }
        Ok(Err(e)) => log::error!("[GRAIN] recall: write save failed: {e:#}"),
        Err(e) => log::error!("[GRAIN] recall: write save join error: {e}"),
    }
}

/// System prompt + memories block (as a system message) + the conversation
/// Assemble the tool-enabled conversation: system prompt, the conversation
/// turns (roles normalized), and the memories block PREPENDED to the latest
/// user turn (not a system message — "lost in the middle" mitigation). The
/// block is folded into the user's own message so the model treats the facts as
/// part of what it's being asked about.
fn build_entries(
    now: &str,
    weekday: &str,
    block: &str,
    messages: &[AgentMessage],
) -> Vec<crate::llm_client::ChatEntry> {
    use crate::llm_client::ChatEntry;
    let mut entries: Vec<ChatEntry> = Vec::with_capacity(messages.len() + 1);
    entries.push(ChatEntry::System(system_prompt(now, weekday)));

    // Index of the last non-assistant (user) turn — the one we augment.
    let last_user = messages.iter().rposition(|m| m.role != "assistant");

    for (i, m) in messages.iter().enumerate() {
        let is_user = m.role != "assistant";
        if is_user {
            let content = if Some(i) == last_user {
                prepend_memories(block, &m.content)
            } else {
                m.content.clone()
            };
            entries.push(ChatEntry::User(content));
        } else {
            entries.push(ChatEntry::Assistant(m.content.clone()));
        }
    }
    entries
}

/// Prepend the retrieved memories to the user's message text.
fn prepend_memories(block: &str, user_msg: &str) -> String {
    let ctx = if block.trim().is_empty() {
        "Relevant saved memories: (none matched yet — call search_memory with different words \
         or a date range if you need to look further)."
            .to_string()
    } else {
        format!("Relevant saved memories (each tagged [Mn] for citation):\n\n{block}")
    };
    format!("{ctx}\n\n---\n\nMy message: {user_msg}")
}

/// The single tool Grain Recall exposes: `search_memory(query, minDate?,
/// maxDate?)`. One simple, well-described function keeps small edge models
/// consistent and token-efficient.
fn search_memory_spec() -> crate::llm_client::ToolSpec {
    crate::llm_client::ToolSpec {
        name: "search_memory".to_string(),
        description:
            "Search the user's saved memories for facts not already shown. Use this when the \
             memories prepended to the message don't contain what you need — the user changed \
             topic, referred to something not shown, or asked about a specific time. Returns \
             matching memories tagged [Mn] that you can then cite in SOURCES."
                .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Focused search terms — the key nouns/topic to look up."
                },
                "minDate": {
                    "type": "string",
                    "description": "Earliest saved date to include, as YYYY-MM-DD. Optional."
                },
                "maxDate": {
                    "type": "string",
                    "description": "Latest saved date to include, as YYYY-MM-DD. Optional."
                }
            },
            "required": ["query"]
        }),
    }
}

/// The bounded agentic loop (SEARCH-OVERHAUL S2/S3, native-tool form). Calls the
/// LLM with `search_memory` available; while it asks for tool calls (up to
/// `MAX_TOOL_HOPS`), execute each search, fold hits into the session registry,
/// feed results back, and re-ask. After the cap, force a direct answer with no
/// tools so the turn always terminates. Returns the final raw text.
async fn run_tool_loop(
    app: &AppHandle,
    be: &Backend,
    mut entries: Vec<crate::llm_client::ChatEntry>,
) -> Result<String, String> {
    use crate::llm_client::ChatEntry;

    let tools = vec![search_memory_spec()];
    let mut reply =
        crate::agent::run_messages_with_tools(app, entries.clone(), clone_tools(&tools)).await?;

    let mut hops = 0usize;
    while !reply.tool_calls.is_empty() && hops < MAX_TOOL_HOPS {
        hops += 1;
        entries.push(ChatEntry::AssistantToolCalls(reply.tool_calls.clone()));
        for tc in &reply.tool_calls {
            let result = execute_search_memory(app, be, tc).await;
            entries.push(ChatEntry::ToolResult {
                call_id: tc.id.clone(),
                content: result,
            });
        }
        reply = crate::agent::run_messages_with_tools(app, entries.clone(), clone_tools(&tools))
            .await?;
    }

    // Still wanting tools after the cap: `entries` ends cleanly on a tool
    // result, so nudge for a direct answer WITH NO TOOLS advertised (avoids a
    // dangling assistant tool-call the API would reject).
    if !reply.tool_calls.is_empty() {
        log::info!("[GRAIN] recall: tool hop cap ({MAX_TOOL_HOPS}) reached; forcing answer");
        entries.push(ChatEntry::User(
            "You've searched enough. Answer now using only the memories already provided; if the \
             fact genuinely isn't there, say so honestly."
                .to_string(),
        ));
        reply = crate::agent::run_messages_with_tools(app, entries, Vec::new()).await?;
    }

    Ok(reply.content)
}

/// Clone tool specs for a repeated round-trip (schema is tiny; active-turn only).
fn clone_tools(tools: &[crate::llm_client::ToolSpec]) -> Vec<crate::llm_client::ToolSpec> {
    tools
        .iter()
        .map(|t| crate::llm_client::ToolSpec {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        })
        .collect()
}

/// Execute one `search_memory` tool call: parse args, apply the optional date
/// pre-filter, dual-stage retrieve, fold hits into the session registry (stable
/// M-ids), and render them as `[Mn]` entries for the tool result. Never errors
/// — a bad-args or empty result just returns a short note the model can read.
async fn execute_search_memory(
    app: &AppHandle,
    be: &Backend,
    tc: &crate::llm_client::ToolCallOut,
) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&tc.arguments).unwrap_or(serde_json::Value::Null);
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let min_ms = args
        .get("minDate")
        .and_then(|v| v.as_str())
        .and_then(|s| parse_date_ms(s, false));
    let max_ms = args
        .get("maxDate")
        .and_then(|v| v.as_str())
        .and_then(|s| parse_date_ms(s, true));
    let range = match (min_ms, max_ms) {
        (None, None) => None,
        (lo, hi) => Some((lo.unwrap_or(0), hi.unwrap_or(i64::MAX))),
    };

    if query.is_empty() && range.is_none() {
        return "No search terms were given.".to_string();
    }

    log::info!("[GRAIN] recall: search_memory(query={query:?}, range={range:?})");

    let hits = match retrieve(app, be, &query, range).await {
        Ok(h) => h,
        Err(e) => {
            log::warn!("[GRAIN] recall: search_memory failed: {e:#}");
            return "The search could not be completed.".to_string();
        }
    };
    if hits.is_empty() {
        return "No saved memories matched that search.".to_string();
    }

    // Register (stable M-ids) and render with the assigned M-numbers so the
    // model can cite them exactly like the prepended block.
    let now_ms = chrono::Utc::now().timestamp_millis();
    let terms = query_terms(&query);
    let mut rendered: Vec<String> = Vec::with_capacity(hits.len());
    if let Some(state) = app.try_state::<crate::agent::AgentState>() {
        if let Ok(mut session) = state.recall.lock() {
            for note in &hits {
                if let Some(m) = session.register(&note.id) {
                    rendered.push(render_memory(m, note, now_ms, &terms));
                }
            }
        }
    }
    if rendered.is_empty() {
        return "No additional memories could be added this turn.".to_string();
    }
    format!("Found these memories:\n\n{}", rendered.join("\n\n"))
}

/// Parse a `YYYY-MM-DD` (or RFC3339) date into epoch ms in LOCAL time. When
/// `end_of_day`, snap to 23:59:59.999 so a `maxDate` window is inclusive.
fn parse_date_ms(s: &str, end_of_day: bool) -> Option<i64> {
    use chrono::{Local, NaiveDate, NaiveTime, TimeZone};
    let s = s.trim();
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let time = if end_of_day {
            NaiveTime::from_hms_milli_opt(23, 59, 59, 999)?
        } else {
            NaiveTime::from_hms_opt(0, 0, 0)?
        };
        let naive = date.and_time(time);
        return match Local.from_local_datetime(&naive).single() {
            Some(dt) => Some(dt.timestamp_millis()),
            None => Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|dt| dt.timestamp_millis()),
        };
    }
    // Fallback: full RFC3339 timestamp.
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// The best human label for a note's source chip: its title, else its summary,
/// else a plain-code title from the first words of the body (so raw/quick-add
/// notes with no metadata still get a readable chip instead of a blank one).
fn display_title(note: &Note) -> String {
    if !note.title.trim().is_empty() {
        note.title.trim().to_string()
    } else if !note.tldr.trim().is_empty() {
        note.tldr.trim().to_string()
    } else {
        capture::fallback_title(&note.body)
    }
}

/// Register a batch of retrieved notes into the session registry (stable
/// M-ids). No-op if the agent state or lock is unavailable.
fn register_hits(app: &AppHandle, hits: &[Note]) {
    if let Some(state) = app.try_state::<crate::agent::AgentState>() {
        if let Ok(mut session) = state.recall.lock() {
            for note in hits {
                let _ = session.register(&note.id);
            }
        }
    }
}

/// The session's ordered memory registry (note ids in M-order), or empty if
/// unavailable.
fn session_registry(app: &AppHandle) -> Vec<String> {
    app.try_state::<crate::agent::AgentState>()
        .and_then(|s| s.recall.lock().ok().map(|g| g.ordered()))
        .unwrap_or_default()
}

/// Build the memories block AND the M-number → source-meta map from a session
/// registry, in one off-runtime pass. SOURCES/ACTION cite M-numbers, so keying
/// by M-number (not list position) means an unreadable/skipped note never
/// renumbers the rest. `query` steers per-memory excerpting AND the block's
/// ORDER: entries are laid out weakest→strongest relevance so the best memory
/// sits adjacent to the user's message ("lost in the middle" — models attend
/// most to the edges of a context region; M-numbers are labels, not positions,
/// so reordering is free).
async fn build_block_and_meta(
    be: &Backend,
    registry: Vec<String>,
    query: &str,
) -> Result<(String, std::collections::HashMap<usize, AgentSource>), String> {
    let be_block = be.clone();
    let terms = query_terms(query);
    tauri::async_runtime::spawn_blocking(move || {
        use std::collections::HashMap;
        let now_ms = chrono::Utc::now().timestamp_millis();
        // (relevance, M, rendered) — sorted ascending before joining.
        let mut blocks: Vec<(f64, usize, String)> = Vec::new();
        let mut meta: HashMap<usize, AgentSource> = HashMap::new();
        for (i, id) in registry.iter().enumerate() {
            match backend::get_note(&be_block, id) {
                Ok(note) => {
                    let m = i + 1;
                    let relevance = term_overlap(&terms, &note);
                    blocks.push((relevance, m, render_memory(m, &note, now_ms, &terms)));
                    let title = display_title(&note);
                    meta.insert(
                        m,
                        AgentSource {
                            note_id: note.id.clone(),
                            title,
                            saved_at: note.timestamp,
                        },
                    );
                }
                Err(e) => log::warn!("[GRAIN] recall: memory {id} unreadable: {e:#}"),
            }
        }
        blocks.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        let joined = blocks
            .into_iter()
            .map(|(_, _, s)| s)
            .collect::<Vec<_>>()
            .join("\n\n");
        (joined, meta)
    })
    .await
    .map_err(|e| format!("recall block join error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain_space::note::Note;

    fn note(id: &str, ts: i64) -> Note {
        let mut n = Note::raw(format!("body of {id}"));
        n.id = id.to_string();
        n.timestamp = ts;
        n
    }

    #[test]
    fn rrf_fuses_and_ranks_overlap_first() {
        // b appears high in both lists → should win.
        let fts = vec![note("a", 1), note("b", 2), note("c", 3)];
        let sem = vec![note("b", 2), note("d", 4), note("a", 1)];
        let out = fuse(fts, sem, 4);
        assert_eq!(out[0].id, "b"); // in both, top ranks
                                    // a is in both too (rank2 + rank3) — beats c/d which are single-list.
        assert_eq!(out[1].id, "a");
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn fuse_dedupes_by_id() {
        let fts = vec![note("a", 1), note("a", 1)];
        let out = fuse(fts, vec![], 6);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn parse_tail_extracts_sources() {
        let (body, tail) = parse_tail("Superlist is the app.\nSOURCES: M2, M4");
        assert_eq!(body, "Superlist is the app.");
        assert_eq!(tail.sources, vec![2, 4]);
        assert!(!tail.not_found);
    }

    #[test]
    fn parse_tail_handles_sources_none_and_case() {
        let (body, tail) = parse_tail("An answer.\nsources: none");
        assert_eq!(body, "An answer.");
        assert!(tail.sources.is_empty());
        assert!(!tail.not_found);
    }

    #[test]
    fn parse_tail_detects_not_found() {
        let (body, tail) = parse_tail("I don't have a memory about that.\nNOT_FOUND");
        assert_eq!(body, "I don't have a memory about that.");
        assert!(tail.not_found);
        assert!(tail.sources.is_empty());
    }

    #[test]
    fn parse_tail_tolerates_missing_line() {
        let (body, tail) = parse_tail("Just an answer with no convention line.");
        assert_eq!(body, "Just an answer with no convention line.");
        assert_eq!(tail, ParsedTail::default());
    }

    #[test]
    fn parse_tail_extracts_actions() {
        let (body, tail) = parse_tail("Done — added the refactor.\nACTION: update M2");
        assert_eq!(body, "Done — added the refactor.");
        assert_eq!(tail.action, Some(RecallAction::Reconcile { m: 2 }));
        assert!(tail.sources.is_empty());

        let (_, tail) = parse_tail("Saved that.\naction: remember");
        assert_eq!(tail.action, Some(RecallAction::Remember));

        let (_, tail) = parse_tail("Delete the Rust note?\nACTION: forget M3");
        assert_eq!(tail.action, Some(RecallAction::Forget { m: 3 }));
    }

    #[test]
    fn parse_action_reads_todo_indices_not_the_m_number() {
        // "M2" must not leak its 2 into the todo list.
        assert_eq!(
            parse_action(" complete m2 todos 1,3 "),
            Some(RecallAction::Complete {
                m: 2,
                todos: vec![1, 3],
            })
        );
        // No explicit indices → defer to the reconcile pass.
        assert_eq!(
            parse_action("complete m2"),
            Some(RecallAction::Reconcile { m: 2 })
        );
        // Synonyms + unknown verbs.
        assert_eq!(
            parse_action("append m5"),
            Some(RecallAction::Reconcile { m: 5 })
        );
        assert_eq!(parse_action("frobnicate m1"), None);
    }

    #[test]
    fn session_registers_stable_m_ids() {
        let mut s = RecallSession::default();
        assert_eq!(s.register("a"), Some(1));
        assert_eq!(s.register("b"), Some(2));
        assert_eq!(s.register("a"), Some(1)); // stable
        assert_eq!(s.note_id_of(2), Some("b"));
        s.clear();
        assert_eq!(s.note_id_of(1), None);
    }

    #[test]
    fn session_caps_without_renumbering() {
        let mut s = RecallSession::default();
        for i in 0..MAX_SESSION_MEMORIES {
            assert!(s.register(&format!("n{i}")).is_some());
        }
        assert_eq!(s.register("overflow"), None); // full: not added
        assert_eq!(s.note_id_of(1), Some("n0")); // earliest still M1
    }

    #[test]
    fn relative_age_reads_naturally() {
        let now = 1_000_000_000_000;
        assert_eq!(relative_age(now, now), "just now");
        assert_eq!(relative_age(now - 5 * 60_000, now), "5 minutes ago");
        assert_eq!(relative_age(now - 86_400_000, now), "yesterday");
        assert_eq!(relative_age(now - 3 * 86_400_000, now), "3 days ago");
        assert_eq!(relative_age(now - 14 * 86_400_000, now), "2 weeks ago");
    }

    #[test]
    fn render_memory_keeps_normal_bodies_verbatim() {
        // No-truncation holds for anything a person actually dictates: bodies
        // within FULL_BODY_CHARS reach the block untouched.
        let mut n = note("normal", 1);
        n.body = "y".repeat(FULL_BODY_CHARS);
        let out = render_memory(1, &n, 2, &[]);
        assert!(out.contains(&"y".repeat(FULL_BODY_CHARS)));
        assert!(!out.contains("[…]"));
    }

    #[test]
    fn render_memory_excerpts_giant_bodies() {
        // A wall-of-text vault note must be excerpted, not shipped whole.
        let mut n = note("big", 1);
        n.body = "y".repeat(8_000);
        let out = render_memory(1, &n, 2, &[]);
        assert!(out.contains("long note"));
        assert!(out.len() < 4_000, "excerpt must respect the budget");
    }

    #[test]
    fn excerpt_picks_query_matching_sections() {
        // Sections split on blank lines; the query term sits deep in the note.
        // The excerpt must contain that section and mark the elided gap.
        let filler = "lorem ipsum dolor sit amet ".repeat(35); // ~945 chars
        let body = format!(
            "{filler}\n\n{filler}\n\n{filler}\n\n{filler}\n\nthe wifi password is interstellar\n\n{filler}"
        );
        let terms = query_terms("wifi password");
        let out = excerpt_body(&body, &terms).expect("long body must excerpt");
        assert!(
            out.contains("interstellar"),
            "matching section must survive"
        );
        assert!(out.contains("[…]"), "gaps must be marked");
        // Short bodies stay verbatim.
        assert!(excerpt_body("short note", &terms).is_none());
    }

    #[test]
    fn rerank_favours_term_overlap_over_raw_rrf() {
        use std::collections::HashMap;
        // Two candidates, equal recency (no decay). `hit` has the query term in
        // its title; `miss` has a slightly higher raw RRF. Overlap should lift
        // `hit` to the top.
        let mut hit = note("hit", 100);
        hit.title = "Wifi password".into();
        let miss = note("miss", 100);
        let pool = vec![(miss, 0.02_f64), (hit, 0.019_f64)];
        let out = rerank("what is the wifi password", pool, &HashMap::new(), 0, 6);
        assert_eq!(out[0].id, "hit");
    }

    #[test]
    fn rerank_recency_breaks_ties() {
        use std::collections::HashMap;
        // Identical RRF + no term overlap → newer note wins on recency.
        let older = note("older", 1_000);
        let newer = note("newer", 5_000_000_000_000);
        let pool = vec![(older, 0.01_f64), (newer, 0.01_f64)];
        let out = rerank("unrelated terms", pool, &HashMap::new(), 30, 6);
        assert_eq!(out[0].id, "newer");
    }

    #[test]
    fn rerank_semantic_evidence_outranks_equal_lexical() {
        use std::collections::HashMap;
        // Same RRF, same (zero) overlap, same recency — the exact stored-vector
        // cosine must decide. A candidate missing from sims scores neutral 0.5,
        // BELOW the strong match but ABOVE the poor one.
        let strong = note("strong", 100);
        let unknown = note("unknown", 100);
        let weak = note("weak", 100);
        let pool = vec![(weak, 0.01_f64), (unknown, 0.01_f64), (strong, 0.01_f64)];
        let sims = HashMap::from([
            ("strong".to_string(), 0.82_f64),
            ("weak".to_string(), 0.31_f64),
        ]);
        let out = rerank("query with no lexical hits", pool, &sims, 0, 6);
        assert_eq!(out[0].id, "strong");
        assert_eq!(out[1].id, "unknown");
        assert_eq!(out[2].id, "weak");
    }

    #[test]
    fn term_overlap_fraction() {
        let mut n = note("x", 1);
        n.title = "Home wifi".into();
        n.tldr = "network details".into();
        let terms = query_terms("home wifi password");
        // "home" + "wifi" present, "password" absent → 2/3.
        let frac = term_overlap(&terms, &n);
        assert!((frac - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn parse_date_ms_day_bounds() {
        let start = parse_date_ms("2026-06-15", false).unwrap();
        let end = parse_date_ms("2026-06-15", true).unwrap();
        assert!(end > start);
        // End-of-day is within the same 24h window as start.
        assert!(end - start < 86_400_000);
        assert!(end - start > 86_000_000);
        assert_eq!(parse_date_ms("not-a-date", false), None);
    }
}
