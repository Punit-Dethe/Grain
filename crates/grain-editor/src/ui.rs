//! [GRAIN] The editor shell (EXECUTION-PLAN.md P4, Mem-inspired): three
//! panes — sidebar (create / pinned / notes / collections), the editor
//! sheet, and a toggleable chat panel that is pure SCAFFOLDING for now
//! (slides in and out; nothing inside it works yet, by design).

use std::rc::Rc;
use std::time::Duration;

use floem::action::debounce_action;
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::style::Transition;
use floem::text::Weight;
use floem::views::text_editor::text_editor;

use crate::theme::*;
use crate::vault::{self, NoteMeta, VaultConfig};

#[derive(Clone)]
struct AppState {
    cfg: Rc<VaultConfig>,
    notes: RwSignal<Vec<NoteMeta>>,
    /// Selection key = vault-relative path.
    selected: RwSignal<Option<String>>,
    chat_open: RwSignal<bool>,
    /// Collection names currently expanded in the sidebar.
    expanded: RwSignal<Vec<String>>,
}

impl AppState {
    fn new(cfg: VaultConfig) -> Self {
        let notes = vault::scan(&cfg.root);
        // Dev smoke hook: `GRAIN_EDITOR_SMOKE=1` opens the newest note and
        // the chat panel at startup so one screenshot verifies every pane.
        let smoke = std::env::var_os("GRAIN_EDITOR_SMOKE").is_some();
        let selected = smoke
            .then(|| notes.first().map(|n| n.rel.clone()))
            .flatten();
        AppState {
            cfg: Rc::new(cfg),
            notes: RwSignal::new(notes),
            selected: RwSignal::new(selected),
            chat_open: RwSignal::new(smoke),
            expanded: RwSignal::new(Vec::new()),
        }
    }

    fn rescan(&self) {
        self.notes.set(vault::scan(&self.cfg.root));
    }
}

pub fn app_view(cfg: VaultConfig) -> impl IntoView {
    let state = AppState::new(cfg);
    h_stack((
        sidebar(state.clone()),
        editor_pane(state.clone()),
        chat_panel(state),
    ))
    .style(|s| s.size_full().background(BG).color(INK).font_size(FONT_UI))
}

// -- sidebar ----------------------------------------------------------------------

fn sidebar(state: AppState) -> impl IntoView {
    let label_text = state.cfg.label.clone();
    v_stack((
        // Vault name = the "account" header slot in the inspiration.
        label(move || label_text.clone()).style(|s| {
            s.font_size(FONT_TITLE)
                .font_weight(Weight::SEMIBOLD)
                .padding_horiz(14.0)
                .padding_top(16.0)
                .padding_bottom(10.0)
        }),
        create_note_button(state.clone()),
        scroll(sidebar_lists(state)).style(|s| s.flex_grow(1.0).width_full()),
    ))
    .style(|s| {
        s.width(SIDEBAR_W)
            .height_full()
            .flex_col()
            .background(SIDEBAR_BG)
            .border_right(1.0)
            .border_color(HAIRLINE)
    })
}

fn create_note_button(state: AppState) -> impl IntoView {
    container(
        label(|| "＋  Create note".to_string())
            .style(|s| {
                s.padding_vert(8.0)
                    .padding_horiz(12.0)
                    .width_full()
                    .background(SURFACE)
                    .border(1.0)
                    .border_color(HAIRLINE)
                    .border_radius(RADIUS + 4.0)
                    .font_weight(Weight::MEDIUM)
                    .cursor(CursorStyle::Pointer)
                    .hover(|s| s.background(ACCENT_SOFT).border_color(ACCENT))
            })
            .on_click_stop(move |_| {
                if let Some(new) = vault::create_note(&state.cfg) {
                    state.rescan();
                    state.selected.set(Some(new.rel));
                }
            }),
    )
    .style(|s| s.width_full().padding_horiz(12.0).padding_bottom(12.0))
}

