import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Annotation, EditorState } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder as cmPlaceholder,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import {
  HighlightStyle,
  syntaxHighlighting,
  syntaxTree,
} from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { grainMarkdownExtensions, grainTags } from "./markdownExtensions";
import {
  insertBlock,
  insertLink,
  setHeading,
  TABLE_TEMPLATE,
  toggleLinePrefix,
  toggleWrap,
} from "./editorCommands";

/**
 * [GRAIN] Obsidian-style live markdown editor for the Grain Space workspace.
 * CodeMirror 6 (the same engine Obsidian is built on): markdown is styled in
 * place (headings grow, **bold** is bold, `code` is mono) and the syntax
 * markers themselves are hidden on every line EXCEPT the one the cursor is on,
 * where they reappear for editing — the "live preview" feel.
 *
 * Grammar comes from ./markdownExtensions (GFM + Obsidian constructs). On top
 * of concealment this adds two always-on rich affordances: task checkboxes
 * render as real, clickable inputs, and `> [!note]` blocks render as callouts.
 *
 * A `forwardRef` handle exposes formatting commands so the toolbar can drive
 * the editor. Default export stays intact so `EditorPane` can `React.lazy`
 * code-split it: this chunk (and its JS heap) never loads until a note opens.
 */

export type EditorHandle = {
  bold(): void;
  italic(): void;
  strikethrough(): void;
  highlight(): void;
  code(): void;
  heading(level: number): void;
  bullet(): void;
  ordered(): void;
  task(): void;
  quote(): void;
  codeBlock(): void;
  table(): void;
  link(): void;
  rule(): void;
};

/** Marks external (prop-driven) doc replacements so they don't echo onChange. */
const External = Annotation.define<boolean>();

/** Node names whose text is concealed away from the cursor's line. */
const HIDDEN_MARKS = new Set([
  "HeaderMark",
  "EmphasisMark",
  "CodeMark",
  "StrikethroughMark",
  "LinkMark",
  "URL",
  "HighlightMark",
  "SubscriptMark",
  "SuperscriptMark",
  "WikilinkMark",
  "CommentMark",
  "MathMark",
  "QuoteMark",
]);

/** Callout header, e.g. `> [!warning]- Title`. */
const CALLOUT_RE = /^\s*>\s*\[!([\w-]+)\]([+-]?)/;

/** A real checkbox standing in for a `- [ ]` / `- [x]` task marker. */
class CheckboxWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly from: number,
    readonly to: number,
  ) {
    super();
  }
  eq(other: CheckboxWidget) {
    return other.checked === this.checked && other.from === this.from;
  }
  toDOM(view: EditorView) {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = this.checked;
    box.className = "gs-cm-check";
    box.setAttribute("aria-label", "Toggle task");
    box.addEventListener("mousedown", (e) => e.preventDefault());
    box.addEventListener("change", () => {
      // Positions may have drifted since render — only act on a live marker.
      const cur = view.state.sliceDoc(this.from, this.to);
      if (!/^\[[ xX]\]$/.test(cur)) return;
      view.dispatch({
        changes: {
          from: this.from,
          to: this.to,
          insert: this.checked ? "[ ]" : "[x]",
        },
        userEvent: "input.toggleTask",
      });
    });
    return box;
  }
  ignoreEvent() {
    return false;
  }
}

/**
 * One plugin, one syntax-tree walk. Produces the whole decoration set:
 * concealed marks (off the cursor line), checkbox widgets, and callout line
 * classes. Kept single-pass so switching notes doesn't multiply CPU cost.
 */
