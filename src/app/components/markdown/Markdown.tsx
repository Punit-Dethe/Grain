import { Fragment, type ReactNode } from "react";
import { ArrowUpRight } from "lucide-react";
import { commands } from "@/bindings";

type Block =
  | { type: "paragraph"; lines: string[] }
  | { type: "heading"; depth: number; text: string }
  | { type: "unordered-list"; items: string[][] }
  | { type: "ordered-list"; items: string[][]; start: number }
  | { type: "quote"; lines: string[] }
  | { type: "code"; language: string; lines: string[] }
  | {
      type: "table";
      header: string[];
      alignments: TableAlignment[];
      rows: string[][];
    }
  | { type: "rule" };

type TableAlignment = "left" | "center" | "right";

/** How a block renders. `onLink` (the Agent panel) routes every link through
 * the host instead of letting the webview navigate, and turns a line holding
 * nothing but links into buttons. */
interface Render {
  soft: boolean;
  onLink?: (url: string) => void;
}

interface LinkButton {
  title: string;
  url: string;
}

const HEADING = /^(#{1,6})\s+(.+?)\s*#*\s*$/;
const UNORDERED_ITEM = /^\s*[-*+]\s+(.+)$/;
const ORDERED_ITEM = /^\s*(\d+)\.\s+(.+)$/;
const QUOTE = /^>\s?(.*)$/;
const FENCE = /^```\s*([^\s]*)\s*$/;
const RULE = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const TABLE_DIVIDER_CELL = /^:?-{3,}:?$/;
/** A line that continues the list item above it: indented, and not itself the
 * start of something else. */
const LIST_CONTINUATION = /^\s{1,}\S/;
/** CommonMark's explicit line break inside a paragraph: two trailing spaces, or
 * a trailing backslash. Everything else that looks like a newline is a SOFT
 * break — the author's wrapping, not their intent. */
const HARD_BREAK = /( {2,}|\\)$/;
const LINK = /\[([^\]]+)]\((https?:\/\/[^\s)]+)\)/g;
/** A line that is nothing but one or more links. */
const LINK_ONLY = /^\s*(?:\[[^\]]+]\(https?:\/\/[^\s)]+\)\s*)+$/;

/**
 * [GRAIN] The app's ONE stateless, safe Markdown renderer. Written for Agent
 * replies, shared by the Agent panel and the other surfaces that render
 * authored Markdown.
 *
 * It deliberately covers the shapes real documents use — GFM tables with
 * alignment, fenced code, nested emphasis, strikethrough — without adding a
 * document-parser dependency or retaining an AST/cache after the surface closes.
 * React escapes all text; raw HTML is always displayed literally.
 *
 * `className` picks the stylesheet: `agc-markdown` for the Agent's dark panel,
 * `gm-markdown` for the light settings surfaces. The markup is identical.
 */
export function Markdown({
  markdown,
  className = "gm-markdown",
  softBreaks = false,
  onLink,
}: {
  markdown: string;
  className?: string;
  /** Open links through this instead of the webview (see `Render`). */
  onLink?: (url: string) => void;
  /** Treat a lone newline inside a paragraph as a SPACE (CommonMark), not a
   * line break. Authored documents are hard-wrapped by their author — a README
   * wrapped at 80 columns rendered with every one of those breaks preserved,
   * which looks like a narrow column of text with the right half of the page
   * left empty. Chat replies are the opposite case: a model emits single
   * newlines meaning them, so the Agent keeps break-per-newline. */
  softBreaks?: boolean;
}) {
  return (
    <div className={className}>
      {parseBlocks(markdown).map((block, index) =>
        renderBlock(block, index, { soft: softBreaks, onLink }),
      )}
    </div>
  );
}

/** Links in a reply are model output: the host checks and opens them in the
 * default browser; the panel's webview never navigates. */
function openAgentLink(url: string) {
  void commands
    .agentOpenLink(url)
    .then((result) => {
      if (result.status === "error") console.warn(result.error);
    })
    .catch(console.error);
}

/** The Agent panel's dark-surface binding. Keeps break-per-newline: a reply is
 * written as it is meant to be read, not wrapped to a column. A line holding
 * only links renders as buttons. */
export function AgentMarkdown({ markdown }: { markdown: string }) {
  return (
    <Markdown
      markdown={markdown}
      className="agc-markdown"
      onLink={openAgentLink}
    />
  );
}

/** The links of `lines` when every line is nothing but links, else null. */
function linkButtons(lines: string[]): LinkButton[] | null {
  if (!lines.length || !lines.every((line) => LINK_ONLY.test(line)))
    return null;
  return lines.flatMap((line) =>
    [...line.matchAll(LINK)].map((match) => ({
      title: match[1].trim(),
      url: match[2],
    })),
  );
}

/** Where a link goes, shown beside its title so a misleading title can't
 * hide the destination. */
function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

function renderLinkButtons(
  links: LinkButton[],
  key: number,
  onLink: (url: string) => void,
): ReactNode {
  return (
    <div key={key} className="agc-md-links">
      {links.map((link, index) => (
        <button
          key={index}
          type="button"
          className="agc-md-link"
          title={link.url}
          onClick={() => onLink(link.url)}
        >
          <span className="agc-md-link-title">
            {inline(link.title, { soft: false })}
          </span>
          <span className="agc-md-link-host">{hostOf(link.url)}</span>
          <ArrowUpRight size={13} strokeWidth={2} aria-hidden />
        </button>
      ))}
    </div>
  );
}

function parseBlocks(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];

  for (let i = 0; i < lines.length;) {
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
      blocks.push({
        type: "heading",
        depth: heading[1].length,
        text: heading[2],
      });
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
      const items: string[][] = [];
      while (i < lines.length) {
        const next = lines[i].match(UNORDERED_ITEM);
        if (!next) break;
        i += 1;
        items.push([
          next[1],
          ...readContinuation(
            lines,
            () => i,
            (n) => (i = n),
          ),
        ]);
      }
      blocks.push({ type: "unordered-list", items });
      continue;
    }
    const ordered = line.match(ORDERED_ITEM);
    if (ordered) {
      const items: string[][] = [];
      const start = Number(ordered[1]);
      while (i < lines.length) {
        const next = lines[i].match(ORDERED_ITEM);
        if (!next) break;
        i += 1;
        items.push([
          next[2],
          ...readContinuation(
            lines,
            () => i,
            (n) => (i = n),
          ),
        ]);
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

/** Lines belonging to the list item just read: indented, non-blank, and not the
 * start of a new item or block. Without this a hard-wrapped item spilled out of
 * the list and rendered as a stray paragraph beneath it. */
function readContinuation(
  lines: string[],
  get: () => number,
  set: (next: number) => void,
): string[] {
  const extra: string[] = [];
  let i = get();
  while (
    i < lines.length &&
    lines[i].trim() &&
    LIST_CONTINUATION.test(lines[i]) &&
    !isBlockStart(lines[i].trim())
  ) {
    extra.push(lines[i].trim());
    i += 1;
  }
  set(i);
  return extra;
}

function isBlockStart(line: string) {
  return (
    HEADING.test(line) ||
    RULE.test(line) ||
    QUOTE.test(line) ||
    UNORDERED_ITEM.test(line) ||
    ORDERED_ITEM.test(line) ||
    FENCE.test(line)
  );
}

/** Read the GFM `header | header` + `--- | ---` table form. */
function readTable(
  lines: string[],
  start: number,
): { block: Block; next: number } | null {
  const header = splitTableRow(lines[start]);
  const divider = lines[start + 1] ? splitTableRow(lines[start + 1]) : [];
  if (
    header.length < 2 ||
    divider.length !== header.length ||
    !divider.every((cell) => TABLE_DIVIDER_CELL.test(cell))
  ) {
    return null;
  }

  const alignments = divider.map((cell) => {
    const left = cell.startsWith(":");
    const right = cell.endsWith(":");
    return left && right ? "center" : right ? "right" : "left";
  });
  const rows: string[][] = [];
  let next = start + 2;
  while (
    next < lines.length &&
    lines[next].trim() &&
    lines[next].includes("|")
  ) {
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

function renderBlock(block: Block, key: number, render: Render): ReactNode {
  switch (block.type) {
    case "heading": {
      const Tag = `h${block.depth}` as keyof JSX.IntrinsicElements;
      return <Tag key={key}>{inline(block.text, render)}</Tag>;
    }
    case "unordered-list": {
      // A list of nothing but links is a row of buttons.
      const links = render.onLink
        ? linkButtons(block.items.map((item) => item.join(" ")))
        : null;
      if (links && render.onLink)
        return renderLinkButtons(links, key, render.onLink);
      return (
        <ul key={key}>
          {block.items.map((item, i) => (
            <li key={i}>{joinLines(item, render)}</li>
          ))}
        </ul>
      );
    }
    case "ordered-list":
      return (
        <ol key={key} start={block.start}>
          {block.items.map((item, i) => (
            <li key={i}>{joinLines(item, render)}</li>
          ))}
        </ol>
      );
    case "quote":
      return (
        <blockquote key={key}>{joinLines(block.lines, render)}</blockquote>
      );
    case "code":
      return (
        <pre key={key} data-language={block.language || undefined}>
          <code>{block.lines.join("\n")}</code>
        </pre>
      );
    case "table":
      return (
        <div key={key} className="agc-md-table-wrap">
          <table>
            <thead>
              <tr>
                {block.header.map((cell, index) => (
                  <th
                    key={index}
                    className={`agc-md-align-${block.alignments[index]}`}
                  >
                    {inline(cell, render)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, rowIndex) => (
                <tr key={rowIndex}>
                  {row.map((cell, index) => (
                    <td
                      key={index}
                      className={`agc-md-align-${block.alignments[index]}`}
                    >
                      {inline(cell, render)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "rule":
      return <hr key={key} />;
    case "paragraph": {
      // A paragraph of nothing but links is a row of buttons.
      const links = render.onLink ? linkButtons(block.lines) : null;
      if (links && render.onLink)
        return renderLinkButtons(links, key, render.onLink);
      return <p key={key}>{joinLines(block.lines, render)}</p>;
    }
  }
}

/** Lines of one paragraph-like block, joined per the chosen break rule: a
 * newline becomes a space when `soft`, otherwise a `<br>`. An explicit
 * CommonMark break (two trailing spaces, or a backslash) is always honoured. */
function joinLines(lines: string[], render: Render): ReactNode[] {
  if (!render.soft) {
    return lines.flatMap((line, index) => [
      ...(index ? [<br key={`break-${index}`} />] : []),
      <Fragment key={`line-${index}`}>{inline(line, render)}</Fragment>,
    ]);
  }
  const out: ReactNode[] = [];
  let run: string[] = [];
  const flush = (key: number) => {
    if (run.length)
      out.push(
        <Fragment key={`run-${key}`}>{inline(run.join(" "), render)}</Fragment>,
      );
    run = [];
  };
  lines.forEach((line, index) => {
    if (HARD_BREAK.test(line)) {
      run.push(line.replace(HARD_BREAK, ""));
      flush(index);
      if (index < lines.length - 1) out.push(<br key={`break-${index}`} />);
    } else {
      run.push(line.trim());
    }
  });
  flush(lines.length);
  return out;
}

function inline(text: string, render: Render): ReactNode[] {
  const parts: ReactNode[] = [];
  const token =
    /(\[([^\]]+)]\((https?:\/\/[^\s)]+)\)|`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|~~([^~]+)~~|\*([^*\n]+)\*|_([^_\n]+)_)/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = token.exec(text))) {
    if (match.index > cursor) parts.push(text.slice(cursor, match.index));
    if (match[2] && match[3]) {
      const url = match[3];
      const { onLink } = render;
      parts.push(
        onLink ? (
          <a
            key={match.index}
            href={url}
            title={url}
            onClick={(event) => {
              event.preventDefault();
              onLink(url);
            }}
          >
            {inline(match[2], render)}
          </a>
        ) : (
          <a key={match.index} href={url} target="_blank" rel="noreferrer">
            {inline(match[2], render)}
          </a>
        ),
      );
    } else if (match[4]) {
      parts.push(<code key={match.index}>{match[4]}</code>);
    } else if (match[5] || match[6]) {
      parts.push(
        <strong key={match.index}>
          {inline(match[5] ?? match[6], render)}
        </strong>,
      );
    } else if (match[7]) {
      parts.push(<del key={match.index}>{inline(match[7], render)}</del>);
    } else {
      parts.push(
        <em key={match.index}>{inline(match[8] ?? match[9] ?? "", render)}</em>,
      );
    }
    cursor = token.lastIndex;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}
