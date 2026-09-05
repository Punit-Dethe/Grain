//! [GRAIN] Grain Space capture inputs.
//!
//! - **Input C (quick add):** tap shortcut → grab the current selection via the
//!   Agent's invisible-copy mechanism → save silently as a raw note.
//! - **Inputs A/B (voice):** the `grain_space_capture` binding records through
//!   the ordinary transcription pipeline; `actions.rs` hands the finished
//!   transcript to [`intake_transcript`] instead of pasting it. With a usable
//!   BYOM provider (Input A) one structured LLM call extracts title/TLDR/
//!   todos/reminder; on ANY failure it degrades to Input B (raw save) — a
//!   capture must never lose the user's words.

use std::sync::atomic::{AtomicI64, Ordering};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use super::note::{Note, ReminderState, ReminderStatus, TodoTag};
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};

/// Input C debounce: OS key-repeat / double taps within this window are one add.
const QUICK_ADD_DEBOUNCE_MS: i64 = 500;
static LAST_QUICK_ADD_MS: AtomicI64 = AtomicI64::new(0);

/// Metadata extraction only needs a REPRESENTATIVE sample of the body, not the
/// whole thing — an "astronomically huge" pasted selection would otherwise blow
/// the token budget / latency of the title-TLDR call. The full body is always
/// stored verbatim; only the LLM's metadata input is capped.
const META_SAMPLE_CHARS: usize = 4000;

/// A plain-code title from the first few words of the body. Used when there is
/// no usable LLM (Input B / quick-add) or extraction fails, so a note — and the
/// Recall source chip that cites it — is never blank. No network, no model.
pub(crate) fn fallback_title(body: &str) -> String {
    let stripped = body.trim_start();
    let cleaned = if stripped.starts_with('#') {
        stripped.trim_start_matches('#').trim_start()
    } else if let Some(rest) = stripped.strip_prefix("- [ ]") {
        rest.trim_start()
    } else if let Some(rest) = stripped.strip_prefix("- [x]") {
        rest.trim_start()
    } else if let Some(rest) = stripped.strip_prefix("-") {
        rest.trim_start()
    } else if let Some(rest) = stripped.strip_prefix("*") {
        rest.trim_start()
    } else if let Some(rest) = stripped.strip_prefix(">") {
        rest.trim_start()
    } else {
        stripped
    };

    let title: String = cleaned
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    // Trim trailing punctuation so "Buy milk," → "Buy milk".
    let title = title
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .trim();
    // Guard against a single pathological word (e.g. a giant URL/token).
    title.chars().take(48).collect()
}

/// A capped, representative slice of the body for the metadata LLM call.
fn sample_for_meta(body: &str) -> String {
    if body.chars().count() <= META_SAMPLE_CHARS {
        body.to_string()
    } else {
        body.chars().take(META_SAMPLE_CHARS).collect()
    }
}

/// Input C: capture the highlighted text and save it silently as a raw note.
/// Runs the whole capture off the input thread (the selection grab sleeps/polls
/// the clipboard). Empty selection ⇒ silent no-op — never an empty note.
pub fn quick_add(app: &AppHandle) {
    if !super::is_enabled(app) {
        return;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let last = LAST_QUICK_ADD_MS.swap(now, Ordering::Relaxed);
    if now - last < QUICK_ADD_DEBOUNCE_MS {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let Some(selection) = crate::agent::capture_selection(&app) else {
            log::debug!("[GRAIN] space quick-add: no selection captured; ignoring");
            return;
        };
        let backend = match super::backend::resolve(&app) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[GRAIN] space quick-add: {e}");
                return;
            }
        };
        let mut note = Note::raw(selection);
        // No LLM on the quick-add path — give it a plain-code title so the note
        // and its future source chip aren't blank.
        note.title = fallback_title(&note.body);
        match super::backend::save_note(&backend, &note) {
            Ok(()) => {
                log::info!("[GRAIN] space quick-add: saved note {}", note.id);
                super::emit_notes_changed(&app);
            }
            Err(e) => log::error!("[GRAIN] space quick-add: save failed: {e:#}"),
        }
    });
}

