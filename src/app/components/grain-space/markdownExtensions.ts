import type { MarkdownConfig } from "@lezer/markdown";
import { GFM, Subscript, Superscript } from "@lezer/markdown";
import { Tag, tags } from "@lezer/highlight";

/**
 * [GRAIN] Obsidian-flavored markdown grammar for the Grain Space editor.
 *
 * Bundles GFM (tables, task lists, strikethrough, autolinks) and
 * super/subscript from `@lezer/markdown`, then adds the inline constructs
 * Obsidian ships on top of CommonMark: ==highlights==, [[wikilinks]],
 * #tags, %%comments%%, $inline math$ and [^footnote] references.
 *
 * Each construct defines its own syntax nodes + marks so the live-preview
 * concealment (which hides `*Mark` nodes off the cursor line) and the
 * highlight style can target them precisely. Custom tags are exported for
 * the editor's HighlightStyle to colour.
 */

/** Custom highlight tags for the Obsidian-only constructs. */
export const grainTags = {
  highlight: Tag.define(),
  wikilink: Tag.define(),
  hashtag: Tag.define(),
  comment: Tag.define(),
  math: Tag.define(),
  footnote: Tag.define(),
};

const CHAR = {
  eq: 61, // =
  bracketOpen: 91, // [
  bracketClose: 93, // ]
  hash: 35, // #
  percent: 37, // %
  dollar: 36, // $
  caret: 94, // ^
};

/** True for characters that may not sit flush against an opening delimiter. */
function isSpace(code: number): boolean {
  return code < 0 || code === 32 || code === 9 || code === 10 || code === 13;
}

/** ==highlight== — GFM-style paired delimiter, mirrors Strikethrough. */
const HighlightDelim = { resolve: "Highlight", mark: "HighlightMark" };
const Highlight: MarkdownConfig = {
  defineNodes: [
    { name: "Highlight", style: { "Highlight/...": grainTags.highlight } },
    { name: "HighlightMark", style: tags.processingInstruction },
  ],
  parseInline: [
    {
      name: "Highlight",
      parse(cx, next, pos) {
        if (next !== CHAR.eq || cx.char(pos + 1) !== CHAR.eq) return -1;
        // `===` (and longer runs) are not highlights.
        if (cx.char(pos + 2) === CHAR.eq) return -1;
        const spaceBefore = isSpace(cx.char(pos - 1));
        const spaceAfter = isSpace(cx.char(pos + 2));
        return cx.addDelimiter(
          HighlightDelim,
          pos,
          pos + 2,
          !spaceAfter,
          !spaceBefore,
        );
      },
      after: "Emphasis",
    },
  ],
};

/** Scan an inline section for a literal closer, staying on one line. */
function scanFor(
  cx: { char(pos: number): number; end: number },
  from: number,
  a: number,
  b: number | null,
): number {
  for (let p = from; p < cx.end; p++) {
    const c = cx.char(p);
    if (c === 10) return -1; // never cross a hard line break
    if (c === a && (b == null || cx.char(p + 1) === b)) return p;
  }
  return -1;
}

/** [[wikilink]] and [[target|alias]]. */
const Wikilink: MarkdownConfig = {
  defineNodes: [
    { name: "Wikilink", style: grainTags.wikilink },
    { name: "WikilinkMark", style: tags.processingInstruction },
  ],
  parseInline: [
    {
      name: "Wikilink",
      parse(cx, next, pos) {
        if (next !== CHAR.bracketOpen || cx.char(pos + 1) !== CHAR.bracketOpen)
          return -1;
        const close = scanFor(
          cx,
          pos + 2,
          CHAR.bracketClose,
          CHAR.bracketClose,
        );
        if (close < 0 || close === pos + 2) return -1;
        const end = close + 2;
        return cx.addElement(
          cx.elt("Wikilink", pos, end, [
            cx.elt("WikilinkMark", pos, pos + 2),
            cx.elt("WikilinkMark", close, end),
          ]),
        );
      },
      before: "Link",
    },
  ],
};

