import {
  type CSSProperties,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { CornerDownLeft, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, events, type AgentInputSnapshot } from "@/bindings";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { Waveform } from "@/overlay/Waveform";
import { agentInputKeyAction } from "./input-keys";
const generation = Number(
  new URLSearchParams(location.search).get("generation"),
);

export function AgentInput() {
  const { t, i18n } = useTranslation();
  const [snapshot, setSnapshot] = useState<AgentInputSnapshot | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  const stage = useRef<HTMLElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const inFlight = useRef(false);
  const returning = useRef(false);
  const returnTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  useEffect(() => {
    let disposed = false;
    let started = false;
    let release: (() => void) | undefined;
    const frames: number[] = [];
    const activate = async () => {
      if (disposed || started) return;
      const result = await commands.agentInputSnapshot(generation);
      if (disposed || started) return;
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      setSnapshot(result.data);
      if (!result.data.ready) return;
      started = true;
      const reveal = await commands.agentInputReveal(generation);
      if (disposed) return;
      if (reveal.status === "error") {
        setError(reveal.error);
        return;
      }
      // Paint compact geometry after native handoff, before the morph begins.
      frames.push(
        requestAnimationFrame(() => {
          frames.push(
            requestAnimationFrame(() => {
              if (!disposed) setExpanded(true);
            }),
          );
        }),
      );
    };
    void events.agentInputReady
      .listen(() => {
        void activate().catch(console.error);
      })
      .then((fn) => {
        if (disposed) fn();
        else {
          release = fn;
          void activate().catch(console.error);
        }
      })
      .catch(console.error);
    return () => {
      disposed = true;
      release?.();
      frames.forEach(cancelAnimationFrame);
      clearTimeout(returnTimer.current);
    };
  }, []);
  useLayoutEffect(() => {
    content.current?.toggleAttribute("inert", !expanded);
    if (snapshot) {
      if (expanded) field.current?.focus();
      else stage.current?.focus();
    }
  }, [snapshot, expanded]);
  const request = async (
    action: () => Promise<
      { status: "ok"; data: null } | { status: "error"; error: string }
    >,
  ) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError("");
    try {
      const result = await action();
      if (result.status === "error") setError(result.error);
    } catch (reason) {
      setError(String(reason));
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  };
  const submit = () => {
    if (!snapshot || !text.trim() || pending || !expanded) return;
    void request(() =>
      commands.agentInputSubmit(generation, text, snapshot.quick),
    );
  };
  const cancel = () => {
    returning.current = false;
    clearTimeout(returnTimer.current);
    void request(() => commands.agentInputCancel(generation));
  };
  const finishReturn = () => {
    if (!returning.current) return;
    returning.current = false;
    clearTimeout(returnTimer.current);
    void request(() => commands.agentInputSpeak(generation));
  };
  const speak = () => {
    if (!snapshot || pending || !expanded) return;
    returning.current = true;
    setPending(true);
    setExpanded(false);
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) finishReturn();
    else returnTimer.current = setTimeout(finishReturn, 460);
  };
  const iconEnabled = snapshot?.pill.presentation.app_icon_enabled ?? true;
  const closeEnabled = !(snapshot?.pill.pill_hide_close_button ?? false);
  const slots = 1 + Number(iconEnabled) + Number(closeEnabled);
  const compactWidth =
    51 +
    22 +
    (iconEnabled ? 18 : 0) +
    (closeEnabled ? 22 : 0) +
    (slots - 1) * 12;
  const columns = [
    iconEnabled ? "18px" : "",
    "51px",
    closeEnabled ? "22px" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const position = snapshot?.pill.position ?? "bottom";
  return (
    <main
      ref={stage}
      tabIndex={-1}
      className={`agent-input-stage ${position}`}
      dir={getLanguageDirection(i18n.language)}
      onKeyDown={(event) => {
        const action = agentInputKeyAction(
          event.key,
          event.shiftKey,
          event.nativeEvent.isComposing,
          event.nativeEvent.keyCode,
        );
        if (action === "cancel") {
          event.preventDefault();
          cancel();
        }
        if (action === "switch") {
          event.preventDefault();
          speak();
        }
      }}
    >
      <form
        className={`scard agent agent-input-card ${expanded ? "is-open" : ""}`}
        style={
          { "--agent-compact-width": `${compactWidth}px` } as CSSProperties
        }
        aria-label={t("agent.brand")}
        onTransitionEnd={(event) => {
          if (
            event.target === event.currentTarget &&
            event.propertyName === "height"
          )
            finishReturn();
        }}
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <div
          className="agent-input-compact sbase"
          style={{ gridTemplateColumns: columns }}
          aria-hidden="true"
        >
          {iconEnabled && (
            <div className="sbase-l">
              {snapshot?.pill.presentation.icon ? (
                <img src={snapshot.pill.presentation.icon} alt="" />
              ) : (
                <span className="sdot ready" />
              )}
            </div>
          )}
          {!expanded && <Waveform ready session={generation} />}
          {closeEnabled && (
            <div className="sbase-r">
              <span className="sx">
                <X size={12} />
              </span>
            </div>
          )}
        </div>
        <div className="agent-input-content" ref={content}>
          <header className="agent-input-head">
            <span className="agent-input-title">{t("agent.brand")}</span>
            {snapshot && snapshot.selection_chars > 0 && (
              <span
                className="agent-input-context"
                title={t("agent.selectionChip", {
                  count: snapshot.selection_chars,
                })}
              >
                {t("agent.inputSelection", { count: snapshot.selection_chars })}
              </span>
            )}
            <button
              type="button"
              className="agent-input-close"
              aria-label={t("agent.escCue")}
              title={t("agent.escCue")}
              disabled={pending}
              onClick={cancel}
            >
              <X size={15} />
            </button>
          </header>
          <textarea
            ref={field}
            className="agent-input-field"
            value={text}
            aria-label={t("agent.inputInstruction")}
            placeholder={t("agent.inputPlaceholder")}
            disabled={!snapshot}
            readOnly={pending}
            spellCheck
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => {
              if (
                agentInputKeyAction(
                  event.key,
                  event.shiftKey,
                  event.nativeEvent.isComposing,
                  event.nativeEvent.keyCode,
                ) === "send"
              ) {
                event.preventDefault();
                submit();
              }
            }}
          />
          <footer className="agent-input-foot">
            <span className="agent-input-hint">{t("agent.inputHint")}</span>
            <button
              type="submit"
              className="agent-input-send"
              disabled={!snapshot || pending || !text.trim()}
              aria-label={t("agent.inputSend")}
              title={t("agent.inputSend")}
            >
              <span>{t("agent.inputEnter")}</span>
              <CornerDownLeft size={15} />
            </button>
          </footer>
          {error && (
            <p className="agent-input-error" role="alert">
              {error}
            </p>
          )}
        </div>
      </form>
    </main>
  );
}
