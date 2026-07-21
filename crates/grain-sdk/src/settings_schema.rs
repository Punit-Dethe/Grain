//! [GRAIN] Schema-declared settings: validation and resolution (SPEC §4.1).
//!
//! An extension's settings are **declared**, not coded: the manifest says what
//! the controls are, the host renders them, and the values live in the
//! extension's own namespace — never in `AppSettings`.
//!
//! The schema is therefore a contract with two enforcement points, and this
//! module is the single implementation both share:
//!
//! - [`coerce`] — the **write** path. Runs on every value entering the
//!   namespace, whether it came from the host's own control or from the
//!   extension calling `settings.set` over the WebSocket. A schema enforced
//!   only in the settings form is not enforced at all: the extension can write
//!   to the same keys directly.
//! - [`resolve`] — the **read** path. Turns whatever is stored into a value the
//!   declared control can actually render. Storage outlives schemas — an update
//!   narrows a `select`, tightens a range, or changes a kind outright — so a
//!   stale value degrades to the default with a notice rather than breaking the
//!   settings page.
//!
//! Both are pure over `(&SettingDecl, &Value)`, so the rules are unit-tested
//! without a filesystem, a Tauri handle, or a running extension.

use serde_json::Value;

use crate::manifest::{SettingDecl, SettingKind};

/// An accepted value, plus anything the user should be told about it.
///
/// A `notice` is not an error: the value **was** stored. It exists because
/// silently changing what someone typed is worse than either accepting or
/// rejecting it.
#[derive(Clone, Debug, PartialEq)]
pub struct Accepted {
    pub value: Value,
    pub notice: Option<String>,
}

impl Accepted {
    fn plain(value: Value) -> Self {
        Self {
            value,
            notice: None,
        }
    }

    fn with(value: Value, notice: impl Into<String>) -> Self {
        Self {
            value,
            notice: Some(notice.into()),
        }
    }
}

/// The value a control shows when nothing valid is stored: the declared
/// `default` if it fits the kind, otherwise the kind's own empty state.
///
/// Never `null` for a known kind — a control with no value has nothing to
/// render, and every caller would need its own fallback.
pub fn fallback(decl: &SettingDecl) -> Value {
    if let Ok(a) = check(decl, &decl.default) {
        return a.value;
    }
    match &decl.kind {
        SettingKind::Bool => Value::Bool(false),
        SettingKind::String | SettingKind::Shortcut => Value::String(String::new()),
        SettingKind::Color => Value::String("#000000".into()),
        SettingKind::Number { min, .. } => number(min.unwrap_or(0.0)),
        SettingKind::Slider { min, .. } => number(*min),
        SettingKind::Select { options } => options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or(Value::Null),
        // An unknown kind has no empty state this build could invent, and
        // inventing one would overwrite data a newer build understands.
        SettingKind::Unsupported => Value::Null,
    }
}

/// Validate a value on its way **into** the namespace.
///
/// Type mismatches are rejected outright — accepting a string where a `bool`
/// was declared would hand the extension a value its own schema says cannot
/// exist. Out-of-range numbers are clamped instead, because a range is a
/// preference about magnitude, not about type.
pub fn coerce(decl: &SettingDecl, value: &Value) -> Result<Accepted, String> {
    check(decl, value)
}

/// Turn a **stored** value into something the declared control can render.
///
/// Unlike [`coerce`] this never fails: a value that no longer fits its schema
/// falls back to the default with a notice. That is the update path — a
/// narrowed `select` or a changed kind must not leave the settings page unable
/// to draw a row.
pub fn resolve(decl: &SettingDecl, stored: Option<&Value>) -> Accepted {
    let Some(stored) = stored.filter(|v| !v.is_null()) else {
        return Accepted::plain(fallback(decl));
    };
    // A kind this build doesn't understand is passed through untouched: the
    // value belongs to whoever wrote it, and a newer build must find it intact.
    if matches!(decl.kind, SettingKind::Unsupported) {
        return Accepted::plain(stored.clone());
    }
    match check(decl, stored) {
        Ok(a) => a,
        Err(reason) => Accepted::with(
            fallback(decl),
            format!("“{}” was reset to its default: {reason}", decl.label),
        ),
    }
}