/** %%comment%% — Obsidian inline comments (hidden in preview). */
const Comment: MarkdownConfig = {
  defineNodes: [
    { name: "Comment", style: grainTags.comment },
    { name: "CommentMark", style: tags.processingInstruction },
  ],
  parseInline: [
    {
      name: "Comment",
      parse(cx, next, pos) {
        if (next !== CHAR.percent || cx.char(pos + 1) !== CHAR.percent)
          return -1;
        const close = scanFor(cx, pos + 2, CHAR.percent, CHAR.percent);
        if (close < 0) return -1;
        const end = close + 2;
        return cx.addElement(
          cx.elt("Comment", pos, end, [
            cx.elt("CommentMark", pos, pos + 2),
            cx.elt("CommentMark", close, end),
          ]),
        );
      },
      before: "Emphasis",
    },
  ],
};

/** #tag — a hash directly against a run of tag characters (never a heading). */
const TAG_BODY = /[\p{L}\p{N}_/-]/u;
const Hashtag: MarkdownConfig = {
  defineNodes: [{ name: "Hashtag", style: grainTags.hashtag }],
  parseInline: [
    {
      name: "Hashtag",
      parse(cx, next, pos) {
        if (next !== CHAR.hash) return -1;
        const prev = cx.char(pos - 1);
        // Must start inline or follow whitespace / an opening bracket.
        if (
          prev >= 0 &&
          !isSpace(prev) &&
          prev !== 40 &&
          prev !== CHAR.bracketOpen
        )
          return -1;
        let p = pos + 1;
        let hasLetter = false;
        while (p < cx.end) {
          const ch = String.fromCharCode(cx.char(p));
          if (!TAG_BODY.test(ch)) break;
          if (/\p{L}/u.test(ch)) hasLetter = true;
          p++;
        }
        // Reject `# ` (heading), `#123` (pure numeric) and bare `#`.
        if (p === pos + 1 || !hasLetter) return -1;
        return cx.addElement(cx.elt("Hashtag", pos, p));
      },
      before: "Emphasis",
    },
  ],
};

/** $inline math$ — recognised and styled, but not rendered (no MathJax dep). */
const InlineMath: MarkdownConfig = {
  defineNodes: [
    { name: "InlineMath", style: { "InlineMath/...": grainTags.math } },
    { name: "MathMark", style: tags.processingInstruction },
  ],
  parseInline: [
    {
      name: "InlineMath",
      parse(cx, next, pos) {
        if (next !== CHAR.dollar) return -1;
        if (cx.char(pos + 1) === CHAR.dollar) return -1; // leave $$block$$ alone
        if (isSpace(cx.char(pos + 1))) return -1; // `$ ` is a literal dollar
        const close = scanFor(cx, pos + 1, CHAR.dollar, null);
        if (close < 0 || isSpace(cx.char(close - 1))) return -1;
        const end = close + 1;
        return cx.addElement(
          cx.elt("InlineMath", pos, end, [
            cx.elt("MathMark", pos, pos + 1),
            cx.elt("MathMark", close, end),
          ]),
        );
      },
      before: "Emphasis",
    },
  ],
};

/** [^ref] footnote references. */
const Footnote: MarkdownConfig = {
  defineNodes: [{ name: "FootnoteRef", style: grainTags.footnote }],
  parseInline: [
    {
      name: "FootnoteRef",
      parse(cx, next, pos) {
        if (next !== CHAR.bracketOpen || cx.char(pos + 1) !== CHAR.caret)
          return -1;
        const close = scanFor(cx, pos + 2, CHAR.bracketClose, null);
        if (close < 0 || close === pos + 2) return -1;
        return cx.addElement(cx.elt("FootnoteRef", pos, close + 1));
      },
      before: "Link",
    },
  ],
};

/** The full extension bundle handed to `markdown({ extensions })`. */
export const grainMarkdownExtensions = [
  GFM,
  Superscript,
  Subscript,
  Highlight,
  Wikilink,
  Comment,
  Hashtag,
  InlineMath,
  Footnote,
];
