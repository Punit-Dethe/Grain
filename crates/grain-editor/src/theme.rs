//! [GRAIN] The editor's design language (EXECUTION-PLAN.md P4, Mem-inspired):
//! warm paper surfaces, quiet hairlines, one orange accent. Every pane pulls
//! from this palette — no ad-hoc colors in view code, so the language stays
//! coherent as the UI grows.

use floem::peniko::Color;

/// App background — warm cream, the "paper" everything sits on.
pub const BG: Color = Color::rgb8(0xF6, 0xF2, 0xEA);
/// Sidebar wash, one step deeper than the paper.
pub const SIDEBAR_BG: Color = Color::rgb8(0xEF, 0xE9, 0xDD);
/// Raised surfaces: the editor sheet, cards, the chat panel.
pub const SURFACE: Color = Color::rgb8(0xFF, 0xFE, 0xFB);
/// Primary text.
pub const INK: Color = Color::rgb8(0x2A, 0x28, 0x24);
/// Secondary text: section headers, hints, timestamps.
pub const MUTED: Color = Color::rgb8(0x8B, 0x86, 0x7B);
/// The one accent — Grain orange.
pub const ACCENT: Color = Color::rgb8(0xE8, 0x59, 0x1C);
/// Accent wash for selected/active rows.
pub const ACCENT_SOFT: Color = Color::rgb8(0xF9, 0xE3, 0xD6);
/// Hairline borders between panes and around inputs.
pub const HAIRLINE: Color = Color::rgb8(0xE3, 0xDC, 0xCD);
/// Hover wash on interactive rows.
pub const HOVER: Color = Color::rgb8(0xE9, 0xE2, 0xD3);
/// Skeleton blocks in scaffolded (not yet functional) areas.
pub const SKELETON: Color = Color::rgb8(0xEE, 0xEA, 0xE1);

pub const FONT_UI: f32 = 13.0;
pub const FONT_SMALL: f32 = 11.5;
pub const FONT_EDITOR: f32 = 15.0;
pub const FONT_TITLE: f32 = 16.0;

pub const SIDEBAR_W: f64 = 248.0;
pub const CHAT_W: f64 = 320.0;
pub const RADIUS: f64 = 8.0;