/// The scrollable Pinned / Notes / Collections stack. Rebuilt when the note
/// set or the expanded collections change; selection highlighting is reactive
/// per-row (style closures), so clicking around never rebuilds the list.
fn sidebar_lists(state: AppState) -> impl IntoView {
    dyn_container(
        {
            let state = state.clone();
            move || (state.notes.get(), state.expanded.get())
        },
        move |(notes, expanded)| {
            let pinned: Vec<&NoteMeta> = notes.iter().filter(|n| n.pinned).collect();
            let root_notes: Vec<&NoteMeta> =
                notes.iter().filter(|n| n.collection.is_none()).collect();
            let mut collections: Vec<String> =
                notes.iter().filter_map(|n| n.collection.clone()).collect();
            collections.sort();
            collections.dedup();

            let pinned_section: Box<dyn View> = if pinned.is_empty() {
                Box::new(
                    label(|| "Pin notes to keep them handy".to_string())
                        .style(|s| s.color(MUTED).font_size(FONT_SMALL).padding_horiz(14.0)),
                )
            } else {
                Box::new(v_stack_from_iter(
                    pinned
                        .iter()
                        .map(|n| note_row(state.clone(), (*n).clone(), false)),
                ))
            };

            let notes_section: Box<dyn View> = if root_notes.is_empty() {
                Box::new(
                    label(|| "Notes you create land here".to_string())
                        .style(|s| s.color(MUTED).font_size(FONT_SMALL).padding_horiz(14.0)),
                )
            } else {
                Box::new(v_stack_from_iter(
                    root_notes
                        .iter()
                        .map(|n| note_row(state.clone(), (*n).clone(), false)),
                ))
            };

            let collections_section = v_stack_from_iter(collections.into_iter().map(|name| {
                let members: Vec<NoteMeta> = notes
                    .iter()
                    .filter(|n| n.collection.as_deref() == Some(name.as_str()))
                    .cloned()
                    .collect();
                let open = expanded.contains(&name);
                collection_block(state.clone(), name, members, open)
            }));

            v_stack((
                section_header("Pinned"),
                pinned_section,
                section_header("Notes"),
                notes_section,
                section_header("Collections"),
                collections_section,
            ))
            .style(|s| s.flex_col().width_full().padding_bottom(16.0))
        },
    )
    .style(|s| s.width_full())
}

fn section_header(name: &'static str) -> impl IntoView {
    label(move || name.to_uppercase()).style(|s| {
        s.color(MUTED)
            .font_size(FONT_SMALL)
            .font_weight(Weight::SEMIBOLD)
            .padding_horiz(14.0)
            .padding_top(16.0)
            .padding_bottom(6.0)
    })
}

/// A collection header row plus (when expanded) its indented member notes.
fn collection_block(
    state: AppState,
    name: String,
    members: Vec<NoteMeta>,
    open: bool,
) -> impl IntoView {
    let count = members.len();
    let header_name = name.clone();
    let toggle_name = name.clone();
    let rows_state = state.clone();
    let header = h_stack((
        label(move || if open { "▾" } else { "▸" }.to_string())
            .style(|s| s.color(MUTED).font_size(FONT_SMALL).width(14.0)),
        label(move || format!("#  {header_name}")).style(|s| s.font_weight(Weight::MEDIUM)),
        label(move || format!("{count}")).style(|s| {
            s.color(MUTED)
                .font_size(FONT_SMALL)
                .margin_left(auto_margin())
        }),
    ))
    .style(row_style)
    .on_click_stop(move |_| {
        state.expanded.update(|ex| {
            if let Some(i) = ex.iter().position(|n| n == &toggle_name) {
                ex.remove(i);
            } else {
                ex.push(toggle_name.clone());
            }
        });
    });

    let body: Box<dyn View> = if open {
        Box::new(v_stack_from_iter(
            members
                .into_iter()
                .map(move |n| note_row(rows_state.clone(), n, true)),
        ))
    } else {
        Box::new(empty())
    };
    v_stack((header, body)).style(|s| s.flex_col().width_full())
}

fn note_row(state: AppState, note: NoteMeta, indented: bool) -> impl IntoView {
    let rel = note.rel.clone();
    let sel_rel = note.rel.clone();
    let selected = state.selected;
    let title = note.title.clone();
    let age = vault::rel_age(note.mtime_ms);
    let pinned = note.pinned;
    h_stack((
        label(move || if pinned { "◆" } else { "·" }.to_string()).style(move |s| {
            s.width(14.0)
                .font_size(FONT_SMALL)
                .color(if pinned { ACCENT } else { MUTED })
        }),
        label(move || title.clone()).style(|s| {
            s.text_ellipsis()
                .flex_grow(1.0)
                .flex_basis(0.0)
                .min_width(0.0)
        }),
        label(move || age.clone()).style(|s| s.color(MUTED).font_size(FONT_SMALL)),
    ))
    .style(move |s| {
        let is_sel = selected.get().as_deref() == Some(sel_rel.as_str());
        row_style(s)
            .apply_if(indented, |s| s.margin_left(16.0))
            .apply_if(is_sel, |s| {
                s.background(ACCENT_SOFT).font_weight(Weight::MEDIUM)
            })
    })
    .on_click_stop(move |_| {
        state.selected.set(Some(rel.clone()));
    })
}

