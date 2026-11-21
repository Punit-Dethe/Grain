// The result card: text the user asked to get back, changed ("turn this into
// an email", "more formal"), opened full in the notch with a Copy button
// (see docs/Agent Replies/PLAN.md). Unlike a generated view, its body scrolls:
// text about to be copied is never cut off or shrunk. Copy copies, shows a
// check, and the panel then closes the notch.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Copy } from "lucide-react";
import type { AgentResult } from "@/bindings";
import { AgentMarkdown } from "../markdown/Markdown";

/** Shell widths per layout, in CSS px (inside NOTCH_REPLY in agent.rs). */
const WIDTH = { short: 440, document: 560, code: 600 } as const;
/** Shell height bounds; past the max the body scrolls. */
export const RESULT_MIN_H = 170;
const RESULT_MAX_H = 560;
/** The shell header (`--notch-reply-header`) and the pane's frame (the pane
 * is the shell minus 2px wide, minus the header and 1px tall). */
const HEADER = 34;
const FRAME_H = 1;
/** Short: a sentence or a message, no subject. */
const SHORT_CHARS = 160;
const SHORT_LINES = 3;
/** How long the check shows before the notch closes. */
const COPIED_MS = 420;

/** Refine chips per kind: the label is also the request sent. */
const CHIPS: Record<string, string[]> = {
  email: ["moreFormal", "shorter", "friendlier"],
  message: ["shorter", "friendlier", "moreFormal"],
  rewrite: ["simpler", "shorter", "moreFormal"],
  list: ["shorter", "moreDetail"],
  code: ["addComments", "simplify"],
  translation: ["moreNatural", "moreFormal"],
};

export type ResultLayout = keyof typeof WIDTH;

export function resultLayout(result: AgentResult): ResultLayout {
  if (result.kind === "code") return "code";
  const short =
    !result.subject &&
    result.body.length <= SHORT_CHARS &&
    result.body.split("\n").length <= SHORT_LINES;
  return short ? "short" : "document";
}

/** The size the shell starts at before the card has measured itself. */
export const resultStartSize = (result: AgentResult) => ({
  w: WIDTH[resultLayout(result)],
  h: RESULT_MIN_H,
});

interface Props {
  result: AgentResult;
  busy: boolean;
  /** Copy the text; resolves true once it is on the clipboard. */
  onCopy: (text: string) => Promise<boolean>;
  /** Copied and shown: the panel closes the notch. */
  onDone: () => void;
  /** A refine chip: the follow-up request. */
  onAsk: (request: string) => void;
  /** The shell size this card needs, already clamped to the bounds. */
  onSize: (size: { w: number; h: number }) => void;
}

export function ResultCard({
  result,
  busy,
  onCopy,
  onDone,
  onAsk,
  onSize,
}: Props) {
  const { t } = useTranslation();
  const layout = resultLayout(result);
  const headRef = useRef<HTMLDivElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const footRef = useRef<HTMLDivElement>(null);
  const copyRef = useRef<HTMLButtonElement>(null);
  const onSizeRef = useRef(onSize);
  onSizeRef.current = onSize;
  const [copied, setCopied] = useState<"body" | "subject" | null>(null);

  // A new result starts uncopied, with Copy focused (Enter copies).
  useEffect(() => {
    setCopied(null);
    copyRef.current?.focus({ preventScroll: true });
  }, [result]);

  // The width is fixed per layout, so the text's height never depends on
  // the shell's: measuring it cannot feed back into itself.
  useLayoutEffect(() => {
    const parts = [headRef.current, textRef.current, footRef.current];
    if (parts.some((part) => !part)) return;
    let last = "";
    const measure = () => {
      const natural = parts.reduce(
        (sum, part) => sum + (part?.offsetHeight ?? 0),
        0,
      );
      const h = Math.max(
        RESULT_MIN_H,
        Math.min(RESULT_MAX_H, Math.ceil(HEADER + FRAME_H + natural)),
      );
      const w = WIDTH[layout];
      const key = `${w}x${h}`;
      if (key === last) return;
      last = key;
      onSizeRef.current({ w, h });
    };
    const observer = new ResizeObserver(measure);
    parts.forEach((part) => part && observer.observe(part));
    measure();
    return () => observer.disconnect();
  }, [layout]);

  useEffect(() => {
    if (copied !== "body") return;
    const timer = window.setTimeout(onDone, COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [copied, onDone]);

  const copy = (what: "body" | "subject") => {
    const text = what === "subject" ? result.subject : result.body;
    if (!text || busy || copied === "body") return;
    void onCopy(text).then((ok) => {
      if (ok) setCopied(what);
    });
  };

  const chips = CHIPS[result.kind] ?? CHIPS.rewrite;

  return (
    <div className={`agent-notch-result is-${layout}`}>
      <div ref={headRef} className="ars-head">
        <span className="ars-kind">
          {t(`agent.resultKind.${result.kind}`, {
            defaultValue: t("agent.resultKind.rewrite"),
          })}
        </span>
        {result.subject && (
          <div className="ars-subject">
            <span className="ars-subject-text">{result.subject}</span>
            <button
              type="button"
              className="ars-subject-copy"
              onClick={() => copy("subject")}
              aria-label={t("agent.resultCopySubject")}
              title={t("agent.resultCopySubject")}
            >
              {copied === "subject" ? <Check size={13} /> : <Copy size={13} />}
            </button>
          </div>
        )}
      </div>

      <div className="ars-body">
        <div
          ref={textRef}
          className={`ars-text${busy ? " is-stale" : ""}${layout === "code" ? " is-code" : ""}`}
        >
          {result.kind === "list" ? (
            <AgentMarkdown markdown={result.body} />
          ) : (
            result.body
          )}
        </div>
      </div>

      <div ref={footRef} className="ars-foot">
        <div className="ars-chips">
          {chips.map((chip) => (
            <button
              key={chip}
              type="button"
              className="ars-chip"
              disabled={busy}
              onClick={() => onAsk(t(`agent.resultChip.${chip}`))}
            >
              {t(`agent.resultChip.${chip}`)}
            </button>
          ))}
        </div>
        <button
          ref={copyRef}
          type="button"
          className={`ars-copy${copied === "body" ? " is-done" : ""}`}
          disabled={busy}
          onClick={() => copy("body")}
        >
          {copied === "body" ? (
            <Check size={15} strokeWidth={2.4} />
          ) : (
            <Copy size={14} strokeWidth={2.1} />
          )}
          <span>
            {copied === "body"
              ? t("agent.resultCopied")
              : t("agent.resultCopy")}
          </span>
        </button>
      </div>
    </div>
  );
}
