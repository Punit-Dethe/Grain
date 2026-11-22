//! Opt-in suggestions from a single, confirmed dictation insertion.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use grain_core::AppContext;
use tauri::{AppHandle, Manager};

static GENERATION: AtomicU32 = AtomicU32::new(0);

pub(crate) fn cancel_watch() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(crate) fn is_current(id: u32) -> bool {
    GENERATION.load(Ordering::Acquire) == id
}

#[tauri::command]
#[specta::specta]
pub fn set_learn_from_edits(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled && !cfg!(windows) {
        return Err("Learning from edits is available on Windows.".into());
    }
    app.state::<Arc<AppContext>>()
        .update_public_settings(|settings| settings.learn_from_edits = enabled)
        .map_err(|error| error.to_string())?;
    if !enabled {
        cancel_watch();
        crate::grain_overlay::dismiss_dictionary_suggestion(&app);
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) use windows_watch::prepare;

#[cfg(windows)]
mod windows_watch {
    use super::*;
    use crate::context_detect::dictionary_field;
    use crate::settings::{OverlayStyle, PasteMethod};
    use std::time::{Duration, Instant};

    /// Field reads while confirming the insertion landed...
    const CONFIRM_SAMPLE: Duration = Duration::from_millis(250);
    /// ...which must happen within this long, or the watch ends.
    const CONFIRM_WITHIN: Duration = Duration::from_secs(1);
    /// Field reads while watching for a correction: the same ~40 reads per
    /// watch as before the window grew.
    const SAMPLE: Duration = Duration::from_millis(500);
    /// How long the field is watched after an insertion, so a slow correction
    /// is still caught.
    const WATCH: Duration = Duration::from_secs(20);
    /// A correction is offered only once the edit has stopped changing for
    /// this long. Shorter caught words half-way through being retyped; a late
    /// suggestion costs nothing.
    const SETTLE: Duration = Duration::from_millis(2_500);

    pub(crate) struct Watch {
        id: u32,
        field_id: Vec<i32>,
        before: String,
        inserted: String,
    }

    pub(crate) fn prepare(
        app: &AppHandle,
        text: &str,
        suppress_trailing_space: bool,
    ) -> Option<Watch> {
        let id = GENERATION.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let append_space = app.state::<Arc<AppContext>>().with_settings(|settings| {
            (settings.learn_from_edits
                && !settings.auto_submit
                && settings.overlay_style != OverlayStyle::None
                && !matches!(
                    settings.paste_method,
                    PasteMethod::None | PasteMethod::ExternalScript
                ))
            .then_some(settings.append_trailing_space && !suppress_trailing_space)
        })?;
        if text.trim().is_empty() || text.len() > 2048 || crate::agent::foreground_is_terminal() {
            return None;
        }
        let (field_id, before) = dictionary_field()?;
        let mut inserted = text.replace("\r\n", "\n");
        if append_space {
            inserted.push(' ');
        }
        Some(Watch {
            id,
            field_id,
            before,
            inserted,
        })
    }

    impl Watch {
        pub(crate) fn start(self, app: AppHandle) {
            tauri::async_runtime::spawn(async move {
                let started = Instant::now();
                let mut anchor: Option<(String, String)> = None;
                let mut previous = None;
                let mut changed_at = Instant::now();
                loop {
                    // Quick checks until the insertion is confirmed, then slower.
                    let interval = if anchor.is_none() {
                        CONFIRM_SAMPLE
                    } else {
                        SAMPLE
                    };
                    tokio::time::sleep(interval).await;
                    if !is_current(self.id) || started.elapsed() > WATCH {
                        return;
                    }
                    let Ok(Some((field, value))) =
                        tauri::async_runtime::spawn_blocking(dictionary_field).await
                    else {
                        return;
                    };
                    if field != self.field_id || !is_current(self.id) {
                        return;
                    }
                    if let Some((prefix, suffix)) = &anchor {
                        let Some(edited) = value
                            .strip_prefix(prefix.as_str())
                            .and_then(|rest| rest.strip_suffix(suffix.as_str()))
                        else {
                            return;
                        };
                        let correction = replacement(&self.inserted, edited)
                            .map(|(old, new)| (old.to_owned(), new.to_owned()));
                        if correction != previous {
                            previous = correction;
                            changed_at = Instant::now();
                        } else if changed_at.elapsed() >= SETTLE {
                            if let Some((original, corrected)) = correction {
                                crate::grain_overlay::show_dictionary_suggestion(
                                    &app, self.id, original, corrected,
                                );
                                return;
                            }
                        }
                    } else {
                        anchor = confirm_insertion(&self.before, &value, &self.inserted);
                        if anchor.is_none() && started.elapsed() >= CONFIRM_WITHIN {
                            return;
                        }
                    }
                }
            });
        }
    }
}

/// Find the inserted span only when it accounts for the change from the pre-paste
/// field. Exact surrounding text anchors the span through later caret movement.
#[cfg(any(windows, test))]
fn confirm_insertion(before: &str, after: &str, inserted: &str) -> Option<(String, String)> {
    if before == after {
        return None;
    }
    let mut matches = after.match_indices(inserted).filter_map(|(index, _)| {
        let prefix = &after[..index];
        let suffix = &after[index + inserted.len()..];
        before.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some((prefix, suffix))
    });
    let (prefix, suffix) = matches.next()?;
    matches
        .next()
        .is_none()
        .then(|| (prefix.into(), suffix.into()))
}

/// A single changed word inside the confirmed span. Appended prose, deletion,
/// and multiple replacements are deliberately inconclusive.
#[cfg(any(windows, test))]
fn replacement<'a>(original: &'a str, edited: &'a str) -> Option<(&'a str, &'a str)> {
    let mut before = original.split_whitespace();
    let mut after = edited.split_whitespace();
    let mut changed = None;
    loop {
        match (before.next(), after.next()) {
            (Some(old), Some(new)) if old == new => {}
            (Some(old), Some(new)) if changed.is_none() => {
                let old = dictionary_word(old)?;
                let new = dictionary_word(new)?;
                if old.to_lowercase() == new.to_lowercase() {
                    return None;
                }
                changed = Some((old, new));
            }
            (None, None) => return changed,
            _ => return None,
        }
    }
}

