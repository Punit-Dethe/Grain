import { Fragment, type ReactNode } from "react";

type Block =
  | { type: "paragraph"; lines: string[] }
  | { type: "heading"; depth: number; text: string }
  | { type: "unordered-list"; items: string[] }
  | { type: "ordered-list"; items: string[]; start: number }
  | { type: "quote"; lines: string[] }
  | { type: "code"; language: string; lines: string[] }
  | { type: "table"; header: string[]; alignments: TableAlignment[]; rows: string[][] }
  | { type: "rule" };

type TableAlignment = "left" | "center" | "right";

const HEADING = /^(#{1,6})\s+(.+?)\s*#*\s*$/;
const UNORDERED_ITEM = /^\s*[-*+]\s+(.+)$/;
const ORDERED_ITEM = /^\s*(\d+)\.\s+(.+)$/;
const QUOTE = /^>\s?(.*)$/;
const FENCE = /^```\s*([^\s]*)\s*$/;
const RULE = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const TABLE_DIVIDER_CELL = /^:?-{3,}:?$/;

/**
 * Stateless, safe Markdown renderer for Agent replies. It deliberately covers
 * the response shapes the local/remote models emit most often without adding a
 * document parser dependency or retaining an AST/cache after the panel closes.
 * React escapes all text; raw HTML is always displayed literally.
 */
export function AgentMarkdown({ markdown }: { markdown: string }) {
  return (
    <div className="agc-markdown">
      {parseBlocks(markdown).map((block, index) => renderBlock(block, index))}
    </div>
  );
}

function parseBlocks(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];

  for (let i = 0; i < lines.length; ) {
    const line = lines[i];
    const fence = line.match(FENCE);
    if (fence) {
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !FENCE.test(lines[i])) code.push(lines[i++]);
      if (i < lines.length) i += 1;
      blocks.push({ type: "code", language: fence[1], lines: code });
      continue;
    }
    if (!line.trim()) {
      i += 1;
      continue;
    }
    const table = readTable(lines, i);
    if (table) {
      blocks.push(table.block);
      i = table.next;
      continue;
    }
    const heading = line.match(HEADING);
    if (heading) {
      blocks.push({ type: "heading", depth: heading[1].length, text: heading[2] });
      i += 1;
      continue;
    }
    if (RULE.test(line)) {
      blocks.push({ type: "rule" });
      i += 1;
      continue;
    }
    const quote = line.match(QUOTE);
    if (quote) {
      const quoteLines: string[] = [];
      while (i < lines.length) {
        const next = lines[i].match(QUOTE);
        if (!next) break;
        quoteLines.push(next[1]);
        i += 1;
      }
      blocks.push({ type: "quote", lines: quoteLines });
      continue;
    }
    const unordered = line.match(UNORDERED_ITEM);
    if (unordered) {
      const items: string[] = [];
      while (i < lines.length) {
        const next = lines[i].match(UNORDERED_ITEM);
        if (!next) break;
        items.push(next[1]);
        i += 1;
      }
      blocks.push({ type: "unordered-list", items });
      continue;
    }
    const ordered = line.match(ORDERED_ITEM);
    if (ordered) {
      const items: string[] = [];
      const start = Number(ordered[1]);
      while (i < lines.length) {
        const next = lines[i].match(ORDERED_ITEM);
        if (!next) break;
        items.push(next[2]);
        i += 1;
      }
      blocks.push({ type: "ordered-list", items, start });
      continue;
    }

    const paragraph: string[] = [];
    while (i < lines.length && lines[i].trim()) {
      if (paragraph.length && isBlockStart(lines[i])) break;
      paragraph.push(lines[i]);
      i += 1;
    }
    blocks.push({ type: "paragraph", lines: paragraph });
  }

  return blocks;
}

function isBlockStart(line: string) {
  return HEADING.test(line) || RULE.test(line) || QUOTE.test(line) || UNORDERED_ITEM.test(line) || ORDERED_ITEM.test(line) || FENCE.test(line);
}

/** Read the GFM `header | header` + `--- | ---` table form. */
function readTable(lines: string[], start: number): { block: Block; next: number } | null {
  const header = splitTableRow(lines[start]);
  const divider = lines[start + 1] ? splitTableRow(lines[start + 1]) : [];
  if (header.length < 2 || divider.length !== header.length || !divider.every((cell) => TABLE_DIVIDER_CELL.test(cell))) {
    return null;
  }

  const alignments = divider.map((cell) => {
    const left = cell.startsWith(":");
    const right = cell.endsWith(":");
    return left && right ? "center" : right ? "right" : "left";
  });
  const rows: string[][] = [];
  let next = start + 2;
  while (next < lines.length && lines[next].trim() && lines[next].includes("|")) {
    const cells = splitTableRow(lines[next]);
    if (!cells.length) break;
    rows.push(normalizeTableRow(cells, header.length));
    next += 1;
  }
  return { block: { type: "table", header, alignments, rows }, next };
}

/** Split table cells without treating an escaped `\|` as a column boundary. */
function splitTableRow(line: string): string[] {
  const source = line.trim();
  if (!source.includes("|")) return [];
  const cells: string[] = [];
  let cell = "";
  for (let i = 0; i < source.length; i += 1) {
    if (source[i] === "\\" && source[i + 1] === "|") {
      cell += "|";
      i += 1;
    } else if (source[i] === "|") {
      cells.push(cell.trim());
      cell = "";
    } else {
      cell += source[i];
    }
  }
  cells.push(cell.trim());
  if (source.startsWith("|")) cells.shift();
  if (source.endsWith("|")) cells.pop();
  return cells;
}

function normalizeTableRow(cells: string[], columns: number): string[] {
  return Array.from({ length: columns }, (_, index) => cells[index] ?? "");
}

function renderBlock(block: Block, key: number): ReactNode {
  switch (block.type) {
    case "heading": {
      const Tag = `h${block.depth}` as keyof JSX.IntrinsicElements;
      return <Tag key={key}>{inline(block.text)}</Tag>;
    }
    case "unordered-list":
      return <ul key={key}>{block.items.map((item, i) => <li key={i}>{inline(item)}</li>)}</ul>;
    case "ordered-list":
      return <ol key={key} start={block.start}>{block.items.map((item, i) => <li key={i}>{inline(item)}</li>)}</ol>;
    case "quote":
      return <blockquote key={key}>{withBreaks(block.lines)}</blockquote>;
    case "code":
      return <pre key={key} data-language={block.language || undefined}><code>{block.lines.join("\n")}</code></pre>;
    case "table":
      return (
        <div key={key} className="agc-md-table-wrap">
          <table>
            <thead>
              <tr>{block.header.map((cell, index) => <th key={index} className={`agc-md-align-${block.alignments[index]}`}>{inline(cell)}</th>)}</tr>
            </thead>
            <tbody>
              {block.rows.map((row, rowIndex) => (
                <tr key={rowIndex}>{row.map((cell, index) => <td key={index} className={`agc-md-align-${block.alignments[index]}`}>{inline(cell)}</td>)}</tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "rule":
      return <hr key={key} />;
    case "paragraph":
      return <p key={key}>{withBreaks(block.lines)}</p>;
  }
}

function withBreaks(lines: string[]): ReactNode[] {
  return lines.flatMap((line, index) => [
    ...(index ? [<br key={`break-${index}`} />] : []),
    <Fragment key={`line-${index}`}>{inline(line)}</Fragment>,
  ]);
}

function inline(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const token = /(\[([^\]]+)]\((https?:\/\/[^\s)]+)\)|`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|~~([^~]+)~~|\*([^*\n]+)\*|_([^_\n]+)_)/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = token.exec(text))) {
    if (match.index > cursor) parts.push(text.slice(cursor, match.index));
    if (match[2] && match[3]) {
      parts.push(<a key={match.index} href={match[3]} target="_blank" rel="noreferrer">{inline(match[2])}</a>);
    } else if (match[4]) {
      parts.push(<code key={match.index}>{match[4]}</code>);
    } else if (match[5] || match[6]) {
      parts.push(<strong key={match.index}>{inline(match[5] ?? match[6])}</strong>);
    } else if (match[7]) {
      parts.push(<del key={match.index}>{inline(match[7])}</del>);
    } else {
      parts.push(<em key={match.index}>{inline(match[8] ?? match[9] ?? "")}</em>);
    }
    cursor = token.lastIndex;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}
