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
    let title: String = body
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
    let settings = get_settings(app);
    // Auto-categorization (AUTO-CATEGORIZATION-PLAN.md P1): when enabled, offer
    // the existing Grain folders to the (already-happening) structuring call so
    // it can route the note. Off/empty ⇒ no routing, no behavior change.
    // Each candidate folder rides in WITH its description — the "what belongs
    // here" evidence the router classifies against (a bare name misfiles; see
    // AUTO-CATEGORIZATION-PLAN.md and folder_meta.rs).
    let folders = if settings.grain_space_auto_categorize {
        let be = backend.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let names = super::backend::list_folders(&be)?;
            super::folder_meta::descriptions_for(&be, &names)
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    let (mut note, routing) = compose_note(app, &body, framing, &folders).await;
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
            // Confidence-gated filing (AUTO-CATEGORIZATION-PLAN.md §4): only a
            // HIGH-confidence route moves the file silently; a MEDIUM route is
            // recorded as a pending suggestion the user accepts with one click
            // in the overlay. A weak/forced guess never touches the file — this
            // is the fix for "news landed in Work". New-folder discovery stays a
            // separate, gated pass (P3), never a single-note decision.
            if let Some(r) = routing {
                let be2 = backend.clone();
                let id2 = id.clone();
                match r.confidence {
                    Confidence::High => {
                        match tauri::async_runtime::spawn_blocking(move || {
                            super::backend::move_note_to_folder(&be2, &id2, Some(&r.folder))
                        })
                        .await
                        {
                            Ok(Ok(_)) => log::info!("[GRAIN] space capture: auto-filed {id}"),
                            Ok(Err(e)) => {
                                log::warn!("[GRAIN] space capture: auto-file failed: {e:#}")
                            }
                            Err(e) => {
                                log::warn!("[GRAIN] space capture: auto-file task panicked: {e}")
                            }
                        }
                    }
                    Confidence::Medium => {
                        let _ = tauri::async_runtime::spawn_blocking(move || {
                            super::folder_meta::set_suggestion(&be2, &id2, &r.folder)
                        })
                        .await;
                        log::info!("[GRAIN] space capture: suggested folder for {id}");
                    }
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

/// How sure the router is that a note belongs in the folder it picked. Drives
/// the filing decision: `High` files silently, `Medium` becomes a one-click
/// suggestion, `Low` (or no fit) leaves the note loose. Parsed from the model's
/// `category_confidence` string; anything unrecognized degrades to `Low` so an
/// unclear answer never moves a file.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Confidence {
    High,
    Medium,
}

/// A validated route: an existing folder plus the confidence behind it. Only
/// `High`/`Medium` reach here — `Low`/none collapse to `None` in `compose_note`.
pub(crate) struct Routing {
    pub folder: String,
    pub confidence: Confidence,
}

/// The structured-output shape for the extraction call.
#[derive(Deserialize, Debug)]
struct ExtractedMeta {
    /// The note rewritten as tidy Markdown (structuring path only). Empty when
    /// structuring was not requested — the verbatim body is then kept.
    #[serde(default)]
    body: String,
    title: String,
    tldr: String,
    #[serde(default)]
    todos: Vec<String>,
    /// Local time "YYYY-MM-DDTHH:MM"; empty string when the note has no
    /// reminder/timer.
    #[serde(default)]
    reminder_at: String,
    /// Auto-categorization (AUTO-CATEGORIZATION-PLAN.md P1): the existing Grain
    /// folder this note best belongs to (chosen ONLY from the provided list), or
    /// "" for none. Piggybacks the structuring call — no extra model, no extra
    /// RAM. Present only when the caller supplied candidate folders.
    #[serde(default)]
    category: String,
    /// How sure the model is about `category`: "high" | "medium" | "low" (or ""
    /// when `category` is ""). Gates whether the note is filed silently, merely
    /// suggested, or left loose. Present only alongside `category`.
    #[serde(default)]
    category_confidence: String,
}

impl ExtractedMeta {
    fn apply(self, note: &mut Note, auto_arm: bool) {
        note.title = self.title.trim().to_string();
        note.tldr = self.tldr.trim().to_string();
        note.todo_tags = self
            .todos
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .map(|text| TodoTag { text, done: false })
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
    structure: bool,
    folders: &[(String, String)],
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

    // Two modes share one call. STRUCTURING (freshly dictated/typed note, no
    // selection): the model also returns a Markdown-formatted `body`. VERBATIM
    // (a selection the user is saving): the body is preserved untouched and the
    // `framing` line steers only the title/summary.
    let body_rule = if structure {
        "- body: the note as clean Markdown. Infer intent and format for readability using headings \
         (#/##), bullet or numbered lists, `- [ ]` checklists for tasks, tables for tabular/columned \
         data, **bold**, > quotes and `code`. Keep EVERY fact and detail — reformat and lightly drop \
         only spoken filler; never summarize, invent, or omit content. Strip a leading command such \
         as \"make a note that…\" or \"note:\". A single plain thought stays one line.\n"
    } else {
        ""
    };
    let framing_line = match framing {
        Some(f) if !f.trim().is_empty() => format!(
            "\nThe user selected the note text and, to say what it is for, added: \"{}\". Use that \
             to shape the title and summary (what the note is FOR); do NOT add it to the note text.",
            f.trim()
        ),
        _ => String::new(),
    };
    let intro = if structure {
        "You turn what the user just captured into a clean personal note. Reply with JSON only."
    } else {
        "You extract metadata from a personal note the user is saving. Reply with JSON only."
    };
    // Auto-categorization rides this existing call: present each folder WITH its
    // description (the label schema) and ask for a fit + a calibrated confidence.
    // Only when folders exist. Descriptions are the evidence that stops a bare
    // name ("Work") from vacuuming up unrelated notes.
    let category_line = if folders.is_empty() {
        String::new()
    } else {
        let catalogue = folders
            .iter()
            .map(|(name, desc)| {
                let desc = desc.trim();
                if desc.is_empty() {
                    format!("  - \"{name}\" (no description)")
                } else {
                    format!("  - \"{name}\": {desc}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n- category: the ONE folder below this note clearly belongs in, judged AGAINST each \
             folder's description, copied EXACTLY (its quoted name), or an empty string when none \
             is a genuine fit. Do NOT force it — an empty string is the correct, expected answer \
             when nothing matches; NEVER invent a folder or match on a superficial word overlap.\n\
             - category_confidence: your confidence that the note truly belongs there — \"high\" \
             only when the note plainly matches the folder's description; \"medium\" when it is a \
             reasonable but uncertain fit; \"low\" when it is a guess. Use \"\" when category is \"\". \
             Prefer \"medium\"/\"low\" over \"high\" whenever there is any doubt.\n\
             Folders (name: what belongs there):\n{catalogue}"
        )
    };
    let system_prompt = format!(
        "{intro}{framing_line}\n\
         Rules:\n\
         {body_rule}\
         - title: at most 3 words, plain text.\n\
         - tldr: exactly one short sentence.\n\
         - todos: action items present in the note (empty array if none).\n\
         - reminder_at: if a reminder/timer is requested, the local datetime it should fire as \
           YYYY-MM-DDTHH:MM; otherwise an empty string. The current local datetime is {now_local}.\
         {category_line}\
         {verbatim_tail}",
        verbatim_tail = if structure {
            ""
        } else {
            "\nNever rewrite or summarize away the note itself — you only produce metadata."
        }
    );

    let mut properties = serde_json::json!({
        "title": { "type": "string" },
        "tldr": { "type": "string" },
        "todos": { "type": "array", "items": { "type": "string" } },
        "reminder_at": { "type": "string" }
    });
    let mut required = vec!["title", "tldr", "todos", "reminder_at"];
    if structure {
        properties["body"] = serde_json::json!({ "type": "string" });
        required.insert(0, "body");
    }
    if !folders.is_empty() {
        properties["category"] = serde_json::json!({ "type": "string" });
        properties["category_confidence"] =
            serde_json::json!({ "type": "string", "enum": ["high", "medium", "low", ""] });
        required.push("category");
        required.push("category_confidence");
    }
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

// -- conversational writing (RECALL-PLAN §7) -----------------------------------

/// The structured-output shape for a reconcile (merge) call. Same fields as
/// [`ExtractedMeta`] plus the merged `body` and per-todo `done` state.
#[derive(Deserialize, Debug)]
struct MergedMeta {
    body: String,
    title: String,
    tldr: String,
    #[serde(default)]
    todos: Vec<MergedTodo>,
    #[serde(default)]
    reminder_at: String,
}

#[derive(Deserialize, Debug)]
struct MergedTodo {
    text: String,
    #[serde(default)]
    done: bool,
}

impl MergedMeta {
    /// Fold the merge onto a clone of `current`, preserving id/timestamp/pin.
    /// Conservative: a blank field from the model keeps the current value, so a
    /// weak completion can never erase the note.
    fn apply_to(self, current: &Note, auto_arm: bool) -> Note {
        let mut note = current.clone();
        if !self.body.trim().is_empty() {
            note.body = self.body.trim().to_string();
        }
        if !self.title.trim().is_empty() {
            note.title = self.title.trim().to_string();
        }
        if !self.tldr.trim().is_empty() {
            note.tldr = self.tldr.trim().to_string();
        }
        // Trust the model's FULL merged todo list; keep the current list only
        // when it returned none (never silently drop todos).
        let todos: Vec<TodoTag> = self
            .todos
            .into_iter()
            .map(|t| TodoTag {
                text: t.text.trim().to_string(),
                done: t.done,
            })
            .filter(|t| !t.text.is_empty())
            .collect();
        if !todos.is_empty() {
            note.todo_tags = todos;
        }
        // Only touch the reminder when the change actually specified timing.
        if let Some(ms) = parse_local_datetime_ms(self.reminder_at.trim()) {
            note.reminder_state = ReminderState {
                status: if auto_arm {
                    ReminderStatus::Armed
                } else {
                    ReminderStatus::Pending
                },
                fire_at: Some(ms),
            };
        }
        note
    }
}

/// Conservatively merge a spoken change into an existing note (RECALL-PLAN §7.1)
/// — the reconcile sibling of [`extract_metadata`], reusing the same structured
/// LLM infra. NEVER loses the user's words: with no usable provider, or on any
/// LLM/parse failure, it falls back to appending the raw change to the body and
/// leaving the rest untouched. Returns the merged note ready to save; id,
/// timestamp, and pin state are preserved.
pub(crate) async fn reconcile_note(
    app: &AppHandle,
    current: &Note,
    change: &str,
    convo_context: &str,
) -> Note {
    let settings = get_settings(app);
    if llm_usable(&settings) {
        match reconcile_call(app, &settings, current, change, convo_context).await {
            Ok(merged) => {
                let candidate = merged.apply_to(current, settings.grain_space_auto_reminders);
                // Confidence guard: a merge that silently drops most of a
                // non-trivial body is almost always a bad merge, not a genuine
                // supersede. A wrong overwrite is worse than a plain append, so
                // fall back to appending the raw change instead.
                if merge_lost_content(current, &candidate) {
                    log::warn!(
                        "[GRAIN] space reconcile: merge dropped too much body; raw-appending change"
                    );
                    return raw_append(current, change);
                }
                return candidate;
            }
            Err(e) => {
                log::warn!("[GRAIN] space reconcile: merge failed ({e}); raw-appending change")
            }
        }
    }
    raw_append(current, change)
}

/// True when a reconcile merge lost more than half of a non-trivial body — the
/// signal we use to distrust the merge and fall back to a safe append. Short
/// bodies (< 40 chars) are exempt: replacing a tiny note wholesale is normal.
fn merge_lost_content(current: &Note, candidate: &Note) -> bool {
    let cur = current.body.trim().chars().count();
    let new = candidate.body.trim().chars().count();
    cur >= 40 && new.saturating_mul(2) < cur
}

/// True when a structuring reformat lost more than half of a non-trivial note —
/// the signal to distrust it and keep the verbatim body. Markdown formatting
/// only ADDS characters, so a big shrink means the model summarized. Short notes
/// (< 40 chars) are exempt: a one-liner legitimately stays short.
fn reformat_lost_content(raw: &str, formatted: &str) -> bool {
    let r = raw.trim().chars().count();
    let f = formatted.trim().chars().count();
    r >= 40 && f.saturating_mul(2) < r
}

/// Degrade path: append the change to the body verbatim, keep everything else.
fn raw_append(current: &Note, change: &str) -> Note {
    let mut note = current.clone();
    let change = change.trim();
    if change.is_empty() {
        return note;
    }
    note.body = if note.body.trim().is_empty() {
        change.to_string()
    } else {
        format!("{}\n{}", note.body.trim_end(), change)
    };
    note
}

/// Build a note from freshly-captured text: verbatim `body` + one metadata
/// extraction. `framing` (a spoken/typed instruction about a saved selection)
/// shapes the title/summary only. Degrades to a raw note on any extraction
/// failure — the body is always preserved. Shared by the `remember` action and
/// note capture; does NOT save.
pub(crate) async fn compose_note(
    app: &AppHandle,
    body: &str,
    framing: Option<&str>,
    folders: &[(String, String)],
) -> (Note, Option<Routing>) {
    let mut note = Note::raw(body.trim().to_string());
    let settings = get_settings(app);
    // The route the model chose (validated back to an exact folder + gated on
    // confidence), or None. Piggybacks the extraction call above.
    let mut routing: Option<Routing> = None;
    if llm_usable(&settings) {
        // Structure the body only for a freshly dictated/typed note (no framed
        // selection) that fits the sample window — a truncated giant paste can't
        // be safely reformatted, and a saved selection stays verbatim.
        let fits = body.trim().chars().count() <= META_SAMPLE_CHARS;
        let structure = framing.is_none() && fits;
        // Only a capped sample of a huge body is sent for metadata; the note
        // body itself (set above) stays complete.
        match extract_metadata(
            app,
            &settings,
            &sample_for_meta(body.trim()),
            framing,
            structure,
            folders,
        )
        .await
        {
            Ok(mut meta) => {
                // Validate the routed category against the real folder list
                // (exact name, case-insensitive) so the model can never invent
                // a folder here — new-folder discovery is a separate, gated pass.
                // Then gate on confidence: only high/medium survive; a low or
                // unrecognized confidence collapses to None (note stays loose).
                let cat = meta.category.trim();
                if !cat.is_empty() {
                    if let Some((folder, _)) =
                        folders.iter().find(|(name, _)| name.eq_ignore_ascii_case(cat))
                    {
                        routing = match meta.category_confidence.trim().to_ascii_lowercase().as_str()
                        {
                            "high" => Some(Routing {
                                folder: folder.clone(),
                                confidence: Confidence::High,
                            }),
                            "medium" => Some(Routing {
                                folder: folder.clone(),
                                confidence: Confidence::Medium,
                            }),
                            _ => None,
                        };
                    }
                }
                // Adopt the reformatted body only when it clearly preserved the
                // content — a formatted note is longer, not shorter, so a big
                // shrink means the model summarized and we keep the raw text.
                let formatted = std::mem::take(&mut meta.body);
                let formatted = formatted.trim();
                if structure
                    && !formatted.is_empty()
                    && !reformat_lost_content(&note.body, formatted)
                {
                    note.body = formatted.to_string();
                }
                meta.apply(&mut note, settings.grain_space_auto_reminders);
            }
            Err(e) => log::warn!("[GRAIN] space compose: extraction failed ({e}); raw note"),
        }
    }
    // Metadata off, unavailable, or a weak completion that left the title blank:
    // fall back to a plain-code title so the note is never untitled.
    if note.title.trim().is_empty() {
        note.title = fallback_title(&note.body);
    }
    (note, routing)
}

/// Propose a one/two-sentence description for a folder from a sample of the
/// notes already in it — the label schema the router will classify against
/// (AUTO-CATEGORIZATION-PLAN.md). One cheap LLM call against the active
/// post-process provider; `None` on no samples / no usable provider / failure.
/// User-triggered (a button in the open overlay), so never idle work.
pub(crate) async fn propose_folder_description(
    app: &AppHandle,
    folder: &str,
    samples: &[String],
) -> Option<String> {
    let settings = get_settings(app);
    if !llm_usable(&settings) || samples.is_empty() {
        return None;
    }
    let provider = settings.active_post_process_provider().cloned()?;
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
        .map(|s| s.inner().clone())?;

    let system_prompt = format!(
        "You write a short definition for a personal-notes folder named \"{folder}\", to help \
         auto-file future notes. Given a sample of the notes it currently holds, reply with JSON \
         {{\"description\": \"...\"}} where description is ONE or two plain sentences stating what \
         kind of note belongs in this folder — the shared subject/purpose, not a list of the \
         examples. No folder name prefix, no markdown."
    );
    let user = format!("Notes currently in \"{folder}\":\n- {}", samples.join("\n- "));
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "description": { "type": "string" } },
        "required": ["description"],
        "additionalProperties": false
    });

    let success = crate::llm_client::send_chat_completion_with_schema(
        &client,
        &provider,
        api_key,
        &model,
        user,
        Some(system_prompt),
        Some(schema),
        None,
        None,
    )
    .await
    .ok()?;
    crate::post_process_router::record_usage(app, &provider.id);

    #[derive(Deserialize)]
    struct Proposed {
        #[serde(default)]
        description: String,
    }
    let parsed: Proposed = serde_json::from_str(strip_code_fences(&success.content?)).ok()?;
    let desc = parsed.description.trim();
    if desc.is_empty() {
        None
    } else {
        Some(desc.chars().take(400).collect())
    }
}

/// One structured merge call against the active post-process provider.
async fn reconcile_call(
    app: &AppHandle,
    settings: &AppSettings,
    current: &Note,
    change: &str,
    convo_context: &str,
) -> Result<MergedMeta, String> {
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

    let note_json = serde_json::json!({
        "title": current.title,
        "tldr": current.tldr,
        "body": current.body,
        "todos": current.todo_tags.iter().map(|t| serde_json::json!({ "text": t.text, "done": t.done })).collect::<Vec<_>>(),
    })
    .to_string();
    let context_block = if convo_context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\nRecent conversation (resolve references like \"the first two\" against this):\n{}\n",
            convo_context.trim()
        )
    };
    let now_local = chrono::Local::now().format("%A %Y-%m-%dT%H:%M").to_string();
    let system_prompt = format!(
        "You are updating one of the user's saved memories from something they just said. Merge \
         their change into the memory CONSERVATIVELY and reply with JSON only.\n\
         Current memory (JSON): {note_json}{context_block}\n\
         Rules:\n\
         - body: incorporate the new information. APPEND by default; only rewrite existing wording \
         when the change genuinely supersedes it (e.g. a changed password replaces the old value, \
         keeping the rest). NEVER drop content the user did not ask to remove. When unsure whether \
         to rewrite or append, APPEND — return the current body with the new information added, \
         never a shorter body than you started with unless the user explicitly removed something.\n\
         - title: at most 3 words. Keep the existing title unless the memory is now about something \
         different.\n\
         - tldr: one short sentence describing the merged memory.\n\
         - todos: the FULL merged list of items as {{text, done}} objects. Add new ones, mark named \
         ones done, drop only ones the user says to remove; preserve existing done states and order \
         otherwise.\n\
         - reminder_at: only if the change mentions a time/reminder — the local datetime \
         YYYY-MM-DDTHH:MM; otherwise an empty string. The current local datetime is {now_local}."
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "body": { "type": "string" },
            "title": { "type": "string" },
            "tldr": { "type": "string" },
            "todos": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "done": { "type": "boolean" }
                    },
                    "required": ["text", "done"],
                    "additionalProperties": false
                }
            },
            "reminder_at": { "type": "string" }
        },
        "required": ["body", "title", "tldr", "todos", "reminder_at"],
        "additionalProperties": false
    });

    let success = crate::llm_client::send_chat_completion_with_schema(
        &client,
        &provider,
        api_key,
        &model,
        change.to_string(),
        Some(system_prompt),
        Some(schema),
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let content = success.content.ok_or("empty completion")?;
    let meta: MergedMeta =
        serde_json::from_str(strip_code_fences(&content)).map_err(|e| e.to_string())?;
    crate::post_process_router::record_usage(app, &provider.id);
    Ok(meta)
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

    #[test]
    fn metadata_apply_arms_or_parks_reminder() {
        let meta = |auto: bool| {
            let m = ExtractedMeta {
                body: String::new(),
                title: " Wifi Note ".into(),
                tldr: "The wifi password.".into(),
                todos: vec!["buy milk".into(), "  ".into()],
                reminder_at: "2026-07-06T18:30".into(),
                category: String::new(),
                category_confidence: String::new(),
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
    fn bad_reminder_string_means_no_reminder() {
        let m = ExtractedMeta {
            body: String::new(),
            title: "T".into(),
            tldr: "S".into(),
            todos: vec![],
            reminder_at: "tomorrow evening".into(),
            category: String::new(),
            category_confidence: String::new(),
        };
        let mut note = Note::raw("b".into());
        m.apply(&mut note, true);
        assert_eq!(note.reminder_state.status, ReminderStatus::None);
        assert!(note.reminder_state.fire_at.is_none());
    }

    #[test]
    fn code_fences_are_stripped() {
        assert_eq!(strip_code_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn raw_append_preserves_and_appends() {
        let mut cur = Note::raw("original".into());
        cur.title = "Keep Me".into();
        let out = raw_append(&cur, "  more info  ");
        assert_eq!(out.body, "original\nmore info");
        assert_eq!(out.title, "Keep Me"); // untouched
        assert_eq!(out.id, cur.id); // identity preserved

        let empty = Note::raw("".into());
        assert_eq!(raw_append(&empty, "first").body, "first");
    }

    #[test]
    fn merged_meta_is_conservative_on_blanks() {
        let mut cur = Note::raw("body".into());
        cur.title = "Old Title".into();
        cur.tldr = "old summary".into();
        cur.todo_tags = vec![TodoTag {
            text: "task".into(),
            done: false,
        }];
        // Model returned blank title/tldr/body and no todos → keep everything.
        let merged = MergedMeta {
            body: "  ".into(),
            title: "".into(),
            tldr: "".into(),
            todos: vec![],
            reminder_at: "".into(),
        }
        .apply_to(&cur, true);
        assert_eq!(merged.body, "body");
        assert_eq!(merged.title, "Old Title");
        assert_eq!(merged.tldr, "old summary");
        assert_eq!(merged.todo_tags.len(), 1);
        assert_eq!(merged.reminder_state.status, ReminderStatus::None);
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
    fn reformat_lost_content_flags_summaries_only() {
        let raw = "buy milk and eggs, call the plumber about the leak, book the dentist";
        // A genuine reformat is longer (adds markdown) → trusted.
        let formatted = "- [ ] buy milk and eggs\n- [ ] call the plumber about the leak\n- [ ] book the dentist";
        assert!(!reformat_lost_content(raw, formatted));
        // A summary that dropped over half the note → distrusted.
        assert!(reformat_lost_content(raw, "- buy groceries"));
        // Short notes are exempt (a one-liner stays short).
        assert!(!reformat_lost_content("call mom", "call mom"));
    }

    #[test]
    fn merge_lost_content_flags_big_drops_only() {
        let mut cur = Note::raw("a".repeat(100));
        let mut cand = cur.clone();
        // Kept most of it → fine.
        cand.body = "a".repeat(60);
        assert!(!merge_lost_content(&cur, &cand));
        // Dropped more than half of a long body → distrust.
        cand.body = "a".repeat(30);
        assert!(merge_lost_content(&cur, &cand));
        // Short bodies are exempt (wholesale replace is normal).
        cur.body = "tiny".into();
        cand.body = "x".into();
        assert!(!merge_lost_content(&cur, &cand));
    }

    #[test]
    fn merged_meta_replaces_todos_when_provided() {
        let cur = Note::raw("b".into());
        let merged = MergedMeta {
            body: "b".into(),
            title: "T".into(),
            tldr: "s".into(),
            todos: vec![
                MergedTodo {
                    text: "one".into(),
                    done: true,
                },
                MergedTodo {
                    text: " ".into(),
                    done: false,
                },
            ],
            reminder_at: "".into(),
        }
        .apply_to(&cur, false);
        assert_eq!(merged.todo_tags.len(), 1); // blank dropped
        assert!(merged.todo_tags[0].done);
    }
}
