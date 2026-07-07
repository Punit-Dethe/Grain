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

use std::path::Path;

use anyhow::Result;
use tauri::{AppHandle, Manager};

use super::capture;
use super::store::{self, Note};
use crate::agent::{AgentMessage, AgentReply, AgentSource};

/// Memories fed to the model per turn (post-fusion). Recall over precision —
/// the model does the final filtering; an extra note costs a few hundred
/// tokens, a missing one costs the answer.
const TOP_K_PER_TURN: usize = 6;
/// Cap on the session's stable memory registry. M-ids never move within a
/// session, so we never evict (that would renumber): once full, new hits for
/// later turns simply aren't added to the block. Personal-scale, short
/// sessions — 10 distinct memories is ample and bounds the block ~4k tokens.
const MAX_SESSION_MEMORIES: usize = 10;
/// Per-memory body budget in the block (head-biased truncation).
const BODY_HEAD_CHARS: usize = 1200;
const BODY_TAIL_CHARS: usize = 300;
/// RRF constant (standard).
const RRF_K: f64 = 60.0;

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

/// The Grain Recall system prompt (v1 — iterate from real usage). Small models
/// need short imperative rules + one simple output convention.
fn system_prompt(now: &str, weekday: &str) -> String {
    format!(
        "You are Grain, the user's personal memory. You answer their questions using ONLY \
         their saved memories, listed in the next message. Current date/time: {now} ({weekday}).\n\
         Rules:\n\
         1. Answer directly in the first sentence — short, natural, conversational. The user is \
         mid-flow; no preamble, no headers, no markdown lists unless they ask for structure.\n\
         2. Use only the memories provided. If they don't contain the answer, say so plainly — \
         NEVER guess or invent details.\n\
         3. When memories conflict, trust the most recent one; mention the older value only if \
         the difference matters.\n\
         4. If more than one memory could be the answer, lead with the most likely and offer the \
         runner-up in one clause.\n\
         5. Ask at most ONE short clarifying question, and only when you genuinely cannot choose \
         between interpretations.\n\
         6. Each memory shows when it was saved; use that to resolve time references like \
         \"yesterday\" or \"back in June\".\n\
         7. End with exactly one line: `SOURCES: M2, M4` naming only the memories your answer \
         actually used. If you used none, write `SOURCES: none`.\n\
         8. If the thing the user is asking about is genuinely NOT among the memories (absent, \
         not merely thin), do not keep asking questions to fish for it. Give one honest sentence \
         and make your LAST line exactly `NOT_FOUND` (instead of the SOURCES line). Use this only \
         when you are confident the memory does not exist.\n\
         9. If the user asks you to CHANGE their memories, do it instead of just answering: reply \
         with a short confirmation and end with ONE action line IN PLACE OF the SOURCES line —\n\
         - add to or update a memory: `ACTION: update Mn`\n\
         - save something brand-new: `ACTION: remember`\n\
         - mark todos done in a memory: `ACTION: complete Mn todos 1,2`\n\
         - delete a memory: `ACTION: forget Mn`\n\
         10. For update/remember/complete, word your answer as a done-confirmation (\"Done — added \
         the parser refactor and marked the first two off.\"). For forget, do NOT confirm deletion; \
         instead ASK them to confirm it (\"Delete the 'Rust tasks' note?\") — the app will ask for a \
         final click.\n\
         11. Use an action line ONLY when the user clearly asks to change something; a plain \
         question always ends with SOURCES, never ACTION."
    )
}

// -- retrieval ------------------------------------------------------------------

