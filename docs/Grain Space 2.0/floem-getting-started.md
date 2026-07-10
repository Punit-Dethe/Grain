# Floem — Getting Started Reference (for Grain's editor process)

This is a working reference to get you from zero to a running Floem window with an
editable text buffer, oriented specifically toward building the Obsidian-style
markdown editor as the "heavy" process in your two-process architecture.

Floem is still pre-1.0 and described by its own maintainers as "still maturing... we
will make occasional breaking changes." Treat every code snippet here as a starting
point, not gospel — always cross-check against `docs.rs/floem` (select the exact
version you're pinned to) and the `examples/` folder in the repo before you rely on
an API shape.

- Repo: https://github.com/lapce/floem
- Docs: https://docs.rs/floem
- Examples: https://github.com/lapce/floem/tree/main/examples
- Discord: `#floem` channel, linked from the repo README
- Reference implementation for the text engine: https://github.com/lapce/lapce (uses the exact same `text_editor` view)

---

## 1. Mental model

Floem is a **retained-tree, fine-grained-reactive** UI library — closer to Leptos/Xilem
than to React or egui. Two things to internalize before writing any code:

1. **The view tree is built once.** Unlike egui (which rebuilds and re-lays-out
   everything every frame), Floem constructs your view hierarchy a single time. After
   that, changes propagate through **reactive signals** — only the parts of the UI
   that actually depend on a changed signal re-run. This is why Floem can stay cheap
   at idle: no work happens unless state actually changes.
2. **Signals, not callbacks-first.** State lives in `RwSignal<T>` values. Views read
   signals inside closures; Floem tracks that dependency automatically and re-renders
   only that view when the signal updates. You don't manually wire up "on change"
   plumbing the way you would in an imperative UI toolkit.

This model maps well onto your Obsidian-style decoration layer: the raw markdown
text is one signal, the parsed decoration spans (bold ranges, hidden-marker ranges,
etc.) are a derived signal computed from it, and the renderer just reacts to both.

---

## 2. Project setup

```toml
# Cargo.toml
[dependencies]
floem = "0.2"   # check crates.io for the current version before pinning
```

Floem pulls in its own forked `winit` (`floem-winit`) and its own text stack
(`floem-cosmic-text`, a fork of `cosmic-text` maintained by cosmic-text's original
author). You generally don't need to depend on these directly — `floem` re-exports
what you need — but it's useful to know they're there when you hit docs for
`FontSystem`, `Buffer`, `Attrs`, etc.

Minimum Rust version tracks recent stable; check the workspace `Cargo.toml` in the
Floem repo for the exact `rust-version` field if you hit a compiler error.

---

## 3. Hello window

```rust
use floem::prelude::*;

fn main() {
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let counter = RwSignal::new(0);

    h_stack((
        button("Increment").action(move || counter.set(counter.get() + 1)),
        label(move || format!("Value: {}", counter.get())),
        button("Decrement").action(move || counter.set(counter.get() - 1)),
    ))
    .style(|s| s.size_full().items_center().justify_center().gap(10))
}
```

Notes:
- `floem::launch` takes a function returning `impl IntoView` — that's your window root.
- `h_stack` / `v_stack` are horizontal/vertical layout containers (Taffy-flexbox
  under the hood). Older examples use `Stack::horizontal(...)`; if that doesn't
  compile against your pinned version, fall back to `h_stack`/`v_stack` — this is
  exactly the kind of breaking-change drift the maintainers warn about.
- `.style(|s| ...)` is the universal styling entry point on every view — Tailwind-ish
  chainable methods (`size_full`, `items_center`, `justify_center`, `gap`, `padding`,
  `background`, `border`, etc.).

---

## 4. Reactive signals, in more depth

```rust
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};

let text = RwSignal::new(String::new());

// Read
let current = text.get();          // clones out the value
let len = text.with(|t| t.len());  // read without cloning

// Write
text.set("hello".to_string());
text.update(|t| t.push_str(" world"));
```

For values derived from other signals (e.g., parsed markdown spans derived from raw
text), you generally just read the source signal inside a closure passed to a view
or a `create_memo`-style derived computation — Floem's reactive core tracks the
dependency graph for you, the same pattern as Leptos.

---

## 5. Layout

Floem layout is Taffy-based, so if you know CSS flexbox/grid, you already know this.
Key style methods you'll use constantly:

```rust
some_view.style(|s| {
    s.flex_col()              // or .flex_row()
     .width_full()
     .height(400.0)
     .padding(12.0)
     .gap(8.0)
     .items_start()
     .justify_between()
     .background(Color::rgb8(30, 30, 30))
     .border(1.0)
     .border_radius(6.0)
});
```

Conditional/dynamic styling reads a signal inside the closure — since the closure
re-runs when its signal dependencies change, styles update reactively without any
manual "force redraw" call.

---

## 6. The text editor — this is the part you actually care about

Floem exposes the editor Lapce itself is built on through two related pieces:

- `floem::views::text_editor` — the high-level view you drop into your UI.
- `floem::views::editor` — the lower-level editor machinery (state, actions, text
  layout) that `text_editor` is built on, which you'll dig into once you need custom
  behavior (decorations, custom keymaps, custom rendering of spans).
- Underneath both: `floem-editor-core` (the editor state/rope logic extracted as its
  own crate), `lapce-xi-rope` (the actual rope data structure — Xi-editor lineage),
  and `floem-cosmic-text` (shaping, font fallback, glyph layout, bidi).

A minimal editable buffer:

```rust
use floem::prelude::*;
use floem::views::text_editor::text_editor;

fn markdown_editor_view() -> impl IntoView {
    text_editor("")
        .placeholder("Start writing...")
        .style(|s| s.size_full())
}
```

From here, the path to Obsidian-style live preview is:

1. **Get read access to the document.** The editor's underlying buffer is your
   source of truth — you don't maintain a separate `String` in parallel; you read
   from the editor's own rope/doc state.
2. **Run your markdown parser on change.** Feed the current buffer content through
   your parser (e.g. `pulldown-cmark` or `comrak`) whenever it changes, producing a
   list of spans: `(range, style)` pairs — bold ranges, heading ranges, link ranges,
   syntax-marker ranges (the `**` / `#` characters themselves) that should be hidden
   or dimmed when the cursor isn't on that line.
3. **Apply spans as text-layout attributes**, the same mechanism Lapce already uses
   for syntax-highlighted code — this is the "decoration layer" pattern, and it's
   the exact same primitive as code-editor syntax highlighting, just driven by a
   markdown parser instead of tree-sitter.
4. **Gate marker-hiding on cursor position.** Track the editor's current cursor
   line (the editor exposes cursor/selection state); when computing which spans to
   actually hide vs. show, check whether the cursor is inside that span's line and
   render the raw markup there instead of the collapsed/styled form.

This is genuinely the crux of the whole project, and no framework hands it to you
pre-built — but you're building it on top of the same rope + span-decoration
machinery a full IDE editor already relies on, not on top of raw drawing primitives.

---

## 7. Scrolling and large documents

```rust
use floem::views::Decorators;

scroll(your_content_view).style(|s| s.size_full())
```

For anything that renders a large list of discrete items (e.g., a file/note list in
a sidebar, not the editor buffer itself, which the text editor view already
virtualizes internally), use the `virtual_list` example in the repo as your
reference — it only pays render cost for visible rows.

---

## 8. Theming

```rust
use floem::style::Style;

// Classes let you define reusable style rules, similar to CSS classes,
// and Floem ships light/dark theme support out of the box.
```

Check `examples/` for the current theming API shape — this is one of the areas most
likely to have shifted between versions given the "occasional breaking changes"
warning.

---

## 9. Debugging

Floem ships a built-in **element inspector** (dev-tools-style, inspect the live
layout tree of a running window) — invaluable once your view tree gets deep. Check
current docs for how it's enabled in your pinned version (this has moved around
across releases).