/// Inputs A/B (note capture): the user summoned the Agent pill in Capture mode,
/// then spoke or typed. This turns that into a saved note, HEADLESS — no panel,
/// no confirmation surface (the app confirms the save a different way). The
/// `selection` (captured at summon) is the note body when present — the user
/// selected some text and their spoken/typed words FRAME it; otherwise the
/// spoken/typed text IS the note. The body is always verbatim (never rewritten);
/// the LLM only supplies metadata, and any failure degrades to a raw save.
///
/// Returns `Ok(true)` when a note was actually saved (the caller shows the
/// in-card "Saved" confirmation), `Ok(false)` when there was nothing to save.
///
/// `title_override` is an explicit user-typed note title (the two-field note
/// card); when present and non-empty it wins over the auto-generated title.
pub async fn capture_and_save(
    app: &AppHandle,
    instruction: &str,
    selection: Option<&str>,
    title_override: Option<&str>,
) -> Result<bool, String> {
    if !super::is_enabled(app) {
        return Err("Grain Space is disabled".to_string());
    }

    let instruction = instruction.trim();
    let selection = selection.map(str::trim).filter(|s| !s.is_empty());

    // Selection present → it's the note body, the instruction frames it.
    // No selection → the spoken/typed text is the note.
    let (body, framing) = match selection {
        Some(sel) => (
            sel.to_string(),
            if instruction.is_empty() {
                None
            } else {
                Some(instruction)
            },
        ),
        None => (instruction.to_string(), None),
    };
    if body.trim().is_empty() {
        // Nothing was heard/typed and nothing selected — silent no-op.
        return Ok(false);
    }

    let backend = super::backend::resolve(app)?;
    // The capture origin: a saved selection is something the user READ, a bare
    // dictation is something they SAID. Worth telling apart at retrieval time.
    let source = if selection.is_some() {
        "selection"
    } else {
        "dictation"
    };
    let (mut note, relations) = compose_note(app, &body, framing, source).await;
    // An explicit typed title wins over the auto-generated one (kept short).
    if let Some(t) = title_override.map(str::trim).filter(|t| !t.is_empty()) {
        note.title = t.chars().take(80).collect();
    }
    let id = note.id.clone();

    let app2 = app.clone();
    let be_save = backend.clone();
    let saved =
        tauri::async_runtime::spawn_blocking(move || super::backend::save_note(&be_save, &note))
            .await;
    match saved {
        Ok(Ok(())) => {
            // Typed relations are index-level, so they land after the file write
            // (the note's own entities went in with its frontmatter). Best-effort:
            // the note is already safe on disk, and a thinner graph is never a
            // reason to report a failed capture.
            if !relations.is_empty() {
                let be_graph = backend.clone();
                if let Ok(Err(e)) = tauri::async_runtime::spawn_blocking(move || {
                    super::backend::record_relations(&be_graph, &relations)
                })
                .await
                {
                    log::warn!("[GRAIN] space capture: graph relations not recorded: {e:#}");
                }
            }
            super::emit_notes_changed(&app2);
            // Capture may have armed a reminder.
            super::reminders::sync(&app2);
            log::info!("[GRAIN] space capture: saved note {id}");
            Ok(true)
        }
        Ok(Err(e)) => {
            log::error!("[GRAIN] space capture: save failed: {e:#}");
            Err("Couldn't save the note.".to_string())
        }
        Err(e) => {
            log::error!("[GRAIN] space capture: save task panicked: {e}");
            Err("Couldn't save the note.".to_string())
        }
    }
}

/// Input A is available iff post-processing has an HTTP provider with a model.
/// Apple Intelligence (no OpenAI-style structured output path here) degrades to
/// Input B for now.
fn llm_usable(settings: &AppSettings) -> bool {
    if !settings.post_process_enabled {
        return false;
    }
    let Some(provider) = settings.active_post_process_provider() else {
        return false;
    };
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        return false;
    }
    settings
        .post_process_models
        .get(&provider.id)
        .map(|m| !m.trim().is_empty())
        .unwrap_or(false)
}

/// The structured-output shape for the extraction call.
#[derive(Deserialize, Debug, Default)]
struct ExtractedMeta {
    title: String,
    tldr: String,
    #[serde(default)]
    todos: Vec<String>,
    /// Local time "YYYY-MM-DDTHH:MM"; empty string when the note has no
    /// reminder/timer.
    #[serde(default)]
    reminder_at: String,
    /// [GRAIN] Distillation (KNOWLEDGE-ARCHITECTURE-PLAN.md D2): the searchable
    /// question — one sentence someone would actually go looking with. This is
    /// what gets embedded (D3), which is the single measured accuracy win in the
    /// Cerebras write-up: distil, then embed.
    #[serde(default)]
    question: String,
    /// Entities named in the note. LightRAG's `Recog` step, riding this call
    /// instead of a second LLM pass — which is the whole reason a graph is
    /// affordable here.
    #[serde(default)]
    entities: Vec<ExtractedEntity>,
    /// How those entities relate. LightRAG's edges, same free ride.
    #[serde(default)]
    relations: Vec<ExtractedRelation>,
}

/// Entity kinds we accept. Anything else collapses to `topic` — a taxonomy the
/// model can't corrupt.
const ENTITY_KINDS: [&str; 6] = ["person", "app", "file", "project", "topic", "place"];

#[derive(Deserialize, Debug, Default)]
struct ExtractedEntity {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
}

#[derive(Deserialize, Debug, Default)]
struct ExtractedRelation {
    #[serde(default)]
    from: String,
    #[serde(default)]
    pred: String,
    #[serde(default)]
    to: String,
}