fn row_style(s: floem::style::Style) -> floem::style::Style {
    s.items_center()
        .width_full()
        .padding_vert(6.0)
        .padding_horiz(10.0)
        .margin_horiz(6.0)
        .border_radius(RADIUS)
        .cursor(CursorStyle::Pointer)
        .hover(|s| s.background(HOVER))
}

// -- editor pane ------------------------------------------------------------------

fn editor_pane(state: AppState) -> impl IntoView {
    v_stack((top_bar(state.clone()), editor_body(state))).style(|s| {
        s.flex_col()
            .flex_grow(1.0)
            .flex_basis(0.0)
            .min_width(0.0)
            .height_full()
    })
}

fn top_bar(state: AppState) -> impl IntoView {
    let notes = state.notes;
    let selected = state.selected;
    let chat_open = state.chat_open;
    let vault_label = state.cfg.label.clone();
    h_stack((
        label(move || {
            selected
                .get()
                .and_then(|rel| {
                    notes.with(|ns| ns.iter().find(|n| n.rel == rel).map(|n| n.title.clone()))
                })
                .unwrap_or_else(|| vault_label.clone())
        })
        .style(|s| {
            s.font_weight(Weight::SEMIBOLD)
                .font_size(FONT_UI)
                .text_ellipsis()
                .min_width(0.0)
        }),
        label(move || {
            selected
                .get()
                .and_then(|rel| {
                    notes.with(|ns| {
                        ns.iter()
                            .find(|n| n.rel == rel)
                            .and_then(|n| n.collection.clone())
                    })
                })
                .map(|c| format!("#{c}"))
                .unwrap_or_default()
        })
        .style(|s| {
            s.color(ACCENT)
                .font_size(FONT_SMALL)
                .margin_left(8.0)
                .flex_grow(1.0)
        }),
        label(move || {
            if chat_open.get() {
                "Chat ▸".to_string()
            } else {
                "◂ Chat".to_string()
            }
        })
        .style(|s| {
            s.padding_vert(5.0)
                .padding_horiz(12.0)
                .border(1.0)
                .border_color(HAIRLINE)
                .border_radius(RADIUS + 4.0)
                .background(SURFACE)
                .cursor(CursorStyle::Pointer)
                .hover(|s| s.background(ACCENT_SOFT).border_color(ACCENT))
        })
        .on_click_stop(move |_| chat_open.update(|o| *o = !*o)),
    ))
    .style(|s| {
        s.items_center()
            .width_full()
            .padding_horiz(18.0)
            .padding_vert(10.0)
            .border_bottom(1.0)
            .border_color(HAIRLINE)
    })
}

fn editor_body(state: AppState) -> impl IntoView {
    let selected = state.selected;
    dyn_container(
        move || selected.get(),
        move |sel| {
            let Some(rel) = sel else {
                return empty_state().into_any();
            };
            let Some(note) = state
                .notes
                .with(|ns| ns.iter().find(|n| n.rel == rel).cloned())
            else {
                return empty_state().into_any();
            };
            note_editor(note).into_any()
        },
    )
    .style(|s| {
        s.flex_grow(1.0)
            .flex_basis(0.0)
            .min_height(0.0)
            .width_full()
    })
}

fn empty_state() -> impl IntoView {
    v_stack((
        label(|| "Nothing open".to_string()).style(|s| {
            s.font_size(FONT_TITLE)
                .font_weight(Weight::SEMIBOLD)
                .color(MUTED)
        }),
        label(|| "Pick a note from the sidebar, or create one.".to_string())
            .style(|s| s.color(MUTED).margin_top(6.0)),
    ))
    .style(|s| s.size_full().flex_col().items_center().justify_center())
}

