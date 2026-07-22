//! [GRAIN] Pill theme (SPEC §9) — declarative, data only.
//!
//! **No extension code ever runs in the pill process.** A theme is colours plus
//! a named animation pattern per state; it travels to `grain-pill` inside a
//! [`crate::DaemonEvent::PillTheme`] and is rendered by Grain's own code. The
//! per-dot expression evaluator SPEC §9 also describes is a later addition —
//! this v1 is the "calculator, not an engine" subset: three named patterns.
//!
//! Every field is optional and degrades toward Grain's own look:
//! - a theme that restyles only Recording leaves the other three states alone
//!   (SPEC §9: "missing state → Grain's default FOR THAT STATE");
//! - a state that sets a `dot` colour but no `background` keeps Grain's
//!   background;
//! - a `pattern` written against a newer contract deserializes to
//!   [`PillPattern::Unsupported`] rather than failing the whole theme, and the
//!   renderer treats it as the state's default motion.
//!
//! **The pill must always render** — there is no theme, however malformed, that
//! can produce a blank pill, because every gap is a fallback, never an error.

use serde::{Deserialize, Serialize};

/// A named animation pattern. Deliberately tiny (R1: widen with real consumers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PillPattern {
    /// A solid field at the dot colour over a gentle breathing base.
    #[default]
    Static,
    /// The whole field pulses together.
    Breathe,
    /// A soft band crosses the field (the motion Grain's own idle uses).
    Sweep,
    /// A pattern this build doesn't know. Kept (not rejected) so a theme written
    /// against a newer contract still installs and shows; the renderer falls
    /// back to the state's default motion.
    #[serde(other)]
    Unsupported,
}

/// How one pill state looks. Any omitted field falls back to Grain's default for
/// that state, so a partial theme is a valid theme.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PillStateTheme {
    /// Background RGBA behind the dots. `None` keeps Grain's background.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<[u8; 4]>,
    /// The lit-dot colour (RGB). Per-dot alpha stays the animation's business, so
    /// a theme sets a hue and the motion keeps its shape. `None` keeps Grain's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot: Option<[u8; 3]>,
    /// The animation. Defaults to `Static` when a state is themed at all.
    #[serde(default)]
    pub pattern: PillPattern,
}

/// A pill theme: an optional look per state (SPEC §9). A `None` state is Grain's
/// own — a theme is never required to restyle everything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PillTheme {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle: Option<PillStateTheme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<PillStateTheme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing: Option<PillStateTheme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<PillStateTheme>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_theme_is_valid_and_leaves_other_states_none() {
        let json = r#"{"recording":{"dot":[110,162,224],"pattern":"breathe"}}"#;
        let t: PillTheme = serde_json::from_str(json).unwrap();
        let rec = t.recording.unwrap();
        assert_eq!(rec.dot, Some([110, 162, 224]));
        assert_eq!(rec.pattern, PillPattern::Breathe);
        assert!(
            rec.background.is_none(),
            "an unset background keeps Grain's"
        );
        assert!(t.idle.is_none(), "an unstyled state stays Grain's own");
    }

    #[test]
    fn an_unknown_pattern_degrades_rather_than_failing() {
        // A theme written against a future contract must still install.
        let json = r#"{"idle":{"pattern":"kaleidoscope"}}"#;
        let t: PillTheme = serde_json::from_str(json).unwrap();
        assert_eq!(t.idle.unwrap().pattern, PillPattern::Unsupported);
    }

    #[test]
    fn a_themed_state_with_no_pattern_defaults_to_static() {
        let json = r#"{"idle":{"dot":[10,20,30]}}"#;
        let t: PillTheme = serde_json::from_str(json).unwrap();
        assert_eq!(t.idle.unwrap().pattern, PillPattern::Static);
    }

    #[test]
    fn an_empty_object_is_the_all_default_theme() {
        let t: PillTheme = serde_json::from_str("{}").unwrap();
        assert_eq!(t, PillTheme::default());
    }
}