const richDecorations = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        this.decorations = this.build(update.view);
      }
    }

    build(view: EditorView): DecorationSet {
      const doc = view.state.doc;
      const active = new Set<number>();
      for (const range of view.state.selection.ranges) {
        const from = doc.lineAt(range.from).number;
        const to = doc.lineAt(range.to).number;
        for (let l = from; l <= to; l++) active.add(l);
      }

      const decos: {
        from: number;
        to: number;
        deco: Decoration;
      }[] = [];

      const tree = syntaxTree(view.state);
      for (const { from, to } of view.visibleRanges) {
        tree.iterate({
          from,
          to,
          enter: (node) => {
            // Headings get breathing room above them (like every desktop
            // editor). A line class carries the margin; concealment of the
            // `#` marks still happens via the child HeaderMark below.
            if (
              node.name.startsWith("ATXHeading") ||
              node.name.startsWith("SetextHeading")
            ) {
              const ln = doc.lineAt(node.from);
              decos.push({
                from: ln.from,
                to: ln.from,
                deco: Decoration.line({ class: "gs-cm-head" }),
              });
              // fall through — children (HeaderMark) still need concealing.
            }
            // Block backgrounds: fenced code + tables read as distinct slabs.
            if (node.name === "FencedCode" || node.name === "Table") {
              const startLine = doc.lineAt(node.from).number;
              const endPos = Math.min(node.to, doc.length);
              const endLine = doc.lineAt(
                Math.max(node.from, endPos - 1),
              ).number;
              for (let l = startLine; l <= endLine; l++) {
                const ln = doc.line(l);
                let cls =
                  node.name === "Table" ? "gs-cm-tablerow" : "gs-cm-code";
                if (node.name === "FencedCode") {
                  if (l === startLine) cls += " gs-cm-code--top";
                  if (l === endLine) cls += " gs-cm-code--bot";
                }
                decos.push({
                  from: ln.from,
                  to: ln.from,
                  deco: Decoration.line({ class: cls }),
                });
              }
              return;
            }
            // Interactive task checkbox — always on, cursor line included.
            if (node.name === "TaskMarker") {
              const text = doc.sliceString(node.from, node.to);
              const checked = /[xX]/.test(text);
              decos.push({
                from: node.from,
                to: node.to,
                deco: Decoration.replace({
                  widget: new CheckboxWidget(checked, node.from, node.to),
                }),
              });
              return;
            }
            if (!HIDDEN_MARKS.has(node.name)) return;
            const startLine = doc.lineAt(node.from).number;
            const endLine = doc.lineAt(node.to).number;
            for (let l = startLine; l <= endLine; l++) {
              if (active.has(l)) return;
            }
            // Never conceal a blockquote marker that heads/continues a callout.
            if (node.name === "QuoteMark") {
              const lineText = doc.lineAt(node.from).text;
              if (CALLOUT_RE.test(lineText)) return;
            }
            let end = node.to;
            if (
              node.name === "HeaderMark" &&
              doc.sliceString(end, end + 1) === " "
            ) {
              end += 1;
            }
            decos.push({
              from: node.from,
              to: end,
              deco: Decoration.replace({}),
            });
          },
        });
      }

      // Callout line classes: header line + its `>` continuation lines.
      const first = doc.lineAt(view.viewport.from).number;
      const last = doc.lineAt(view.viewport.to).number;
      let calloutType: string | null = null;
      for (let n = first; n <= last; n++) {
        const line = doc.line(n);
        const head = CALLOUT_RE.exec(line.text);
        if (head) {
          calloutType = head[1].toLowerCase();
          decos.push({
            from: line.from,
            to: line.from,
            deco: Decoration.line({
              class: `gs-cm-callout gs-cm-callout--head gs-cm-callout--${calloutType}`,
            }),
          });
        } else if (/^\s*>/.test(line.text)) {
          decos.push({
            from: line.from,
            to: line.from,
            deco: Decoration.line({
              class: calloutType
                ? `gs-cm-callout gs-cm-callout--${calloutType}`
                : "gs-cm-quote",
            }),
          });
        } else {
          calloutType = null;
        }
      }

      decos.sort((a, b) => a.from - b.from || a.to - b.to);
      return Decoration.set(
        decos.map((d) => d.deco.range(d.from, d.to)),
        true,
      );
    }
  },
  { decorations: (v) => v.decorations },
);

/** Markdown token styling — pulls only from the grain-space.css tokens. */
const mdHighlight = HighlightStyle.define([
  { tag: tags.heading1, fontSize: "1.7em", fontWeight: "700" },
  { tag: tags.heading2, fontSize: "1.42em", fontWeight: "680" },
  { tag: tags.heading3, fontSize: "1.2em", fontWeight: "650" },
  { tag: tags.heading4, fontSize: "1.08em", fontWeight: "650" },
  { tag: tags.heading5, fontWeight: "650" },
  { tag: tags.heading6, fontWeight: "650", color: "var(--muted)" },
  { tag: tags.strong, fontWeight: "700" },
  { tag: tags.emphasis, fontStyle: "italic" },
  {
    tag: tags.strikethrough,
    textDecoration: "line-through",
    color: "var(--muted)",
  },
  {
    tag: tags.monospace,
    fontFamily: "var(--mono)",
    fontSize: "0.9em",
    background: "var(--mono-inline-bg)",
    borderRadius: "4px",
    padding: "0.5px 3px",
  },
  { tag: tags.link, color: "var(--orange)", textDecoration: "underline" },
  { tag: tags.url, color: "var(--faint)" },
  { tag: tags.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: tags.list, color: "var(--ink)" },
  { tag: tags.meta, color: "var(--faint)" },
  { tag: tags.processingInstruction, color: "var(--faint)" },
  { tag: tags.contentSeparator, color: "var(--faint)" },
  // Obsidian-only constructs.
  {
    tag: grainTags.highlight,
    background: "var(--hl-mark)",
    borderRadius: "3px",
    padding: "0.5px 2px",
  },
  { tag: grainTags.wikilink, color: "var(--orange)", fontWeight: "560" },
  { tag: grainTags.hashtag, color: "var(--blue)", fontWeight: "560" },
  { tag: grainTags.comment, color: "var(--ghost)", fontStyle: "italic" },
  {
    tag: grainTags.math,
    fontFamily: "var(--mono)",
    fontSize: "0.92em",
    color: "var(--amber)",
  },
  { tag: grainTags.footnote, color: "var(--blue)", fontSize: "0.85em" },
]);