/// Hard caps, enforced in Rust rather than hoped for in the prompt. A model that
/// returns 400 entities or a 5 KB name cannot bloat the graph or the note file.
const MAX_ENTITIES: usize = 12;
const MAX_RELATIONS: usize = 8;
const MAX_ENTITY_NAME_CHARS: usize = 64;
const MAX_QUESTION_CHARS: usize = 240;
const MAX_TODOS: usize = 32;
const MAX_TODO_CHARS: usize = 512;

/// Normalize one entity name: trim, collapse whitespace, cap length. Empty when
/// nothing usable is left.
fn clean_entity_name(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_ENTITY_NAME_CHARS).collect()
}

/// The dedup key for an entity (LightRAG's `Dedupe`, applied at both ends —
/// here and as a UNIQUE column in the index).
pub(crate) fn entity_norm(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// One extracted relation, cleaned and capped. Public so the graph writer shares
/// exactly the same normalization the note file was written with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Relation {
    pub from: String,
    pub pred: String,
    pub to: String,
}

/// Validate and cap the extracted entity list: normalized names, known kinds,
/// deduplicated by norm, first-come order preserved.
fn clean_entities(raw: Vec<ExtractedEntity>) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in raw {
        let name = clean_entity_name(&e.name);
        if name.is_empty() {
            continue;
        }
        let norm = entity_norm(&name);
        if !seen.insert(norm) {
            continue;
        }
        let kind = e.kind.trim().to_ascii_lowercase();
        let kind = if ENTITY_KINDS.contains(&kind.as_str()) {
            kind
        } else {
            "topic".to_string()
        };
        out.push((name, kind));
        if out.len() >= MAX_ENTITIES {
            break;
        }
    }
    out
}

/// Validate and cap relations. A relation is kept only when BOTH ends survived
/// entity cleaning — an edge to something we never recorded is a dangling
/// pointer, and a self-edge carries no information.
fn clean_relations(raw: Vec<ExtractedRelation>, entities: &[(String, String)]) -> Vec<Relation> {
    let known: std::collections::HashMap<String, &str> = entities
        .iter()
        .map(|(name, _)| (entity_norm(name), name.as_str()))
        .collect();
    let mut out: Vec<Relation> = Vec::new();
    for r in raw {
        let from = known.get(&entity_norm(&clean_entity_name(&r.from)));
        let to = known.get(&entity_norm(&clean_entity_name(&r.to)));
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        if entity_norm(from) == entity_norm(to) {
            continue;
        }
        let pred = clean_entity_name(&r.pred).to_lowercase().replace(' ', "_");
        if pred.is_empty() {
            continue;
        }
        let rel = Relation {
            from: from.to_string(),
            pred,
            to: to.to_string(),
        };
        if out.contains(&rel) {
            continue;
        }
        out.push(rel);
        if out.len() >= MAX_RELATIONS {
            break;
        }
    }
    out
}

impl ExtractedMeta {
    /// Apply the metadata to the note, returning the cleaned relations — which
    /// belong to the derived graph, not the note file (a note records the
    /// entities it names; how they connect is index-level and rebuildable from
    /// re-capture, so keeping it out of frontmatter keeps the file readable).
    fn apply(self, note: &mut Note, auto_arm: bool) -> Vec<Relation> {
        note.title = self.title.trim().to_string();
        note.tldr = self.tldr.trim().to_string();
        note.question = self
            .question
            .trim()
            .chars()
            .take(MAX_QUESTION_CHARS)
            .collect();
        let entities = clean_entities(self.entities);
        let relations = clean_relations(self.relations, &entities);
        note.entities = entities.into_iter().map(|(name, _)| name).collect();
        note.todo_tags = self
            .todos
            .into_iter()
            .map(|t| t.trim().chars().take(MAX_TODO_CHARS).collect::<String>())
            .filter(|t| !t.is_empty())
            .map(|text| TodoTag { text, done: false })
            .take(MAX_TODOS)
            .collect();

        let fire_at = parse_local_datetime_ms(self.reminder_at.trim());
        note.reminder_state = match fire_at {
            Some(ms) => ReminderState {
                // Auto-reminders off ⇒ extracted but not armed; the note pane
                // (Phase 3) offers a manual "arm" button.
                status: if auto_arm {
                    ReminderStatus::Armed
                } else {
                    ReminderStatus::Pending
                },
                fire_at: Some(ms),
            },
            None => ReminderState::default(),
        };
        relations
    }
}

/// "YYYY-MM-DDTHH:MM" (local wall clock, as instructed in the prompt) → epoch ms.
fn parse_local_datetime_ms(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    use chrono::TimeZone;
    match chrono::Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        chrono::LocalResult::None => None,
    }
}

