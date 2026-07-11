import { useEffect, useRef } from "react";
import { Annotation, EditorState } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder as cmPlaceholder,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import {
  HighlightStyle,
  syntaxHighlighting,
  syntaxTree,
} from "@codemirror/language";
import { tags } from "@lezer/highlight";

/**
 * [GRAIN] Obsidian-style live markdown editor for the Grain Space workspace.
 * CodeMirror 6 (the same engine Obsidian is built on): markdown is styled in
 * place (headings grow, **bold** is bold, `code` is mono) and the syntax
 * markers themselves are hidden on every line EXCEPT the one the cursor is on,
 * where they reappear for editing — the "live preview" feel.
 *
 * Default export so `React.lazy` can code-split it: this chunk (and its JS
 * heap) is never loaded until the first time a note is actually opened.
 */

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
]);

/** Conceal markdown syntax markers on lines the cursor is not on. */
const livePreview = ViewPlugin.fromClass(
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
      const marks: { from: number; to: number }[] = [];
      for (const { from, to } of view.visibleRanges) {
        syntaxTree(view.state).iterate({
          from,
          to,
          enter: (node) => {
            if (!HIDDEN_MARKS.has(node.name)) return;
            const startLine = doc.lineAt(node.from).number;
            const endLine = doc.lineAt(node.to).number;
            for (let l = startLine; l <= endLine; l++) {
              if (active.has(l)) return;
            }
            let end = node.to;
            // A heading's `#` marks swallow their following space too.
            if (
              node.name === "HeaderMark" &&
              doc.sliceString(end, end + 1) === " "
            ) {
              end += 1;
            }
            marks.push({ from: node.from, to: end });
          },
        });
      }
      marks.sort((a, b) => a.from - b.from || a.to - b.to);
      return Decoration.set(
        marks.map((m) => Decoration.replace({}).range(m.from, m.to)),
      );
    }
  },
  { decorations: (v) => v.decorations },
);

/** Markdown token styling — pulls only from the grain-space.css tokens. */
const mdHighlight = HighlightStyle.define([
  { tag: tags.heading1, fontSize: "1.55em", fontWeight: "700" },
  { tag: tags.heading2, fontSize: "1.32em", fontWeight: "680" },
  { tag: tags.heading3, fontSize: "1.16em", fontWeight: "650" },
  { tag: tags.heading4, fontSize: "1.05em", fontWeight: "650" },
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
    background: "rgba(23, 20, 18, 0.06)",
    borderRadius: "4px",
  },
  { tag: tags.link, color: "var(--orange)", textDecoration: "underline" },
  { tag: tags.url, color: "var(--faint)" },
  { tag: tags.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: tags.list, color: "var(--ink)" },
  { tag: tags.meta, color: "var(--faint)" },
  { tag: tags.processingInstruction, color: "var(--faint)" },
  { tag: tags.contentSeparator, color: "var(--faint)" },
]);

const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "13.5px",
    backgroundColor: "transparent",
    color: "var(--ink)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-content": {
    fontFamily: "var(--body)",
    lineHeight: "1.7",
    padding: "4px 0 28px",
    caretColor: "var(--orange)",
  },
  ".cm-line": { padding: "0" },
  ".cm-cursor": { borderLeftColor: "var(--orange)" },
  ".cm-placeholder": { color: "var(--faint)" },
});

type Props = {
  /** Remounts the document when the editing session changes. */
  docKey: number;
  value: string;
  readOnly: boolean;
  placeholder: string;
  onChange: (text: string) => void;
  onBlur: () => void;
};

export default function MarkdownEditor({
  docKey,
  value,
  readOnly,
  placeholder,
  onChange,
  onBlur,
}: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onBlurRef = useRef(onBlur);
  const valueRef = useRef(value);
  onChangeRef.current = onChange;
  onBlurRef.current = onBlur;
  valueRef.current = value;

  // (Re)create the editor per note. The full teardown (destroy) releases the
  // previous document's state — nothing accumulates across note switches.
  useEffect(() => {
    if (!hostRef.current) return;
    const state = EditorState.create({
      doc: valueRef.current,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        markdown({ base: markdownLanguage }),
        syntaxHighlighting(mdHighlight),
        livePreview,
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
