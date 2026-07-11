import { EditorSelection, type ChangeSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/**
 * [GRAIN] Editing primitives shared by the markdown editor's keymap and the
 * formatting toolbar. Everything here is a pure operation on an EditorView —
 * no React, no extra state — so it stays zero-overhead per the workspace's
 * low-RAM contract.
 */

/** Toggle an inline marker (`**`, `*`, `==`, …) around each selection range. */
export function toggleWrap(view: EditorView, mark: string, close = mark): void {
  const { state } = view;
  const tx = state.changeByRange((range) => {
    const before = state.sliceDoc(range.from - mark.length, range.from);
    const after = state.sliceDoc(range.to, range.to + close.length);
    // Already wrapped → peel the markers off.
    if (before === mark && after === close && range.from !== range.to) {
      return {
        changes: [
          { from: range.from - mark.length, to: range.from },
          { from: range.to, to: range.to + close.length },
        ],
        range: EditorSelection.range(
          range.from - mark.length,
          range.to - mark.length,
        ),
      };
    }
    const inner = state.sliceDoc(range.from, range.to);
    return {
      changes: {
        from: range.from,
        to: range.to,
        insert: mark + inner + close,
      },
      range: EditorSelection.range(
        range.from + mark.length,
        range.to + mark.length,
      ),
    };
  });
  view.dispatch(tx, { scrollIntoView: true, userEvent: "input.format" });
  view.focus();
}

/** Set/clear an ATX heading level on every line the selection touches. */
export function setHeading(view: EditorView, level: number): void {
  const { state } = view;
  const changes: ChangeSpec[] = [];
  const seen = new Set<number>();
  for (const range of state.selection.ranges) {
    let lineNo = state.doc.lineAt(range.from).number;
    const last = state.doc.lineAt(range.to).number;
    for (; lineNo <= last; lineNo++) {
      if (seen.has(lineNo)) continue;
      seen.add(lineNo);
      const line = state.doc.line(lineNo);
      const existing = /^(#{1,6}\s+)/.exec(line.text);
      const prefix = "#".repeat(level) + " ";
      const already = existing?.[1] === prefix;
      changes.push({
        from: line.from,
        to: line.from + (existing ? existing[1].length : 0),
        insert: already ? "" : prefix, // same level toggles the heading off
      });
    }
  }
  view.dispatch(state.update({ changes, userEvent: "input.format" }));
  view.focus();
}

/**
 * Toggle a line prefix (`- `, `> `, `- [ ] `, …) on each selected line.
 * `ordered` renumbers as `1. 2. 3.` instead of a fixed prefix.
 */
export function toggleLinePrefix(
  view: EditorView,
  prefix: string,
  ordered = false,
): void {
  const { state } = view;
  const changes: ChangeSpec[] = [];
  const seen = new Set<number>();
  const bulletLike = /^(\s*)([-*+]\s+|\d+\.\s+|>\s+|-\s\[[ xX]\]\s+)/;
  let n = 1;
  for (const range of state.selection.ranges) {
    let lineNo = state.doc.lineAt(range.from).number;
    const last = state.doc.lineAt(range.to).number;
    for (; lineNo <= last; lineNo++) {
      if (seen.has(lineNo)) continue;
      seen.add(lineNo);
      const line = state.doc.line(lineNo);
      const want = ordered ? `${n++}. ` : prefix;
      const has = line.text.startsWith(want);
      const existing = bulletLike.exec(line.text);
      if (has) {
        changes.push({
          from: line.from,
          to: line.from + want.length,
          insert: "",
        });
      } else {
        changes.push({
          from: line.from,
          to: line.from + (existing ? existing[0].length : 0),
          insert: want,
        });
      }
    }
  }
  view.dispatch(state.update({ changes, userEvent: "input.format" }));
  view.focus();
}

/** Insert `text` as its own block, padded with blank lines, at the cursor. */
export function insertBlock(view: EditorView, text: string): void {
  const { state } = view;
  const range = state.selection.main;
  const line = state.doc.lineAt(range.from);
  const atLineStart = range.from === line.from;
  const lead = atLineStart ? "" : "\n";
  const insert = `${lead}${text}\n`;
  const anchor = range.from + insert.length;
  view.dispatch(
    state.update({
      changes: { from: range.from, insert },
      selection: EditorSelection.cursor(anchor),
      scrollIntoView: true,
      userEvent: "input.format",
    }),
  );
  view.focus();
}

/** Insert `[label](url)` (or wrap the selection as the label) and select `url`. */
export function insertLink(view: EditorView): void {
  const { state } = view;
  const tx = state.changeByRange((range) => {
    const label = state.sliceDoc(range.from, range.to);
    const insert = `[${label}](url)`;
    const urlFrom = range.from + label.length + 3;
    return {
      changes: { from: range.from, to: range.to, insert },
      range: EditorSelection.range(urlFrom, urlFrom + 3),
    };
  });
  view.dispatch(tx, { scrollIntoView: true, userEvent: "input.format" });
  view.focus();
}

/** A ready-to-fill 3-column GFM table. */
export const TABLE_TEMPLATE = [
  "| Column A | Column B | Column C |",
  "| --- | --- | --- |",
  "| Cell | Cell | Cell |",
].join("\n");