const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    // Body copy sized to match desktop note editors (Obsidian/Bear ≈ 16px);
    // hierarchy still reads through weight + the relative heading scale below.
    fontSize: "16px",
    backgroundColor: "transparent",
    color: "var(--ink)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-content": {
    fontFamily: "var(--body)",
    // Roomier leading + a readable measure (~64 chars) so long lines wrap for
    // comfort instead of spanning the whole sheet.
    lineHeight: "1.75",
    letterSpacing: "0.001em",
    padding: "6px 0 48px",
    maxWidth: "42rem",
    caretColor: "var(--orange)",
  },
  ".cm-line": { padding: "1px 0" },
  ".cm-cursor": { borderLeftColor: "var(--orange)" },
  ".cm-placeholder": { color: "var(--faint)" },
});

/** Ctrl/Cmd formatting shortcuts, mirroring Obsidian's defaults. */
const formatKeymap = keymap.of([
  { key: "Mod-b", run: (v) => (toggleWrap(v, "**"), true) },
  { key: "Mod-i", run: (v) => (toggleWrap(v, "*"), true) },
  { key: "Mod-e", run: (v) => (toggleWrap(v, "`"), true) },
  { key: "Mod-Shift-h", run: (v) => (toggleWrap(v, "=="), true) },
  { key: "Mod-Shift-m", run: (v) => (toggleWrap(v, "~~"), true) },
  { key: "Mod-k", run: (v) => (insertLink(v), true) },
  { key: "Mod-Shift-7", run: (v) => (toggleLinePrefix(v, "", true), true) },
  { key: "Mod-Shift-8", run: (v) => (toggleLinePrefix(v, "- "), true) },
  { key: "Mod-Shift-9", run: (v) => (toggleLinePrefix(v, "- [ ] "), true) },
  { key: "Mod-Shift-.", run: (v) => (toggleLinePrefix(v, "> "), true) },
]);

type Props = {
  /** Remounts the document when the editing session changes. */
  docKey: number;
  value: string;
  readOnly: boolean;
  placeholder: string;
  onChange: (text: string) => void;
  onBlur: () => void;
};

function MarkdownEditor(
  { docKey, value, readOnly, placeholder, onChange, onBlur }: Props,
  ref: React.Ref<EditorHandle>,
) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onBlurRef = useRef(onBlur);
  const valueRef = useRef(value);
  onChangeRef.current = onChange;
  onBlurRef.current = onBlur;
  valueRef.current = value;

  // Toolbar-facing command surface. Each call is a no-op until the view exists.
  useImperativeHandle(ref, (): EditorHandle => {
    const v = () => viewRef.current;
    return {
      bold: () => v() && toggleWrap(v()!, "**"),
      italic: () => v() && toggleWrap(v()!, "*"),
      strikethrough: () => v() && toggleWrap(v()!, "~~"),
      highlight: () => v() && toggleWrap(v()!, "=="),
      code: () => v() && toggleWrap(v()!, "`"),
      heading: (level) => v() && setHeading(v()!, level),
      bullet: () => v() && toggleLinePrefix(v()!, "- "),
      ordered: () => v() && toggleLinePrefix(v()!, "", true),
      task: () => v() && toggleLinePrefix(v()!, "- [ ] "),
      quote: () => v() && toggleLinePrefix(v()!, "> "),
      codeBlock: () => v() && insertBlock(v()!, "```\n\n```"),
      table: () => v() && insertBlock(v()!, TABLE_TEMPLATE),
      link: () => v() && insertLink(v()!),
      rule: () => v() && insertBlock(v()!, "---"),
    };
  }, []);

  // (Re)create the editor per note. The full teardown (destroy) releases the
  // previous document's state — nothing accumulates across note switches.
  useEffect(() => {
    if (!hostRef.current) return;
    const state = EditorState.create({
      doc: valueRef.current,
      extensions: [
        history(),
        formatKeymap,
        keymap.of([...defaultKeymap, ...historyKeymap]),
        markdown({
          base: markdownLanguage,
          extensions: grainMarkdownExtensions,
        }),
        syntaxHighlighting(mdHighlight),
        richDecorations,
        editorTheme,
        EditorView.lineWrapping,
        cmPlaceholder(placeholder),
        EditorState.readOnly.of(readOnly),
        EditorView.editable.of(!readOnly),
        EditorView.updateListener.of((update) => {
          if (!update.docChanged) return;
          if (update.transactions.some((tr) => tr.annotation(External))) return;
          onChangeRef.current(update.state.doc.toString());
        }),
        EditorView.domEventHandlers({
          blur: () => {
            onBlurRef.current();
            return false;
          },
        }),
      ],
    });
    const view = new EditorView({ state, parent: hostRef.current });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // placeholder changes only alongside the editing session (deliberately
    // omitted from the deps — a text-only prop change must not reset the doc).
  }, [docKey, readOnly]);

  // External refreshes (e.g. a quick-add elsewhere touched this note): adopt
  // the new text only while the user isn't typing here.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || view.hasFocus) return;
    const current = view.state.doc.toString();
    if (current === value) return;
    view.dispatch({
      changes: { from: 0, to: current.length, insert: value },
      annotations: External.of(true),
    });
  }, [value]);

  return <div ref={hostRef} className="gs-edit" />;
}

export default forwardRef(MarkdownEditor);