/// Hybrid retrieve for one query: FTS ∪ semantic, fused with Reciprocal Rank
/// Fusion, top `TOP_K_PER_TURN`. Semantic is used only when enabled AND the
/// model is on disk; otherwise it degrades to FTS-only SILENTLY (recall must
/// work for users who never opted into the model). All store/embed work runs
/// off the async runtime.
async fn retrieve(app: &AppHandle, base: &Path, query: &str) -> Result<Vec<Note>> {
    let semantic_on = {
        let s = crate::settings::get_settings(app);
        s.grain_space_semantic && super::embed::model_on_disk()
    };
    let half_life_days = crate::settings::get_settings(app).grain_space_decay_half_life_days;

    let base_owned = base.to_path_buf();
    let query_owned = query.to_string();

    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
        let fts = store::search_notes(&base_owned, &query_owned)?;

        let semantic = if semantic_on {
            // Re-embed anything stale so semantic results reflect current
            // content (same batch the overlay's search path uses).
            let stale = store::stale_embed_texts(&base_owned)?;
            if !stale.is_empty() {
                let texts: Vec<String> = stale.iter().map(|(_, t)| t.clone()).collect();
                match super::embed::embed(texts) {
                    Ok(vectors) => {
                        let items: Vec<(String, Vec<f32>)> =
                            stale.into_iter().map(|(id, _)| id).zip(vectors).collect();
                        store::store_embeddings(&base_owned, &items)?;
                    }
                    Err(e) => {
                        // Embedding failed (e.g. model load error): fall back to
                        // FTS-only for this turn rather than failing the answer.
                        log::warn!("[GRAIN] recall: embed failed ({e:#}); FTS-only this turn");
                        return Ok(fts.into_iter().take(TOP_K_PER_TURN).collect());
                    }
                }
            }
            match super::embed::embed(vec![query_owned.clone()]) {
                Ok(mut v) => match v.pop() {
                    Some(q) => store::semantic_search(&base_owned, &q, half_life_days)?,
                    None => Vec::new(),
                },
                Err(e) => {
                    log::warn!("[GRAIN] recall: query embed failed ({e:#}); FTS-only");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(fuse(fts, semantic, TOP_K_PER_TURN))
    })
    .await?
}

/// Reciprocal Rank Fusion of two ranked note lists: `score(id) = Σ 1/(k+rank)`
/// (0-based rank, k=60). No score normalization needed. Returns the top `k`
/// notes; the semantic side already carries recency decay + pin exemption, so
/// we don't re-apply decay after fusion.
fn fuse(fts: Vec<Note>, semantic: Vec<Note>, k: usize) -> Vec<Note> {
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
    // Sort by fused score desc; break ties by newest timestamp so the order is
    // deterministic (HashMap iteration is not).
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
        .filter_map(|(id, _)| notes.remove(&id))
        .collect()
}

// -- memories block -------------------------------------------------------------

/// Render one memory as an `[Mn] …` block entry with a human-readable saved-age.
fn render_memory(m: usize, note: &Note, now_ms: i64) -> String {
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

    out.push_str(&truncate_body(&note.body));
    out
}

/// Head-biased body truncation — dictated notes put the point up front, so keep
/// the head and a little tail with an elision marker.
fn truncate_body(body: &str) -> String {
    let body = body.trim();
    let len = body.chars().count();
    if len <= BODY_HEAD_CHARS + BODY_TAIL_CHARS {
        return body.to_string();
    }
    let head: String = body.chars().take(BODY_HEAD_CHARS).collect();
    let tail: String = body.chars().skip(len - BODY_TAIL_CHARS).collect::<String>();
    format!("{head} […] {tail}")
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
    let base = super::base_dir(app).map_err(|e| e.to_string())?;

    let latest = messages
        .iter()
        .rev()
        .find(|m| m.role != "assistant")
        .map(|m| m.content.trim().to_string())
        .unwrap_or_default();
    if latest.is_empty() {
        return Err("Nothing was asked.".to_string());
    }

    // Empty-corpus fast path: no LLM call when there's nothing to recall.
    let base_check = base.clone();
    let total = tauri::async_runtime::spawn_blocking(move || store::list_notes(&base_check))
        .await
        .map_err(|e| format!("recall scan join error: {e}"))?
        .map_err(|e| format!("{e:#}"))?
        .len();
    if total == 0 {
        return Ok(AgentReply::plain(
            "You haven't saved any memories yet — capture one with your Grain Space shortcut, then ask me again."
                .to_string(),
        ));
    }

    // Retrieve this turn's hits and fold them into the session registry (stable
    // M-ids, unioned across follow-up turns).
    let hits = retrieve(app, &base, &latest)
        .await
        .map_err(|e| format!("{e:#}"))?;
    if let Some(state) = app.try_state::<crate::agent::AgentState>() {
        if let Ok(mut session) = state.recall.lock() {
            for note in &hits {
                let _ = session.register(&note.id);
            }
        }
    }

    // Build the memories block from the FULL session registry (re-read fresh so
    // conversational edits, later, are reflected). Missing notes are skipped.
    let registry = app
        .try_state::<crate::agent::AgentState>()
        .and_then(|s| s.recall.lock().ok().map(|g| g.ordered()))
        .unwrap_or_default();

    // Build the block AND a M-id → source-meta map in one pass: SOURCES on the
    // model's reply cite M-numbers, and resolving them to note titles/dates here
    // avoids a second DB read after the LLM call. Keyed by M-number so an
    // unreadable (skipped) note never renumbers the rest.
    let base_block = base.clone();
    let (block, source_meta) = tauri::async_runtime::spawn_blocking(move || {
        use std::collections::HashMap;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut entries: Vec<String> = Vec::new();
        let mut meta: HashMap<usize, AgentSource> = HashMap::new();
        for (i, id) in registry.iter().enumerate() {
            match store::get_note(&base_block, id) {
                Ok(note) => {
                    let m = i + 1;
                    entries.push(render_memory(m, &note, now_ms));
                    let title = if note.title.trim().is_empty() {
                        note.tldr.trim().to_string()
                    } else {
                        note.title.trim().to_string()
                    };
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
        (entries.join("\n\n"), meta)
    })
    .await
    .map_err(|e| format!("recall block join error: {e}"))?;

    // Assemble the LLM message list: system prompt + memories block + turns.
    let now = chrono::Local::now();
    let full = build_full(
        &now.format("%Y-%m-%d %H:%M").to_string(),
        &now.format("%A").to_string(),
        &block,
        messages,
    );

    let raw = crate::agent::run_messages(app, full).await?;
    let (display, tail) = parse_tail(&raw);
    log::info!(
        "[GRAIN] recall: answered ({} memories in block, sources={:?}, not_found={}, action={:?})",
        total.min(MAX_SESSION_MEMORIES),
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
                let note = capture::compose_note(app, &latest).await;
                persist(app, &base, note).await;
            }
            RecallAction::Reconcile { m } => {
                if let Some(src) = source_meta.get(m) {
                    if let Some(current) = read_note(&base, &src.note_id).await {
                        let merged =
                            capture::reconcile_note(app, &current, &latest, &convo_context).await;
                        persist(app, &base, merged).await;
                    }
                }
            }
            RecallAction::Complete { m, todos } => {
                if let Some(src) = source_meta.get(m) {
                    if let Some(mut current) = read_note(&base, &src.note_id).await {
                        for i in todos {
                            if *i >= 1 {
                                if let Some(t) = current.todo_tags.get_mut(i - 1) {
                                    t.done = true;
                                }
                            }
                        }
                        persist(app, &base, current).await;
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
async fn read_note(base: &Path, id: &str) -> Option<Note> {
    let base = base.to_path_buf();
    let id = id.to_string();
    match tauri::async_runtime::spawn_blocking(move || store::get_note(&base, &id)).await {
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
async fn persist(app: &AppHandle, base: &Path, note: Note) {
    let base = base.to_path_buf();
    match tauri::async_runtime::spawn_blocking(move || store::save_note(&base, &note)).await {
        Ok(Ok(())) => {
            super::emit_notes_changed(app);
            super::reminders::sync(app);
        }
        Ok(Err(e)) => log::error!("[GRAIN] recall: write save failed: {e:#}"),
        Err(e) => log::error!("[GRAIN] recall: write save join error: {e}"),
    }
}

/// System prompt + memories block (as a system message) + the conversation
/// turns (roles normalized). The memories block is re-sent fresh each turn so
/// the model always sees current content.
fn build_full(
    now: &str,
    weekday: &str,
    block: &str,
    messages: &[AgentMessage],
) -> Vec<(String, String)> {
    let mut full: Vec<(String, String)> = Vec::with_capacity(messages.len() + 2);
    full.push(("system".to_string(), system_prompt(now, weekday)));
    let block_msg = if block.trim().is_empty() {
        "MEMORIES:\n(none matched this query)".to_string()
    } else {
        format!("MEMORIES:\n{block}")
    };
    full.push(("system".to_string(), block_msg));
    for m in messages {
        let role = if m.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        full.push((role.to_string(), m.content.clone()));
    }
    full
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain_space::store::Note;

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
    fn truncate_body_keeps_head_and_tail() {
        let long = "x".repeat(2000);
        let out = truncate_body(&long);
        assert!(out.contains("[…]"));
        assert!(out.chars().count() < 2000);
    }
}