---

## 10. Rendering backends (relevant to your pill/editor split)

Floem picks a backend automatically but you can influence it:

- **wgpu** via `vger` or `vello` — GPU-accelerated, this is what you'll use for the
  editor process.
- **tiny-skia** — pure CPU rasterizer, used automatically when no GPU is available.
  Relevant if you ever reconsider using Floem for the pill process too (you likely
  won't, given Slint's head start there, but it's an option in reserve if you ever
  want to consolidate onto one framework later).

---

## 11. Suggested first milestones for Grain

1. Get a blank Floem window rendering with `floem::launch`, confirm it builds and
   runs on all three target OSes early — don't wait until the app is complex to
   discover a platform-specific build issue.
2. Drop in `text_editor` with a hardcoded string, confirm typing/editing/scrolling
   works out of the box with zero customization.
3. Wire up your markdown parser (pick `pulldown-cmark` for speed/simplicity or
   `comrak` if you want GFM extensions like tables/footnotes built in) and log the
   parsed spans to console — don't touch rendering yet, just prove the parse step
   is fast enough to run on every keystroke.
4. Read through `floem::views::editor`'s source for how Lapce applies syntax-
   highlight spans to the text layout — this is your template for applying markdown
   spans the same way.
5. Build the cursor-position gating (show raw markup on the active line, collapse
   everywhere else) as the last step, once static span-based styling is working.
6. Only after the editor process is solid, build the IPC bridge back to your pill
   daemon.

Step 3 and 4 are where you'll actually spend most of your time — budget accordingly.

---

## 12. Known rough edges to plan around

- Pre-1.0, expect breaking changes on `cargo update` — pin your version and update
  deliberately, don't float on `"*"`.
- Documentation is thinner than a mature toolkit like Qt/GTK — the `examples/`
  folder and Lapce's own source are your primary references more often than
  prose docs.
- WASM support exists but is marked experimental — irrelevant to you unless Grain
  grows a web target later.
- Vello renderer support exists behind a feature flag but isn't the default yet —
  stick with the default backend unless you have a specific reason to opt in.
