//! [GRAIN] Pill skin — which BODY LOOK the collapsed pill wears.
//!
//! Orthogonal to [`crate::PillTheme`], and deliberately so:
//! - a **skin** is Grain's own built-in *form* — the pill's geometry and the
//!   shape of its voice visualisation. The user picks one in Settings.
//! - a **theme** is an extension's *colours* for a given state, painted into
//!   whatever form the skin defines.
//!
//! Keeping them apart means a theme keeps working when the user switches skin,
//! and a new skin costs no theme migration. The skin crosses the wire inside
//! [`crate::DaemonEvent::PillSkin`] and is also the persisted `pill_skin`
//! setting (grain-core re-exports it).

use serde::{Deserialize, Serialize};

/// The collapsed pill's body look. Adding a variant here is the ONLY thing a new
/// pill look must touch in the protocol; the renderer owns everything else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PillSkin {
    /// **Default.** A compact capsule with a smooth, centre-mirrored waveform —
    /// the quiet, professional look. 20% smaller than [`PillSkin::Matrix`].
    #[default]
    Wave,
    /// The original dot-matrix aura: an 25x8 grid of dots whose density tracks
    /// the voice. Kept as a selectable look, no longer the default.
    Matrix,
}

impl PillSkin {
    /// Parse the persisted / command-passed wire name. An unknown value is the
    /// default skin rather than an error — a settings file written by a newer
    /// build must never leave the user without a pill.
    pub fn from_wire(s: &str) -> Self {
        match s {
            "matrix" => Self::Matrix,
            _ => Self::Wave,
        }
    }

    /// The wire name (matches the serde representation).
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Wave => "wave",
            Self::Matrix => "matrix",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_is_the_default_skin() {
        assert_eq!(PillSkin::default(), PillSkin::Wave);
    }

    #[test]
    fn wire_names_round_trip_through_serde_and_the_helpers() {
        for skin in [PillSkin::Wave, PillSkin::Matrix] {
            let json = serde_json::to_string(&skin).unwrap();
            assert_eq!(json, format!("\"{}\"", skin.as_wire()));
            assert_eq!(serde_json::from_str::<PillSkin>(&json).unwrap(), skin);
            assert_eq!(PillSkin::from_wire(skin.as_wire()), skin);
        }
    }

    #[test]
    fn an_unknown_skin_name_degrades_to_the_default() {
        // A settings file from a newer build must still produce a working pill.
        assert_eq!(PillSkin::from_wire("hologram"), PillSkin::Wave);
    }
}