fn number(n: f64) -> Value {
    serde_json::Number::from_f64(n)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// The shared rule table. Returns the value to store, or why it cannot be.
fn check(decl: &SettingDecl, value: &Value) -> Result<Accepted, String> {
    match &decl.kind {
        SettingKind::Bool => value
            .as_bool()
            .map(|b| Accepted::plain(Value::Bool(b)))
            .ok_or_else(|| "expected true or false".into()),

        SettingKind::String => value
            .as_str()
            .map(|s| Accepted::plain(Value::String(s.to_string())))
            .ok_or_else(|| "expected text".into()),

        SettingKind::Shortcut => {
            let s = value.as_str().ok_or("expected a shortcut")?;
            // The binding registry owns what a *valid* chord is (and whether it
            // collides); the schema only guarantees the shape.
            if s.trim().is_empty() {
                return Err("a shortcut cannot be blank".into());
            }
            Ok(Accepted::plain(Value::String(s.to_string())))
        }

        SettingKind::Color => {
            let s = value.as_str().ok_or("expected a colour")?;
            let hex = s.strip_prefix('#').unwrap_or("");
            let shaped = matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit());
            if !shaped {
                return Err("expected a hex colour like #4f8cff".into());
            }
            Ok(Accepted::plain(Value::String(s.to_ascii_lowercase())))
        }

        SettingKind::Number { min, max } => {
            let n = value.as_f64().ok_or("expected a number")?;
            Ok(clamp(n, *min, *max, None))
        }

        SettingKind::Slider { min, max, step } => {
            let n = value.as_f64().ok_or("expected a number")?;
            Ok(clamp(n, Some(*min), Some(*max), *step))
        }

        SettingKind::Select { options } => {
            let s = value.as_str().ok_or("expected one of the listed choices")?;
            if !options.iter().any(|o| o.value == s) {
                return Err(format!("“{s}” is not one of the listed choices"));
            }
            Ok(Accepted::plain(Value::String(s.to_string())))
        }

        // Rejected on the way in, passed through on the way out (see `resolve`):
        // this build cannot know what a valid value for it looks like.
        SettingKind::Unsupported => {
            Err("this version of Grain does not support that control".into())
        }
    }
}