/// One structured chat-completion against the active post-process provider.
/// `body` is the verbatim note text (never rewritten). `framing`, when present,
/// is the user's spoken/typed instruction about a SELECTION they're saving
/// (e.g. "reference for my essay") — it shapes the title/summary only.
async fn extract_metadata(
    app: &AppHandle,
    settings: &AppSettings,
    body: &str,
    framing: Option<&str>,
) -> Result<ExtractedMeta, String> {
    let provider = settings
        .active_post_process_provider()
        .cloned()
        .ok_or("no active provider")?;
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let client = app
        .try_state::<reqwest::Client>()
        .map(|s| s.inner().clone())
        .ok_or("shared HTTP client unavailable")?;

    let now_local = chrono::Local::now().format("%A %Y-%m-%dT%H:%M").to_string();

    let framing_line = match framing {
        Some(f) if !f.trim().is_empty() => format!(
            "\nThe user selected the note text and, to say what it is for, added: \"{}\". Use that \
             to shape the title and summary (what the note is FOR); do NOT add it to the note text.",
            f.trim()
        ),
        _ => String::new(),
    };
    let intro =
        "You extract metadata from a personal note the user is saving. Reply with JSON only.";
    let system_prompt = format!(
        "{intro}{framing_line}\n\
         Rules:\n\
         - title: at most 3 words, plain text.\n\
         - tldr: exactly one short sentence.\n\
         - todos: action items present in the note (empty array if none).\n\
         - question: the ONE question this note is the answer to, phrased as the \
           user would actually search for it later (\"why did token refresh fail on \
           large payloads?\"). Use the note's own words for specifics. One sentence, \
           no preamble. If the note answers nothing (a bare reminder, a phone \
           number), use an empty string.\n\
         - entities: the specific things this note is ABOUT — people, apps, files, \
           projects, places, topics — copied as they appear in the note, each with \
           its kind (person|app|file|project|topic|place). At most 12, most \
           important first. Only what is actually named: no invented categories, \
           no generic words like \"work\" or \"meeting\" unless the note names them \
           as a thing. Empty array is correct for a note with no clear subjects.\n\
         - relations: how those entities connect, as {{from, pred, to}} using ONLY \
           names from `entities`. `pred` is a short lowercase verb phrase \
           (used_for, blocked_by, owns, part_of, fixed_in). At most 8, and only \
           relations the note actually states — never inferred from general \
           knowledge. Empty array when the note states none.\n\
         - reminder_at: if a reminder/timer is requested, the local datetime it should fire as \
           YYYY-MM-DDTHH:MM; otherwise an empty string. The current local datetime is {now_local}.\
         {verbatim_tail}",
        verbatim_tail =
            "\nNever rewrite or summarize away the note itself — you only produce metadata."
    );

    let properties = serde_json::json!({
        "title": { "type": "string", "maxLength": 80 },
        "tldr": { "type": "string", "maxLength": 240 },
        "todos": {
            "type": "array",
            "maxItems": MAX_TODOS,
            "items": { "type": "string", "maxLength": MAX_TODO_CHARS }
        },
        "reminder_at": { "type": "string", "maxLength": 19 },
        // Distillation (D2/D3). `kind` is an enum so the taxonomy is fixed by the
        // schema rather than by the prompt; Rust still re-validates on the way in.
        "question": { "type": "string", "maxLength": MAX_QUESTION_CHARS },
        "entities": {
            "type": "array",
            "maxItems": MAX_ENTITIES,
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "maxLength": MAX_ENTITY_NAME_CHARS },
                    "kind": { "type": "string", "enum": ENTITY_KINDS }
                },
                "required": ["name", "kind"],
                "additionalProperties": false
            }
        },
        "relations": {
            "type": "array",
            "maxItems": MAX_RELATIONS,
            "items": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "maxLength": MAX_ENTITY_NAME_CHARS },
                    "pred": { "type": "string", "maxLength": MAX_ENTITY_NAME_CHARS },
                    "to": { "type": "string", "maxLength": MAX_ENTITY_NAME_CHARS }
                },
                "required": ["from", "pred", "to"],
                "additionalProperties": false
            }
        }
    });
    let required = vec![
        "title",
        "tldr",
        "todos",
        "reminder_at",
        "question",
        "entities",
        "relations",
    ];
    let schema = serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    });

    let success = crate::llm_client::send_chat_completion_with_schema(
        &client,
        &provider,
        api_key,
        &model,
        body.to_string(),
        Some(system_prompt),
        Some(schema),
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let content = success.content.ok_or("empty completion")?;
    let meta: ExtractedMeta =
        serde_json::from_str(strip_code_fences(&content)).map_err(|e| e.to_string())?;
    crate::post_process_router::record_usage(app, &provider.id);
    Ok(meta)
}