/// The editor sheet: the note's FULL file text (frontmatter included — no
/// hidden mutations) in a live buffer, auto-saved atomically after typing
/// pauses. The decoration layer (milestones 2–4) will style this buffer.
fn note_editor(note: NoteMeta) -> impl IntoView {
    let text = vault::read_text(&note.abs);
    let edits = RwSignal::new(0u64);
    let editor = text_editor(text)
        .placeholder("Start writing…")
        .update(move |_| edits.update(|n| *n += 1))
        .editor_style(|s| {
            s.hide_gutter(true)
                .cursor_color(ACCENT)
                .selection_color(ACCENT_SOFT)
        })
        .style(|s| s.size_full().font_size(FONT_EDITOR));
    let doc = editor.doc();
    let abs = note.abs.clone();
    debounce_action(edits, Duration::from_millis(600), move || {
        // The updater runs once at setup — only persist real keystrokes.
        if edits.get_untracked() == 0 {
            return;
        }
        vault::save_text(&abs, &doc.text().to_string());
    });

    container(editor).style(|s| {
        s.size_full()
            .padding(18.0)
            .background(SURFACE)
            .border(1.0)
            .border_color(HAIRLINE)
            .border_radius(RADIUS + 4.0)
            .margin(16.0)
    })
}

// -- chat panel (pure scaffolding) --------------------------------------------------

/// Slides in/out via an animated width on the clipped shell; the inner panel
/// keeps a FIXED width so its content never reflows mid-slide. Nothing in it
/// is functional yet — skeleton blocks mark where the conversation will live.
fn chat_panel(state: AppState) -> impl IntoView {
    let chat_open = state.chat_open;
    clip(
        v_stack((
            h_stack((
                label(|| "Chat".to_string()).style(|s| {
                    s.font_weight(Weight::SEMIBOLD)
                        .font_size(FONT_UI)
                        .flex_grow(1.0)
                }),
                label(|| "scaffold".to_string()).style(|s| {
                    s.color(MUTED)
                        .font_size(FONT_SMALL)
                        .padding_horiz(8.0)
                        .padding_vert(2.0)
                        .border(1.0)
                        .border_color(HAIRLINE)
                        .border_radius(RADIUS + 4.0)
                }),
            ))
            .style(|s| {
                s.items_center()
                    .width_full()
                    .padding_horiz(16.0)
                    .padding_vert(12.0)
                    .border_bottom(1.0)
                    .border_color(HAIRLINE)
            }),
            v_stack((
                skeleton_bubble(0.72, false),
                skeleton_bubble(0.55, true),
                skeleton_bubble(0.80, false),
                skeleton_bubble(0.40, true),
            ))
            .style(|s| {
                s.flex_col()
                    .flex_grow(1.0)
                    .width_full()
                    .padding(16.0)
                    .gap(10.0)
            }),
            container(label(|| "Ask your Grain…".to_string()).style(|s| {
                s.color(MUTED)
                    .width_full()
                    .padding_vert(9.0)
                    .padding_horiz(12.0)
                    .background(BG)
                    .border(1.0)
                    .border_color(HAIRLINE)
                    .border_radius(RADIUS + 6.0)
            }))
            .style(|s| s.width_full().padding(12.0)),
        ))
        .style(|s| {
            s.width(CHAT_W)
                .height_full()
                .flex_col()
                .background(SURFACE)
                .border_left(1.0)
                .border_color(HAIRLINE)
        }),
    )
    .style(move |s| {
        s.width(if chat_open.get() { CHAT_W } else { 0.0 })
            .height_full()
            .transition_width(Transition::ease_in_out(Duration::from_millis(220)))
    })
}

/// A placeholder "message": rounded muted block, alternating sides.
fn skeleton_bubble(width_frac: f64, right: bool) -> impl IntoView {
    container(empty().style(move |s| {
        s.width(CHAT_W * width_frac)
            .height(34.0)
            .background(SKELETON)
            .border_radius(RADIUS + 4.0)
    }))
    .style(move |s| {
        s.width_full()
            .apply_if(right, |s| s.justify_end())
            .apply_if(!right, |s| s.justify_start())
    })
}

// -- small helpers ------------------------------------------------------------------

/// `margin-left: auto` for pushing trailing labels to the row's edge.
fn auto_margin() -> floem::unit::PxPctAuto {
    floem::unit::PxPctAuto::Auto
}