fn clamp(n: f64, min: Option<f64>, max: Option<f64>, step: Option<f64>) -> Accepted {
    let mut out = n;
    if let (Some(step), Some(min)) = (step.filter(|s| *s > 0.0), min) {
        out = min + ((out - min) / step).round() * step;
    }
    if let Some(min) = min {
        out = out.max(min);
    }
    if let Some(max) = max {
        out = out.min(max);
    }
    // Compare against the input, not the pre-clamp value, so a pure snap still
    // tells the user their number moved.
    if (out - n).abs() > f64::EPSILON {
        return Accepted::with(number(out), format!("adjusted from {n} to {out}"));
    }
    Accepted::plain(number(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::SelectOption;

    fn decl(kind: SettingKind, default: Value) -> SettingDecl {
        SettingDecl {
            key: "k".into(),
            label: "Thing".into(),
            kind,
            default,
            description: String::new(),
            anchor: None,
            order: 0,
        }
    }

    #[test]
    fn type_mismatches_are_rejected_not_silently_converted() {
        let d = decl(SettingKind::Bool, Value::Bool(false));
        assert!(coerce(&d, &Value::String("true".into())).is_err());
        assert!(coerce(&d, &Value::Bool(true)).is_ok());

        let d = decl(SettingKind::String, Value::Null);
        assert!(coerce(&d, &serde_json::json!(3)).is_err());
    }

    #[test]
    fn numbers_clamp_with_a_notice_rather_than_failing() {
        let d = decl(
            SettingKind::Number {
                min: Some(1.0),
                max: Some(10.0),
            },
            serde_json::json!(5),
        );
        let a = coerce(&d, &serde_json::json!(99)).unwrap();
        assert_eq!(a.value, serde_json::json!(10.0));
        assert!(a.notice.is_some(), "a silently changed value is worse");

        let a = coerce(&d, &serde_json::json!(4)).unwrap();
        assert_eq!(a.value, serde_json::json!(4.0));
        assert!(a.notice.is_none());
    }

    #[test]
    fn sliders_snap_to_their_step_inside_the_range() {
        let d = decl(
            SettingKind::Slider {
                min: 0.0,
                max: 100.0,
                step: Some(25.0),
            },
            serde_json::json!(0),
        );
        // 37 is nearer 25 than 50, so it snaps down.
        assert_eq!(
            coerce(&d, &serde_json::json!(37)).unwrap().value,
            serde_json::json!(25.0)
        );
        assert_eq!(
            coerce(&d, &serde_json::json!(38)).unwrap().value,
            serde_json::json!(50.0)
        );
        assert_eq!(
            coerce(&d, &serde_json::json!(-5)).unwrap().value,
            serde_json::json!(0.0)
        );
        assert_eq!(
            coerce(&d, &serde_json::json!(999)).unwrap().value,
            serde_json::json!(100.0)
        );
    }

    #[test]
    fn a_select_only_accepts_a_listed_choice() {
        let d = decl(
            SettingKind::Select {
                options: vec![
                    SelectOption {
                        value: "a".into(),
                        label: "A".into(),
                    },
                    SelectOption {
                        value: "b".into(),
                        label: "B".into(),
                    },
                ],
            },
            serde_json::json!("a"),
        );
        assert!(coerce(&d, &serde_json::json!("b")).is_ok());
        assert!(coerce(&d, &serde_json::json!("c")).is_err());
    }

    #[test]
    fn colours_must_be_hex() {
        let d = decl(SettingKind::Color, serde_json::json!("#fff"));
        assert_eq!(
            coerce(&d, &serde_json::json!("#4F8CFF")).unwrap().value,
            serde_json::json!("#4f8cff")
        );
        assert!(coerce(&d, &serde_json::json!("cornflowerblue")).is_err());
        assert!(coerce(&d, &serde_json::json!("#12345")).is_err());
    }

    #[test]
    fn a_stored_value_that_no_longer_fits_falls_back_to_the_default() {
        // v2 of a pack narrowed the choices; the stored value is now invalid.
        let d = decl(
            SettingKind::Select {
                options: vec![SelectOption {
                    value: "a".into(),
                    label: "A".into(),
                }],
            },
            serde_json::json!("a"),
        );
        let r = resolve(&d, Some(&serde_json::json!("gone")));
        assert_eq!(r.value, serde_json::json!("a"));
        assert!(
            r.notice.is_some(),
            "a reset the user did not ask for must be visible"
        );

        // …and writing it is still refused, so the invalid value cannot return.
        assert!(coerce(&d, &serde_json::json!("gone")).is_err());
    }

    #[test]
    fn an_unset_value_resolves_to_the_declared_default() {
        let d = decl(SettingKind::Bool, Value::Bool(true));
        assert_eq!(resolve(&d, None).value, Value::Bool(true));
        assert_eq!(resolve(&d, Some(&Value::Null)).value, Value::Bool(true));
    }

    #[test]
    fn a_nonsense_default_still_yields_a_renderable_value() {
        // A pack author who typed the wrong default must not produce a control
        // with nothing to draw.
        let d = decl(SettingKind::Bool, serde_json::json!("yes"));
        assert_eq!(resolve(&d, None).value, Value::Bool(false));

        let d = decl(
            SettingKind::Number {
                min: Some(4.0),
                max: None,
            },
            Value::Null,
        );
        assert_eq!(resolve(&d, None).value, serde_json::json!(4.0));
    }

    #[test]
    fn an_unsupported_kind_is_never_written_but_never_destroyed() {
        let d = decl(SettingKind::Unsupported, Value::Null);
        assert!(
            coerce(&d, &serde_json::json!("anything")).is_err(),
            "this build cannot validate what it does not understand"
        );
        // A newer build's value survives a round trip through an older one.
        let stored = serde_json::json!({ "rows": [1, 2] });
        assert_eq!(resolve(&d, Some(&stored)).value, stored);
    }
}