#[cfg(any(windows, test))]
fn dictionary_word(text: &str) -> Option<&str> {
    let word = text.trim_matches(|c: char| {
        matches!(
            c,
            '.' | ','
                | '!'
                | '?'
                | ':'
                | ';'
                | '"'
                | '\''
                | '“'
                | '”'
                | '‘'
                | '’'
                | '('
                | ')'
                | '['
                | ']'
        )
    });
    (word.chars().count() <= 48
        && word.chars().filter(|c| c.is_alphabetic()).take(2).count() == 2
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '\'' | '’')))
    .then_some(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_a_confirmed_replacement_without_reading_unrelated_edits() {
        assert_eq!(
            confirm_insertion("To Bob today", "To Puneet today", "Puneet"),
            Some(("To ".into(), " today".into()))
        );
        assert_eq!(confirm_insertion("Puneet", "Puneet", "Puneet"), None);
        assert_eq!(confirm_insertion("Puneet", "PuneetPuneet", "Puneet"), None);
        assert_eq!(
            confirm_insertion("To Bob today", "From Puneet today", "Puneet"),
            None
        );
    }

    #[test]
    fn offers_only_one_replaced_word() {
        assert_eq!(
            replacement("Hello Puneet.", "Hello Punit."),
            Some(("Puneet", "Punit"))
        );
        assert_eq!(
            replacement("forty", "fourteen"),
            Some(("forty", "fourteen"))
        );
        assert_eq!(replacement("Puneet", "Puneet is here"), None);
        assert_eq!(replacement("Hello Puneet", "Hi Punit"), None);
        assert_eq!(replacement("Puneet", "Puneet!"), None);
        assert_eq!(replacement("Puneet", "https://example.com"), None);
    }
}
