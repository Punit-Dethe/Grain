import {
  type Extension,
  type Range,
  StateField,
  type EditorState,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";

/**
 * [GRAIN] Live-preview GFM table rendering for the Grain Space editor.
 *
 * CodeMirror styles markdown in place, but tables have no built-in rendered
 * form — the `| a | b |` source rows just sit there as text. This field swaps
 * a table's source lines for a real <table> whenever the cursor is OUTSIDE it
 * (Obsidian's model), and steps aside the instant the cursor enters so the raw
 * pipes become editable again.
 *
 * Why a StateField and not the richDecorations ViewPlugin: replacing across
 * line breaks / adding block widgets changes vertical layout, which CodeMirror
 * forbids from plugins (they run after the viewport is measured — it throws
 * "Block decorations may not be specified via plugins"). StateFields are part
 * of editor state and computed first, so block-level decorations are legal.
 *
 * Low-overhead by construction: one field, one decoration set, rebuilt only
 * when the doc or selection changes (otherwise the set is mapped through the
 * edit). The <table> DOM is built once per distinct source (WidgetType.eq gates
 * reuse) and every cell is filled with DOM APIs only — never innerHTML — so no
 * markup in a note can be injected into the page.
 */

type Align = "left" | "center" | "right" | null;
type Cell = { text: string; pos: number };
type ParsedTable = { header: Cell[]; align: Align[]; rows: Cell[][] };

/**
 * Split one table row into cells. A `|` ends a cell only when it is a real
 * column boundary — not when escaped (`\|`), inside an inline code span, or
 * inside `[...]` brackets (so `[[Note|alias]]` links survive intact, matching
 * Obsidian's reading view).
 */
function splitCells(rowText: string, rowStart: number): Cell[] {
  const segs: { text: string; start: number }[] = [];
  let start = 0;
  let inCode = false;
  let bracket = 0;
  for (let i = 0; i <= rowText.length; i++) {
    const c = rowText[i];
    if (i < rowText.length) {
      if (c === "`") inCode = !inCode;
      else if (!inCode && c === "[") bracket++;
      else if (!inCode && c === "]" && bracket > 0) bracket--;
    }
    const boundary =
      i === rowText.length ||
      (c === "|" && rowText[i - 1] !== "\\" && !inCode && bracket === 0);
    if (boundary) {
      segs.push({ text: rowText.slice(start, i), start: rowStart + start });
      start = i + 1;
    }
  }
  // A leading/trailing `|` yields an empty edge segment — those are borders,
  // not cells. Interior empty cells (`a || c`) are preserved.
  if (segs.length && segs[0].text.trim() === "") segs.shift();
  if (segs.length && segs[segs.length - 1].text.trim() === "") segs.pop();
  return segs.map((s) => {
    const leading = s.text.length - s.text.trimStart().length;
    const text = s.text.trim().replace(/\\\|/g, "|");
    return { text, pos: text ? s.start + leading : s.start };
  });
}

/** Read a delimiter-row cell (`:--`, `--:`, `:-:`) into a column alignment. */
function readAlign(cell: string): Align {
  const t = cell.trim();
  const left = t.startsWith(":");
  const right = t.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  if (left) return "left";
  return null;
}

/** Parse the raw table source into header, per-column alignment and body rows. */
function parseTable(source: string, base: number): ParsedTable | null {
  const lines = source.split("\n");
  if (lines.length < 2) return null;
  const spans: { text: string; start: number }[] = [];
  let offset = base;
  for (const text of lines) {
    spans.push({ text, start: offset });
    offset += text.length + 1; // +1 for the "\n" split removed
  }
  const header = splitCells(spans[0].text, spans[0].start);
  if (header.length === 0) return null;
  const align = splitCells(spans[1].text, spans[1].start).map((c) =>
    readAlign(c.text),
  );
  const rows: Cell[][] = [];
  for (let i = 2; i < spans.length; i++) {
    if (!/\S/.test(spans[i].text)) continue; // skip a blank trailing line
    rows.push(splitCells(spans[i].text, spans[i].start));
  }
  return { header, align, rows };
}

/** Paired inline delimiters, longest-first so `**` wins over `*`. */
const INLINE_DELIMS: { mark: string; tag: string; cls?: string }[] = [
  { mark: "**", tag: "strong" },
  { mark: "__", tag: "strong" },
  { mark: "~~", tag: "s" },
  { mark: "==", tag: "mark", cls: "gs-cm-thl" },
  { mark: "*", tag: "em" },
  { mark: "_", tag: "em" },
];

/**
 * Render a cell's inline markdown into `parent` using DOM nodes only. Covers
 * the constructs that actually show up in tables — code, emphasis, strike,
 * highlight, links, wikilinks — and treats anything else as literal text.
 * Bounded and recursive on delimiter interiors; cells are short.
 */
function renderInline(text: string, parent: Node): void {
  let i = 0;
  let buf = "";
  const flush = () => {
    if (buf) {
      parent.appendChild(document.createTextNode(buf));
      buf = "";
    }
  };

  while (i < text.length) {
    const ch = text[i];

    // Backslash-escaped markdown punctuation → the literal character.
    if (
      ch === "\\" &&
      i + 1 < text.length &&
      /[\\`*_~=[\]()!]/.test(text[i + 1])
    ) {
      buf += text[i + 1];
      i += 2;
      continue;
    }

    // Inline code — opaque span, no nested parsing. Match a run of N backticks
    // to the next identical run.
    if (ch === "`") {
      let n = 1;
      while (text[i + n] === "`") n++;
      const fence = "`".repeat(n);
      const close = text.indexOf(fence, i + n);
      if (close !== -1) {
        flush();
        const code = document.createElement("code");
        code.className = "gs-cm-tcode";
        code.textContent = text.slice(i + n, close).trim();
        parent.appendChild(code);
        i = close + n;
        continue;
      }
    }

    // Wikilink [[target|alias]] — checked before plain links.
    if (ch === "[" && text[i + 1] === "[") {
      const close = text.indexOf("]]", i + 2);
      if (close !== -1) {
        flush();
        const inner = text.slice(i + 2, close);
        const bar = inner.indexOf("|");
        const span = document.createElement("span");
        span.className = "gs-cm-twlink";
        span.textContent = (bar >= 0 ? inner.slice(bar + 1) : inner).trim();
        parent.appendChild(span);
        i = close + 2;
        continue;
      }
    }

    // Link [text](href) — rendered non-navigable (edits on click like the rest
    // of the table); the href is surfaced via the tooltip.
    if (ch === "[") {
      const m = /^\[([^\]]*)\]\(([^)\s]*)(?:\s+"[^"]*")?\)/.exec(text.slice(i));
      if (m) {
        flush();
        const link = document.createElement("span");
        link.className = "gs-cm-tlink";
        link.title = m[2];
        renderInline(m[1], link);
        parent.appendChild(link);
        i += m[0].length;
        continue;
      }
    }

    // Paired emphasis / strike / highlight.
    let matched = false;
    for (const d of INLINE_DELIMS) {
      if (!text.startsWith(d.mark, i)) continue;
      // Skip intraword `_` (snake_case is not emphasis).
      if (d.mark === "_" && /\w/.test(text[i - 1] ?? "")) continue;
      // indexOf → -1 when unclosed; the same test also rejects an empty body.
      const close = text.indexOf(d.mark, i + d.mark.length);
      if (close < i + d.mark.length + 1) continue; // need a closer + ≥1 char inside
      flush();
      const el = document.createElement(d.tag);
      if (d.cls) el.className = d.cls;
      renderInline(text.slice(i + d.mark.length, close), el);
      parent.appendChild(el);
      i = close + d.mark.length;
      matched = true;
      break;
    }
    if (matched) continue;

    buf += ch;
    i++;
  }
  flush();
}

function applyAlign(el: HTMLElement, a: Align): void {
  if (a) el.style.textAlign = a;
}

/** A real <table> standing in for a block of markdown table source. */
class TableWidget extends WidgetType {
  constructor(
    readonly source: string,
    readonly from: number,
  ) {
    super();
  }

  eq(other: TableWidget): boolean {
    return other.source === this.source && other.from === this.from;
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "gs-cm-tablewrap";

    const parsed = parseTable(this.source, this.from);
    if (!parsed) {
      wrap.textContent = this.source;
    } else {
      const table = document.createElement("table");
      table.className = "gs-cm-table";
      const cols = parsed.header.length;

      const thead = document.createElement("thead");
      const htr = document.createElement("tr");
      parsed.header.forEach((cell, c) => {
        const th = document.createElement("th");
        applyAlign(th, parsed.align[c]);
        th.dataset.pos = String(cell.pos);
        renderInline(cell.text, th);
        htr.appendChild(th);
      });
      thead.appendChild(htr);
      table.appendChild(thead);

      const tbody = document.createElement("tbody");
      for (const row of parsed.rows) {
        const tr = document.createElement("tr");
        for (let c = 0; c < cols; c++) {
          const cell = row[c];
          const td = document.createElement("td");
          applyAlign(td, parsed.align[c]);
          td.dataset.pos = String(cell ? cell.pos : this.from);
          if (cell) renderInline(cell.text, td);
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      }
      table.appendChild(tbody);
      wrap.appendChild(table);
    }

    // Click-to-edit: drop the caret into the source of the clicked cell, which
    // brings the cursor inside the table and dissolves this widget back to raw
    // pipes for editing.
    wrap.addEventListener("mousedown", (e) => {
      const el = (e.target as HTMLElement).closest?.(
        "[data-pos]",
      ) as HTMLElement | null;
      const pos = el?.dataset.pos ? Number(el.dataset.pos) : this.from;
      e.preventDefault();
      view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
      view.focus();
    });

    return wrap;
  }

  ignoreEvent(): boolean {
    // Handle our own clicks; keep CodeMirror from coordinate-mapping into the
    // replaced range.
    return true;
  }
}

/** Rebuild the whole table decoration set for the current state. */
function buildTables(state: EditorState): DecorationSet {
  const widgets: Range<Decoration>[] = [];
  const { doc, selection } = state;

  syntaxTree(state).iterate({
    enter: (node) => {
      if (node.name !== "Table") return;
      // Cursor (or any selection range) touching the table → leave it as raw,
      // editable source.
      const editing = selection.ranges.some(
        (r) => r.from <= node.to && r.to >= node.from,
      );
      if (!editing) {
        const first = doc.lineAt(node.from);
        const last = doc.lineAt(
          Math.max(node.from, Math.min(node.to, doc.length) - 1),
        );
        if (last.to > first.from) {
          const source = doc.sliceString(first.from, last.to);
          widgets.push(
            Decoration.replace({
              widget: new TableWidget(source, first.from),
              block: true,
            }).range(first.from, last.to),
          );
        }
      }
      return false; // never descend into a table's children
    },
  });

  return Decoration.set(widgets, true);
}

/**
 * The exported extension: block-level table rendering. Add it AFTER the
 * `markdown(...)` language extension so the syntax tree field exists first.
 */
export const grainTableField: Extension = StateField.define<DecorationSet>({
  create: buildTables,
  update(deco, tr) {
    if (tr.docChanged || tr.selection) return buildTables(tr.state);
    return deco.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});