/// Deterministic safe append: preserves all old body text byte-for-byte,
/// appending the change with the readable separator.
#[cfg(test)]
pub(crate) fn raw_append(current: &Note, change: &str) -> Note {
    let mut note = current.clone();
    let change = change.trim();
    if change.is_empty() {
        return note;
    }
    if note.body.is_empty() {
        note.body = change.to_string();
    } else if note.body.ends_with("\n\n") {
        note.body.push_str("---\n\n");
        note.body.push_str(change);
    } else if note.body.ends_with('\n') {
        note.body.push_str("\n---\n\n");
        note.body.push_str(change);
    } else {
        note.body.push_str("\n\n---\n\n");
        note.body.push_str(change);
    }
    note
}

/// Build a note from freshly-captured text: verbatim `body` + one metadata
/// extraction. `framing` (a spoken/typed instruction about a saved selection)
/// shapes the title/summary only. Degrades to a raw note on any extraction
/// failure — the body is always preserved. Shared by the `remember` action and
/// note capture; does NOT save.
///
/// `source` is the capture origin recorded on the note (`dictation`,
/// `selection`, …). The returned relations are the graph edges the same call
/// extracted (D2) — they belong to the derived index, not the note file.
pub(crate) async fn compose_note(
    app: &AppHandle,
    body: &str,
    framing: Option<&str>,
    source: &str,
) -> (Note, Vec<Relation>) {
    let mut note = Note::raw(body.trim().to_string());
    note.source = source.to_string();
    let mut relations = Vec::new();
    let settings = get_settings(app);
    if llm_usable(&settings) {
        // Only a capped sample of a huge body is sent for metadata; the note
        // body itself (set above) remains the durable source of truth.
        match extract_metadata(app, &settings, &sample_for_meta(body.trim()), framing).await {
            Ok(meta) => {
                relations = meta.apply(&mut note, settings.grain_space_auto_reminders);
            }
            Err(e) => log::warn!("[GRAIN] space compose: extraction failed ({e}); raw note"),
        }
    }
    // Metadata off, unavailable, or a weak completion that left the title blank:
    // fall back to a plain-code title so the note is never untitled.
    if note.title.trim().is_empty() {
        note.title = fallback_title(&note.body);
    }
    // Enforce hard bounds on presentation fields (Section 6 quality requirements).
    note.title = note.title.split_whitespace().collect::<Vec<_>>().join(" ");
    note.title = note.title.chars().take(80).collect();
    if note.title.is_empty() {
        note.title = fallback_title(&note.body);
    }
    note.tldr = note.tldr.trim().chars().take(240).collect();
    (note, relations)
}

