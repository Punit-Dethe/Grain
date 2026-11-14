// A generated view, opened full in the notch from its card in the chat
// (experiment, see docs/Generative UI/PLAN.md). The shell fits the view, within
// bounds; a view never scrolls: one taller than the bounds is scaled to fit.
import "./canvas.css";
import {
  Component,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { Renderer, type ActionEvent } from "@openuidev/react-lang";
import { ArrowUp, Square } from "lucide-react";
import { commands } from "@/bindings";
import { AgentMarkdown } from "../../markdown/Markdown";
import { canvasLibrary } from "./library";
import { canvasProgram, withoutSpeech } from "./program";

/** The view shell's bounds, in CSS px. The window is NOTCH_REPLY in
 * agent.rs: keep these (plus the clip slack) inside it. `.gui-content`'s
 * max-width in canvas.css is CANVAS_MAX.w - 2 * PAD_X. */
export const CANVAS_MIN = { w: 360, h: 150 };
export const CANVAS_MAX = { w: 720, h: 600 };
/** The shell header (`--notch-reply-header` in agent.css). */
const HEADER = 34;
/** Horizontal padding of the view body (`.gui-body` in canvas.css). */
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
  /** The view (OpenUI Lang, or plain text from a model that broke format). */
  ui: string;
  busy: boolean;
  composerRef: React.RefObject<HTMLTextAreaElement>;
  hasDraft: boolean;
  onDraft: (hasDraft: boolean) => void;
  /** A follow-up: typed, or sent by a button in the view. */
  onAsk: (request: string) => void;
  onStop: () => void;
  /** The shell size this view needs, already clamped to the bounds. */
  onSize: (size: { w: number; h: number }) => void;
}

export function AgentCanvas({
  ui,
  busy,
  composerRef,
  hasDraft,
  onDraft,
  onAsk,
  onStop,
  onSize,
}: Props) {
  const { t } = useTranslation();
  const contentRef = useRef<HTMLDivElement>(null);
  const footRef = useRef<HTMLDivElement>(null);
  const onSizeRef = useRef(onSize);
  onSizeRef.current = onSize;
  // Natural size and the scale that fits it under the max height.
  const [fit, setFit] = useState({ w: 0, h: 0, scale: 1 });

  // The content lays out at its own natural width (capped), independent of
  // the shell; a transform does not change its layout size, so measuring it
  // never feeds back into itself.
  useLayoutEffect(() => {
    const content = contentRef.current;
    const foot = footRef.current;
    if (!content || !foot) return;
    let last = "";
    const measure = () => {
      const naturalW = content.offsetWidth;
      const naturalH = content.offsetHeight;
      const room = CANVAS_MAX.h - HEADER - foot.offsetHeight;
      const scale = naturalH > room ? room / naturalH : 1;
      const w = clamp(naturalW * scale + PAD_X * 2, CANVAS_MIN.w, CANVAS_MAX.w);
      const h = clamp(
        HEADER + naturalH * scale + foot.offsetHeight,
        CANVAS_MIN.h,
        CANVAS_MAX.h,
      );
      const key = `${naturalW}x${naturalH}x${w}x${h}`;
      if (key === last) return;
      last = key;
      setFit({ w: naturalW, h: naturalH, scale });
      onSizeRef.current({ w, h });
    };
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    observer.observe(foot);
    measure();
    return () => observer.disconnect();
  }, []);

  const program = canvasProgram(ui);
  const text = withoutSpeech(ui);

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
    const value = input?.value.trim();
    if (!input || !value || busy) return;
    input.value = "";
    input.style.height = "auto";
    onDraft(false);
    onAsk(value);
  };

  const scaled = fit.scale < 1;

  return (
    <div className="agent-notch-canvas">
      <div className="gui-body">
        <div
          className="gui-fit"
          style={
            scaled
              ? { width: fit.w * fit.scale, height: fit.h * fit.scale }
              : undefined
          }
        >
          <div
            ref={contentRef}
            className={`gui-content${busy ? " is-stale" : ""}`}
            style={scaled ? { transform: `scale(${fit.scale})` } : undefined}
          >
            {program ? (
              <CanvasBoundary
                key={program}
                fallback={<AgentMarkdown markdown={text} />}
              >
                <Renderer
                  response={program}
                  library={canvasLibrary}
                  isStreaming={false}
                  onAction={onAction}
                />
              </CanvasBoundary>
            ) : (
              <div className="gui-markdown">
                <AgentMarkdown markdown={text} />
              </div>
            )}
          </div>
        </div>
      </div>

      <div ref={footRef} className="gui-foot">
        <div className="agc-c-compose">
          <div className="agc-c-field">
            <textarea
              ref={composerRef}
              rows={1}
              className="agc-c-textarea"
              aria-label={t("agent.placeholderAsk")}
              placeholder={t("agent.placeholderAsk")}
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
