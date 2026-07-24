import React from "react";

/** [GRAIN] A tiny, dependency-free markdown renderer for extension READMEs.
 *
 * It emits React elements (never `dangerouslySetInnerHTML`), so untrusted
 * README text can never inject markup — the worst an author can do is style
 * their own words. It covers the constructs a README actually uses: headings,
 * paragraphs, bold/italic/inline-code, links, fenced code, lists, blockquotes,
 * tables-as-text, and horizontal rules. Anything fancier degrades to plain text.
 */

type Inline = React.ReactNode;

/** Parse inline spans: `code`, **bold**, *italic*, [text](url). Links are
 * restricted to http/https/mailto so a README can't smuggle a javascript: URL. */
function inline(text: string, keyBase: string): Inline[] {
  const out: Inline[] = [];
  let i = 0;
  let buf = "";
  let k = 0;
  const flush = () => {
    if (buf) {
      out.push(buf);
      buf = "";
    }
  };
  const push = (node: Inline) => {
    flush();
    out.push(<React.Fragment key={`${keyBase}-${k++}`}>{node}</React.Fragment>);
  };
  while (i < text.length) {
    const c = text[i];
    // inline code
    if (c === "`") {
      const end = text.indexOf("`", i + 1);
      if (end > i) {
        push(
          <code className="px-1 py-0.5 rounded bg-paper-sunken border border-line text-[0.85em] font-mono">
            {text.slice(i + 1, end)}
          </code>,
        );
        i = end + 1;
        continue;
      }
    }
    // bold
    if (c === "*" && text[i + 1] === "*") {
      const end = text.indexOf("**", i + 2);
      if (end > i) {
        push(<strong className="font-semibold text-ink">{text.slice(i + 2, end)}</strong>);
        i = end + 2;
        continue;
      }
    }
    // italic (single * or _), not mid-word underscore
    if ((c === "*" || c === "_") && text[i + 1] !== c) {
      const end = text.indexOf(c, i + 1);
      if (end > i && text[end - 1] !== " ") {
        push(<em className="italic">{text.slice(i + 1, end)}</em>);
        i = end + 1;
        continue;
      }
    }
    // link [text](url)
    if (c === "[") {
      const close = text.indexOf("]", i);
      if (close > i && text[close + 1] === "(") {
        const paren = text.indexOf(")", close + 2);
        if (paren > close) {
          const label = text.slice(i + 1, close);
          const url = text.slice(close + 2, paren).trim();
          if (/^(https?:|mailto:)/i.test(url)) {
            push(
              <a
                href={url}
                target="_blank"
                rel="noreferrer"
                className="text-accent hover:underline"
              >
                {label}
              </a>,
            );
            i = paren + 1;
            continue;
          }
        }
      }
    }
    buf += c;
    i += 1;
  }
  flush();
  return out;
}

export const Markdown: React.FC<{ text: string }> = ({ text }) => {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const blocks: React.ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    // fenced code
    if (line.trimStart().startsWith("```")) {
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        body.push(lines[i]);
        i += 1;
      }
      i += 1; // closing fence
      blocks.push(
        <pre
          key={key++}
          className="my-2 p-3 rounded-lg bg-paper-sunken border border-line overflow-x-auto text-xs font-mono text-ink-soft"
        >
          <code>{body.join("\n")}</code>
        </pre>,
      );
      continue;
    }

    // blank
    if (line.trim() === "") {
      i += 1;
      continue;
    }

    // horizontal rule
    if (/^\s*([-*_])\1{2,}\s*$/.test(line)) {
      blocks.push(<hr key={key++} className="my-3 border-line" />);
      i += 1;
      continue;
    }

    // heading
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      const level = h[1].length;
      const size =
        level <= 1
          ? "text-lg font-semibold"
          : level === 2
            ? "text-base font-semibold"
            : "text-sm font-semibold";
      blocks.push(
        <div key={key++} className={`${size} text-ink mt-3 mb-1`}>
          {inline(h[2], `h${key}`)}
        </div>,
      );
      i += 1;
      continue;
    }

    // blockquote
    if (/^\s*>\s?/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        body.push(lines[i].replace(/^\s*>\s?/, ""));
        i += 1;
      }
      blocks.push(
        <blockquote
          key={key++}
          className="my-2 pl-3 border-l-2 border-line text-ink-soft italic"
        >
          {inline(body.join(" "), `bq${key}`)}
        </blockquote>,
      );
      continue;
    }

    // list (unordered or ordered)
    if (/^\s*([-*+]|\d+\.)\s+/.test(line)) {
      const items: string[] = [];
      const ordered = /^\s*\d+\.\s+/.test(line);
      while (i < lines.length && /^\s*([-*+]|\d+\.)\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*([-*+]|\d+\.)\s+/, ""));
        i += 1;
      }
      const cls = "my-2 ml-5 space-y-1 text-ink-soft " + (ordered ? "list-decimal" : "list-disc");
      blocks.push(
        <ul key={key++} className={cls}>
          {items.map((it, idx) => (
            <li key={idx}>{inline(it, `li${key}-${idx}`)}</li>
          ))}
        </ul>,
      );
      continue;
    }

    // paragraph (gather until blank)
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !/^\s*(#{1,6}\s|>|```|[-*+]\s|\d+\.\s)/.test(lines[i])) {
      para.push(lines[i]);
      i += 1;
    }
    blocks.push(
      <p key={key++} className="my-2 text-sm text-ink-soft leading-relaxed">
        {inline(para.join(" "), `p${key}`)}
      </p>,
    );
  }

  return <div className="max-w-none">{blocks}</div>;
};