/// Some models fence JSON in ```json blocks even under structured output.
fn strip_code_fences(s: &str) -> &str {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grain_space::content_version_hash;

    #[test]
    fn metadata_apply_arms_or_parks_reminder() {
        let meta = |auto: bool| {
            let m = ExtractedMeta {
                title: " Wifi Note ".into(),
                tldr: "The wifi password.".into(),
                todos: vec!["buy milk".into(), "  ".into()],
                reminder_at: "2026-07-06T18:30".into(),
                ..Default::default()
            };
            let mut note = Note::raw("body".into());
            m.apply(&mut note, auto);
            note
        };

        let armed = meta(true);
        assert_eq!(armed.title, "Wifi Note");
        assert_eq!(armed.todo_tags.len(), 1);
        assert_eq!(armed.reminder_state.status, ReminderStatus::Armed);
        assert!(armed.reminder_state.fire_at.is_some());
        // Body is never touched by extraction.
        assert_eq!(armed.body, "body");

        let parked = meta(false);
        assert_eq!(parked.reminder_state.status, ReminderStatus::Pending);
    }

    #[test]
    fn metadata_todos_are_bounded_before_persistence() {
        let mut note = Note::raw("body".into());
        let meta = ExtractedMeta {
            todos: (0..(MAX_TODOS + 10))
                .map(|_| "界".repeat(MAX_TODO_CHARS + 10))
                .collect(),
            ..Default::default()
        };
        meta.apply(&mut note, false);
        assert_eq!(note.todo_tags.len(), MAX_TODOS);
        assert!(note
            .todo_tags
            .iter()
            .all(|todo| todo.text.chars().count() == MAX_TODO_CHARS));
    }

    #[test]
    fn bad_reminder_string_means_no_reminder() {
        let m = ExtractedMeta {
            title: "T".into(),
            tldr: "S".into(),
            reminder_at: "tomorrow evening".into(),
            ..Default::default()
        };
        let mut note = Note::raw("b".into());
        m.apply(&mut note, true);
        assert_eq!(note.reminder_state.status, ReminderStatus::None);
        assert!(note.reminder_state.fire_at.is_none());
    }

    fn ent(name: &str, kind: &str) -> ExtractedEntity {
        ExtractedEntity {
            name: name.into(),
            kind: kind.into(),
        }
    }

    fn rel(from: &str, pred: &str, to: &str) -> ExtractedRelation {
        ExtractedRelation {
            from: from.into(),
            pred: pred.into(),
            to: to.into(),
        }
    }

    /// The caps and taxonomy are enforced in RUST, not by asking the prompt
    /// nicely. A model that returns 400 entities, 5 KB names or an invented kind
    /// must not be able to bloat or corrupt the graph.
    #[test]
    fn distillation_is_capped_and_normalized_in_rust() {
        let mut raw: Vec<ExtractedEntity> = (0..40)
            .map(|i| ent(&format!("thing {i}"), "topic"))
            .collect();
        raw.push(ent(&"x".repeat(500), "topic"));
        raw.push(ent("  spaced   out  ", "WEIRD-KIND"));
        raw.push(ent("", "person"));
        let cleaned = clean_entities(raw);
        assert_eq!(cleaned.len(), MAX_ENTITIES, "hard cap on entity count");
        assert!(cleaned
            .iter()
            .all(|(n, _)| n.chars().count() <= MAX_ENTITY_NAME_CHARS));
        assert!(
            cleaned
                .iter()
                .all(|(_, k)| ENTITY_KINDS.contains(&k.as_str())),
            "an unknown kind collapses to a known one"
        );

        // Deduplication is by norm (LightRAG's Dedupe), so case and spacing
        // variants of one entity collapse to a single node.
        let cleaned = clean_entities(vec![
            ent("auth.py", "file"),
            ent("Auth.PY", "file"),
            ent("  auth.py ", "file"),
        ]);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0].0, "auth.py", "the first spelling wins");

        // A blank name is dropped, and whitespace is collapsed.
        let cleaned = clean_entities(vec![ent("  spaced   out  ", "topic")]);
        assert_eq!(cleaned[0].0, "spaced out");
    }

    /// An edge is only kept when BOTH ends are entities we actually recorded —
    /// otherwise the graph accumulates dangling pointers that retrieval would
    /// then walk into.
    #[test]
    fn relations_must_land_on_recorded_entities() {
        let entities = clean_entities(vec![ent("JWT", "topic"), ent("auth.py", "file")]);
        let relations = clean_relations(
            vec![
                rel("JWT", "used for", "auth.py"),
                // Unknown endpoint — dropped.
                rel("JWT", "used_for", "Postgres"),
                // Self-edge — no information.
                rel("JWT", "same_as", "jwt"),
                // Empty predicate — dropped.
                rel("JWT", "  ", "auth.py"),
                // Exact duplicate of the first — dropped.
                rel("jwt", "used_for", "AUTH.PY"),
            ],
            &entities,
        );
        assert_eq!(relations.len(), 1, "{relations:?}");
        assert_eq!(
            relations[0],
            Relation {
                from: "JWT".into(),
                pred: "used_for".into(),
                to: "auth.py".into(),
            },
            "predicates normalize to snake_case; endpoints keep their spelling"
        );
    }

    /// Distillation must reach the note, and the relations must NOT — they are
    /// index-level, and a note file stays something a human reads.
    #[test]
    fn apply_puts_distillation_on_the_note_and_returns_the_edges() {
        let m = ExtractedMeta {
            title: "Auth Bug".into(),
            tldr: "Raised the buffer.".into(),
            question: "  why did token refresh fail?  ".into(),
            entities: vec![ent("auth.py", "file"), ent("JWT", "topic")],
            relations: vec![rel("JWT", "used_for", "auth.py")],
            ..Default::default()
        };
        let mut note = Note::raw("body".into());
        let relations = m.apply(&mut note, false);
        assert_eq!(note.question, "why did token refresh fail?");
        assert_eq!(note.entities, vec!["auth.py", "JWT"]);
        assert_eq!(relations.len(), 1);
        assert_eq!(note.body, "body", "extraction never touches the body");
    }

    #[test]
    fn code_fences_are_stripped() {
        assert_eq!(strip_code_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn fallback_title_uses_first_words() {
        assert_eq!(fallback_title("buy milk, eggs and bread"), "buy milk, eggs");
        assert_eq!(fallback_title("done."), "done");
        assert_eq!(
            fallback_title("  wifi password is hunter2 "),
            "wifi password is"
        );
        assert_eq!(fallback_title(""), "");
        // A single pathological token is capped, never unbounded.
        assert!(fallback_title(&"x".repeat(500)).chars().count() <= 48);
    }

    #[test]
    fn sample_for_meta_caps_huge_bodies() {
        let huge = "y".repeat(META_SAMPLE_CHARS * 3);
        assert_eq!(sample_for_meta(&huge).chars().count(), META_SAMPLE_CHARS);
        let small = "short body";
        assert_eq!(sample_for_meta(small), small);
    }

    #[test]
    fn raw_append_preserves_and_appends() {
        let mut cur = Note::raw("original".into());
        cur.title = "Keep Me".into();
        let out = raw_append(&cur, "  more info  ");
        assert_eq!(out.body, "original\n\n---\n\nmore info");
        assert_eq!(out.title, "Keep Me"); // untouched
        assert_eq!(out.id, cur.id); // identity preserved

        let empty = Note::raw("".into());
        assert_eq!(raw_append(&empty, "first").body, "first");

        let with_newline = Note::raw("line1\n".into());
        assert_eq!(
            raw_append(&with_newline, "line2").body,
            "line1\n\n---\n\nline2"
        );

        let with_double_newline = Note::raw("line1\n\n".into());
        assert_eq!(
            raw_append(&with_double_newline, "line2").body,
            "line1\n\n---\n\nline2"
        );
    }

    #[test]
    fn fallback_title_cleans_markdown_prefixes() {
        assert_eq!(fallback_title("# Hello World today"), "Hello World today");
        assert_eq!(
            fallback_title("## Meeting Notes for Q3"),
            "Meeting Notes for"
        );
        assert_eq!(fallback_title("- [ ] buy milk and eggs"), "buy milk and");
        assert_eq!(fallback_title("* Important reminder"), "Important reminder");
        assert_eq!(fallback_title("> Quote of the day"), "Quote of the");
    }

    #[test]
    fn phase_2_gate_explicit_capture_model_disabled_preserves_and_retrieves() {
        use crate::grain_space::vault::{self, Vault};

        let raw_capture = "Emergency contact Dr Alvarez at 408-555-0199, prescription Amlodipine 150mg on 2026-09-04 at cost $349.99. Reference: https://grain.local/spec/v2. Note: \"Always verify facts against active code\", tentatively scheduled maybe?";

        // 1. Model disabled fallback derivation
        let mut note = Note::raw(raw_capture.to_string());
        note.title = fallback_title(&note.body);
        assert!(!note.title.is_empty(), "fallback title must never be empty");
        assert_eq!(note.body, raw_capture, "body must remain 100% verbatim");

        // 2. Save note to an isolated test vault
        let temp_dir = std::env::temp_dir().join(format!("grain_p2_gate_{}", uuid::Uuid::new_v4()));
        let vault = Vault {
            root: temp_dir.join("vault"),
            folder: "Grain".to_string(),
            index_base: temp_dir.join("appdata"),
            native: false,
        };
        std::fs::create_dir_all(&vault.root).unwrap();
        std::fs::create_dir_all(&vault.index_base).unwrap();

        vault::save_note(&vault, &note).expect("save note must succeed");

        // 3. Immediately retrievable through lexical search
        let hit_phone = vault::search_notes_natural(&vault, "555-0199", None).unwrap();
        assert_eq!(hit_phone.len(), 1, "must find note by phone number");
        assert_eq!(hit_phone[0].id, note.id);

        let hit_price = vault::search_notes_natural(&vault, "$349.99", None).unwrap();
        assert_eq!(hit_price.len(), 1, "must find note by price");

        let hit_url = vault::search_notes_natural(&vault, "grain.local/spec", None).unwrap();
        assert_eq!(hit_url.len(), 1, "must find note by URL fragment");

        let hit_quote =
            vault::search_notes_natural(&vault, "verify facts active code", None).unwrap();
        assert_eq!(hit_quote.len(), 1, "must find note by quote fragment");

        let hit_uncertainty =
            vault::search_notes_natural(&vault, "tentatively scheduled maybe", None).unwrap();
        assert_eq!(
            hit_uncertainty.len(),
            1,
            "must find note by uncertainty terms"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase_4_safe_deterministic_append_invariants() {
        use crate::grain_space::vault::{self, Vault};

        let temp_dir = std::env::temp_dir().join(format!("grain_p4_test_{}", uuid::Uuid::new_v4()));
        let vault = Vault {
            root: temp_dir.join("vault"),
            folder: "Grain".to_string(),
            index_base: temp_dir.join("appdata"),
            native: false,
        };
        std::fs::create_dir_all(&vault.root).unwrap();
        std::fs::create_dir_all(&vault.index_base).unwrap();

        // 1. Create initial note
        let original_body = "Line 1\nLine 2 with trailing spaces   \nLine 3";
        let mut note = Note::raw(original_body.to_string());
        note.title = "Roadmap".to_string();
        vault::save_note(&vault, &note).expect("save initial note");

        let v_init =
            vault::note_storage_version(&vault, &note.id).expect("read exact persisted version");

        // 2. Invoke the production append primitive and verify byte-for-byte
        // preservation of everything that was already in the body.
        let addition_1 = "Milestone A: Launch Q3";
        vault::append_note_atomic(&vault, &note.id, addition_1, Some(&v_init))
            .expect("append with current version");
        let appended_1 = vault::get_note(&vault, &note.id).expect("read appended note");
        assert_eq!(
            appended_1.body,
            format!("{original_body}\n\n---\n\n{addition_1}")
        );

        // 3. Reject stale target if modified after preparation.
        let v_after_1 =
            vault::note_storage_version(&vault, &note.id).expect("read post-append version");
        let external_edit = "External edit: Roadmap overhauled completely.";
        let mut ext_note = appended_1.clone();
        ext_note.body = external_edit.to_string();
        vault::save_note(&vault, &ext_note).expect("simulate external edit");
        let stale = vault::append_note_atomic(&vault, &note.id, "must not land", Some(&v_after_1));
        assert!(stale.is_err(), "stale append must fail closed");
        let current_disk = vault::get_note(&vault, &note.id).expect("read after rejection");
        assert_eq!(current_disk.body, external_edit);

        // 4. Missing or wrong target fails closed with zero invented notes.
        let missing_lookup = vault::get_note(&vault, "nonexistent_target_id_999");
        assert!(missing_lookup.is_err(), "missing target must return error");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn phase_4_content_version_hash_properties() {
        // Determinism
        assert_eq!(
            content_version_hash("hello world"),
            content_version_hash("hello world")
        );
        // Sensitivity to single byte changes
        assert_ne!(
            content_version_hash("hello world"),
            content_version_hash("hello world ")
        );
        assert_ne!(
            content_version_hash("hello world\n"),
            content_version_hash("hello world")
        );
        // Multibyte Unicode & CJK
        let cjk_text = "日本語のメモ、茶道と禅。🍵";
        let cjk_hash1 = content_version_hash(cjk_text);
        let cjk_hash2 = content_version_hash(cjk_text);
        assert_eq!(cjk_hash1, cjk_hash2);
        assert_eq!(cjk_hash1.len(), 64); // 64 hex digits (SHA-256)
                                         // Empty text produces valid fixed-width hash
        let empty_hash = content_version_hash("");
        assert_eq!(empty_hash.len(), 64);
    }

    #[test]
    fn phase_4_raw_append_whitespace_and_separator_variations() {
        // 1. Empty body
        let empty_note = Note::raw("".into());
        let appended_empty = raw_append(&empty_note, "  First line  ");
        assert_eq!(appended_empty.body, "First line");

        // 2. Body with no trailing newline
        let note_no_nl = Note::raw("Line 1".into());
        let appended_no_nl = raw_append(&note_no_nl, "Line 2");
        assert_eq!(appended_no_nl.body, "Line 1\n\n---\n\nLine 2");

        // 3. Body with single trailing newline
        let note_single_nl = Note::raw("Line 1\n".into());
        let appended_single_nl = raw_append(&note_single_nl, "Line 2");
        assert_eq!(appended_single_nl.body, "Line 1\n\n---\n\nLine 2");

        // 4. Body with double trailing newline
        let note_double_nl = Note::raw("Line 1\n\n".into());
        let appended_double_nl = raw_append(&note_double_nl, "Line 2");
        assert_eq!(appended_double_nl.body, "Line 1\n\n---\n\nLine 2");

        // 5. Addition with code fences and internal dividers
        let complex_addition = "```rust\nfn main() {}\n```\n---\nfooter note";
        let note_complex = Note::raw("# Heading".into());
        let appended_complex = raw_append(&note_complex, complex_addition);
        assert_eq!(
            appended_complex.body,
            format!("# Heading\n\n---\n\n{complex_addition}")
        );
    }

    #[test]
    fn phase_4_duplicate_detection_edge_cases() {
        let addition = "New task item: Review PR";

        // Case A: Suffix matches but is NOT separated by separator (false positive avoidance)
        let body_accidental_suffix = format!("Prefix text without separator {addition}");
        let is_dup_a = if body_accidental_suffix.ends_with(addition) {
            let before = &body_accidental_suffix[..body_accidental_suffix.len() - addition.len()];
            before.is_empty() || before.ends_with("\n---\n\n") || before.ends_with("\n\n---\n\n")
        } else {
            false
        };
        assert!(
            !is_dup_a,
            "Accidental suffix overlap without separator must NOT be flagged as duplicate"
        );

        // Case B: Exactly separated by \n\n---\n\n
        let body_proper_sep = format!("Existing content\n\n---\n\n{addition}");
        let is_dup_b = if body_proper_sep.ends_with(addition) {
            let before = &body_proper_sep[..body_proper_sep.len() - addition.len()];
            before.is_empty() || before.ends_with("\n---\n\n") || before.ends_with("\n\n---\n\n")
        } else {
            false
        };
        assert!(
            is_dup_b,
            "Proper separator match must be recognized as duplicate"
        );

        // Case C: Body was solely this addition
        let body_sole = addition.to_string();
        let is_dup_c = if body_sole.ends_with(addition) {
            let before = &body_sole[..body_sole.len() - addition.len()];
            before.is_empty() || before.ends_with("\n---\n\n") || before.ends_with("\n\n---\n\n")
        } else {
            false
        };
        assert!(
            is_dup_c,
            "Exact identical body must be recognized as duplicate"
        );
    }
}
