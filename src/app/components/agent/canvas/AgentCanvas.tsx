// The Canvas view: the Agent answers with a generated UI instead of chat text
// (experiment, see docs/Generative UI/PLAN.md). Only the latest UI shows; the
// conversation behind it is host-held (grain_agent_canvas.rs).
import "./devtoolsOff";
import "./canvas.css";
import { Component, useLayoutEffect, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Renderer, type ActionEvent } from "@openuidev/react-lang";
import { ArrowUp, Square } from "lucide-react";
import { commands, type AgentConfirm } from "@/bindings";
import { AgentMarkdown } from "../../markdown/Markdown";
import { canvasLibrary } from "./library";
import { canvasProgram, withoutSpeech } from "./program";

/** The Canvas shell's bounds, in CSS px. The window is NOTCH_REPLY in
 * agent.rs: keep these (plus the clip slack) inside it. */
export const CANVAS_MIN = { w: 360, h: 150 };
export const CANVAS_MAX = { w: 620, h: 540 };
/** The shell header (`--notch-reply-header` in agent.css). */
const HEADER = 34;
/** Horizontal padding of the canvas body (`.gui-body` in agent.css). */
const PAD_X = 16;

const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, Math.ceil(value)));

/** A component that throws must not take the notch down with it. */
class CanvasBoundary extends Component<
  { fallback: ReactNode; children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

/** What a UI interaction asks for next: the button's request, plus any form
 * values and hidden context it carries. */
function nextRequest(event: ActionEvent): string | null {
  const message = event.humanFriendlyMessage?.trim();
  if (!message) return null;
  const extra: string[] = [];
  const context = event.params?.context;
  if (context !== undefined && context !== null && context !== "")
    extra.push(
      `Context: ${typeof context === "string" ? context : JSON.stringify(context)}`,
    );
  if (event.formName && event.formState) {
    const fields = Object.values(event.formState)[0];
    if (fields && typeof fields === "object" && Object.keys(fields).length)
      extra.push(`Form "${event.formName}": ${JSON.stringify(fields)}`);
  }
  return extra.length ? `${message}\n\n${extra.join("\n")}` : message;
}

interface Props {
  /** The latest reply (OpenUI Lang, or plain text from a model that ignored
   * the format); `null` before the first one. */
  reply: string | null;
  busy: boolean;
  error: string | null;
  confirm: AgentConfirm | null;
  composerRef: React.RefObject<HTMLTextAreaElement>;
  hasDraft: boolean;
  onDraft: (hasDraft: boolean) => void;
  onAsk: (request: string) => void;
  onStop: () => void;
  onResolve: (pending: AgentConfirm, approve: boolean) => void;
  /** The shell size this content needs, already clamped to the bounds. */
  onSize: (size: { w: number; h: number }) => void;
}

export function AgentCanvas({
  reply,
  busy,
  error,
  confirm,
  composerRef,
  hasDraft,
  onDraft,
  onAsk,
  onStop,
  onResolve,
  onSize,
}: Props) {
  const { t } = useTranslation();
  const contentRef = useRef<HTMLDivElement>(null);
  const footRef = useRef<HTMLDivElement>(null);
  const onSizeRef = useRef(onSize);
  onSizeRef.current = onSize;

  // The content lays out at its own natural width (capped), independent of
  // the shell, so measuring it never feeds back into itself.
  useLayoutEffect(() => {
    const content = contentRef.current;
    const foot = footRef.current;
    if (!content || !foot) return;
    let last = "";
    const measure = () => {
      const w = clamp(
        content.offsetWidth + PAD_X * 2,
        CANVAS_MIN.w,
        CANVAS_MAX.w,
      );
      const h = clamp(
        HEADER + content.offsetHeight + foot.offsetHeight,
        CANVAS_MIN.h,
        CANVAS_MAX.h,
      );
      const key = `${w}x${h}`;
      if (key === last) return;
      last = key;
      onSizeRef.current({ w, h });
    };
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    observer.observe(foot);
    measure();
    return () => observer.disconnect();
  }, []);

  const program = reply ? canvasProgram(reply) : null;
  // A reply restored from memory still carries its voice line: never shown.
  const text = reply ? withoutSpeech(reply) : "";

  const onAction = (event: ActionEvent) => {
    if (busy) return;
    if (event.type === "open_url") {
      const url = event.params?.url;
      if (typeof url === "string")
        void commands.agentOpenLink(url).catch(console.error);
      return;
    }
    if (event.type === "continue_conversation") {
      const request = nextRequest(event);
      if (request) onAsk(request);
    }
  };

  const send = () => {
    const input = composerRef.current;
    const text = input?.value.trim();
    if (!input || !text || busy) return;
    input.value = "";
    onDraft(false);
    onAsk(text);
  };

  return (
    <div className="agent-notch-canvas">
      <div className="gui-body">
        <div
          ref={contentRef}
          className={`gui-content${busy && reply ? " is-stale" : ""}`}
        >
          {reply ? (
            program ? (
              <CanvasBoundary
                key={program}
                fallback={<AgentMarkdown markdown={text} />}
              >
                <Renderer
                  response={program}
                  library={canvasLibrary}
                  isStreaming={busy}
                  onAction={onAction}
                />
              </CanvasBoundary>
            ) : (
              <div className="gui-markdown">
                <AgentMarkdown markdown={text} />
              </div>
            )
          ) : busy ? null : (
            <div className="gui-empty">
              <span className="gui-heading is-s">{t("agent.canvasEmpty")}</span>
              <span className="gui-text is-faint">
                {t("agent.canvasEmptyHint")}
              </span>
            </div>
          )}
          {busy && !reply && (
            <div className="agent-typing" aria-hidden="true">
              <span />
              <span />
              <span />
            </div>
          )}
          {error && <div className="agc-error">{error}</div>}
        </div>
      </div>

      <div ref={footRef} className="gui-foot">
        {confirm && (
          <div className="agc-evidence agc-confirm-action">
            <span className="agc-confirm-q">{confirm.summary}</span>
            <div className="agc-confirm-actions">
              <button
                type="button"
                className="agc-action-btn"
                disabled={busy}
                onClick={() => onResolve(confirm, true)}
              >
                {t("agent.confirm")}
              </button>
              <button
                type="button"
                className="agc-cancel-btn"
                disabled={busy}
                onClick={() => onResolve(confirm, false)}
              >
                {t("tray.cancel")}
              </button>
            </div>
          </div>
        )}
        <div className="agc-c-compose">
          <div className="agc-c-field">
            <textarea
              ref={composerRef}
              rows={1}
              className="agc-c-textarea"
              aria-label={t("agent.canvasPlaceholder")}
              placeholder={t("agent.canvasPlaceholder")}
              onInput={(event) => {
                const input = event.currentTarget;
                input.style.height = "auto";
                input.style.height = `${Math.min(input.scrollHeight, 76)}px`;
                onDraft(!!input.value.trim());
              }}
              onKeyDown={(event) => {
                if (
                  event.key !== "Enter" ||
                  event.shiftKey ||
                  event.nativeEvent.isComposing
                )
                  return;
                event.preventDefault();
                send();
              }}
            />
            {busy ? (
              <button
                type="button"
                className="agc-c-send agc-c-stop"
                onClick={onStop}
                aria-label={t("agent.stop")}
                title={t("agent.stop")}
              >
                <Square size={10} fill="currentColor" strokeWidth={0} />
              </button>
            ) : (
              <button
                type="button"
                className="agc-c-send"
                disabled={!hasDraft}
                onClick={send}
                aria-label={t("agent.send")}
                title={t("agent.send")}
              >
                <ArrowUp size={15} strokeWidth={2.25} />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
