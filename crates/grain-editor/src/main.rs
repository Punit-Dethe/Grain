//! [GRAIN] grain-editor — the on-demand Floem editor process
//! (EXECUTION-PLAN.md P4 / OBSIDIAN_ROADMAP.md Phase 3).
//!
//! Opens the same vault the main app uses (explicit path arg → main app's
//! settings → the native Grain vault) in a Mem-inspired three-pane shell:
//! sidebar (pinned / notes / collections), editor sheet with atomic
//! auto-save, and a toggleable chat panel scaffold. The live-preview
//! decoration layer and the pill-daemon IPC come next.

mod markdown;
mod theme;
mod ui;
mod vault;

use floem::kurbo::Size;
use floem::window::WindowConfig;

fn main() {
    let cfg = vault::resolve_vault();
    eprintln!(
        "[grain-editor] vault: {} ({} notes)",
        cfg.root.display(),
        vault::scan(&cfg.root).len()
    );
    floem::Application::new()
        .window(
            move |_| ui::app_view(cfg.clone()),
            Some(
                WindowConfig::default()
                    .size(Size::new(1180.0, 760.0))
                    .title("Grain"),
            ),
        )
        .run();
}
