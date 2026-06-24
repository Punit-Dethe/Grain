import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands } from "@/bindings";
import "./agent.css";

type Status = "recording" | "idle" | "transcribing";

const WAVE_ROWS = 4;
const WAVE_COLS = 12;

/** Recording indicator — a 4×12 dot grid (corners removed) with a ruler-wave
 * ripple, mirroring the reference palette. Driven by rAF (no React re-renders). */
function WaveGrid() {
  const dots = useRef<(HTMLSpanElement | null)[]>([]);
  useEffect(() => {
    let raf = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const angle = (((now - start) % 2000) / 2000) * Math.PI * 2;
      for (let i = 0; i < dots.current.length; i++) {
        const el = dots.current[i];
        if (!el) continue;
        const c = i % WAVE_COLS;
        const r = Math.floor(i / WAVE_COLS);
        el.style.opacity = String(
          Math.max(0, Math.sin(angle * 2.1 + c * 1.37 + r * 3.11)),
        );
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const cells = [];
  for (let i = 0; i < WAVE_ROWS * WAVE_COLS; i++) {
    const c = i % WAVE_COLS;
    const r = Math.floor(i / WAVE_COLS);
    const corner =
      (r === 0 || r === WAVE_ROWS - 1) && (c === 0 || c === WAVE_COLS - 1);
    cells.push(
      <span
        key={i}
        ref={(el) => {
          dots.current[i] = el;
        }}
        className="agent-wave-dot"
        style={{ visibility: corner ? "hidden" : "visible" }}
      />,
    );
  }
  return <div className="agent-wave">{cells}</div>;
}

/**
 * [GRAIN] The Agent palette — the centred summon bar.
 *
 * Records by default on open; typing abandons the voice capture; Enter submits
 * (typed text wins, otherwise the in-progress recording is transcribed). On
 * submit it hands the instruction to the panel, opens it, and closes itself. It
 * shows only the selection's char count — never the full text.
 */
export function AgentPalette() {
  const { t } = useTranslation();
  const win = getCurrentWindow();

  const [selectionLen, setSelectionLen] = useState(0);
  const [status, setStatus] = useState<Status>("idle");
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);
  const statusRef = useRef<Status>("idle");
  statusRef.current = status;
  const submittingRef = useRef(false);
  const startedRef = useRef(false);

  const recording = status === "recording";
  const transcribing = status === "transcribing";

  const focusInput = useCallback(() => {
    window.setTimeout(() => inputRef.current?.focus({ preventScroll: true }), 0);
  }, []);

  const startRecording = useCallback(async () => {
    setError(null);
    try {
      const res = await commands.agentStartDictation();
      if (res.status === "ok") {
        setStatus("recording");
      } else {
        setStatus("idle");
        setError(res.error || t("agent.error"));
      }
    } catch (e) {
      setStatus("idle");
      setError(e instanceof Error ? e.message : t("agent.error"));
    } finally {
      focusInput();
    }
  }, [focusInput, t]);

  // On mount: read the selection (char count only) and start recording. Guarded
  // so React StrictMode's double-invoke (dev) doesn't start dictation twice.
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    (async () => {
      try {
        const ctx = await commands.agentGetContext();
        setSelectionLen(ctx ? ctx.length : 0);
      } catch {
        /* no selection is fine */
      }
      await startRecording();
    })();
    focusInput();
    const t1 = window.setTimeout(focusInput, 80);
    const t2 = window.setTimeout(focusInput, 220);
    return () => {
      window.clearTimeout(t1);
      window.clearTimeout(t2);
    };
  }, [focusInput, startRecording]);

  const closeWindow = useCallback(() => {
    void commands.agentCancelDictation().catch(() => {});
    void win.close();
  }, [win]);

  const handToPanel = useCallback(
    async (instruction: string) => {
      const text = instruction.trim();
      if (!text) {
        setStatus("idle");
        return;
      }
      const res = await commands.agentSubmitInstruction(text);
      if (res.status === "error") throw new Error(res.error);
    },
    [],
  );

  const submit = useCallback(async () => {
    if (submittingRef.current) return;
    submittingRef.current = true;

    try {
      const typed = input.trim();
      if (typed) {
        await handToPanel(typed);
        return;
      }

      // No typed text: transcribe the in-progress voice capture (if any).
      if (statusRef.current === "recording") {
        setStatus("transcribing");
        setError(null);
        const res = await commands.agentStopDictation();
        if (res.status === "ok" && res.data.trim()) {
          await handToPanel(res.data);
          return;
        }
        if (res.status === "error") {
          setError(res.error || t("agent.error"));
        }
        setStatus("idle");
      }
    } catch (e) {
      setStatus("idle");
      setError(e instanceof Error ? e.message : t("agent.error"));
    } finally {
      submittingRef.current = false;
      focusInput();
    }
  }, [focusInput, handToPanel, input, t]);

  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void submit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeWindow();
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        void submit();
      } else if (e.key === "Escape") {
        e.preventDefault();
        closeWindow();
      } else if (
        e.key.length === 1 &&
        document.activeElement !== inputRef.current
      ) {
        focusInput();
      }
    };

    window.addEventListener("keydown", onKey);
    window.addEventListener("focus", focusInput);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("focus", focusInput);
    };
  }, [closeWindow, focusInput, submit]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void win.listen("agent-global-enter", () => {
      void submit();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [submit, win]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void win.listen<string>("agent-submit-error", (event) => {
      submittingRef.current = false;
      setStatus("idle");
      setError(event.payload || t("agent.error"));
      focusInput();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [focusInput, t, win]);

  const onChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    // Typing abandons the voice capture — the user chose to type.
    if (statusRef.current === "recording") {
      void commands.agentCancelDictation().catch(() => {});
      setStatus("idle");
    }
    if (error) setError(null);
    setInput(e.target.value);
  };

  const placeholder = recording
    ? t("agent.placeholderListening")
    : selectionLen > 0
      ? t("agent.placeholderSelection")
      : t("agent.placeholderAsk");

  return (
    <div className="agent-palette" onPointerDownCapture={focusInput}>
      <div className="agent-card agent-card--palette">
        {/* Header */}
        <div className="agent-pal-head">
          <span className="agent-dot-orange" />
          <span className="agent-brand">{t("agent.brand")}</span>
          <span className="agent-spacer" />
          {selectionLen > 0 ? (
            <span className="agent-sel-chip">
              {t("agent.selectionChip", { count: selectionLen })}
            </span>
          ) : (
            <span className="agent-sel-none">{t("agent.noSelection")}</span>
          )}
        </div>

        {/* Input */}
        <div className={`agent-pal-input ${input || status !== "idle" ? "is-focus" : ""}`}>
          <input
            ref={inputRef}
            type="text"
            className="agent-pal-field"
            placeholder={placeholder}
            value={input}
            disabled={transcribing}
            onChange={onChange}
            onKeyDown={onInputKeyDown}
          />
        </div>

        {/* Footer */}
        <div className="agent-pal-foot">
          <div className="agent-foot-left">
            {recording ? (
              <WaveGrid />
            ) : (
              status === "idle" && (
                <button
                  type="button"
                  className="agent-speak"
                  onClick={() => {
                    setInput("");
                    void startRecording();
                  }}
                >
                  <span className="agent-mic-glyph" />
                  {t("agent.speak")}
                </button>
              )
            )}
          </div>

          <span
            className={`agent-foot-status ${
              error
                ? "is-error"
                : recording || status === "transcribing"
                  ? "is-active"
                  : ""
            }`}
          >
            {error
              ? error
              : status === "transcribing"
                ? t("agent.transcribing")
                : recording
                  ? t("agent.recordingHint")
                  : ""}
          </span>

          <span className="agent-cue">{t("agent.sendCue")}</span>
          <span className="agent-cue">{t("agent.escCue")}</span>
        </div>
      </div>
    </div>
  );
}
