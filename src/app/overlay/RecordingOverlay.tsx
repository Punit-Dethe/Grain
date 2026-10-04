import { type UnlistenFn } from "@tauri-apps/api/event";
import { memo, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  commands,
  events,
  type OverlayPresentation,
  type PillSkin,
  type StreamTextEvent,
} from "@/bindings";
import { audioAmplitude, barCount, WaveField } from "./wave";
import { MatrixField } from "./matrix";
import { useTranslation } from "react-i18next";
import "@/i18n";

const empty: OverlayPresentation = {
  visible: false,
  state: "recording",
  ready: false,
  session_id: 0,
  agent: false,
  owner: null,
  icon: null,
  notice: null,
  followup: null,
  committed: "",
  tentative: "",
  working: false,
  work_kind: "transcribing",
};

const Waveform = memo(function Waveform({
  ready,
  session,
  skin,
  live,
}: {
  ready: boolean;
  session: number;
  skin: PillSkin;
  live: boolean;
}) {
  const row = useRef<HTMLDivElement>(null);
  const amplitude = useRef(0);
  const [count, setCount] = useState(14);
  const [field] = useState(() => new WaveField());
  const [matrix] = useState(() => new MatrixField());
  useEffect(() => {
    let disposed = false;
    let release: UnlistenFn | undefined;
    void events.micLevel
      .listen(({ payload }) => {
        if (!disposed) amplitude.current = audioAmplitude(payload);
      })
      .then((fn) => {
        if (disposed) fn();
        else release = fn;
      })
      .catch(console.error);
    return () => {
      disposed = true;
      release?.();
    };
  }, []);
  useEffect(() => {
    field.reset();
    matrix.reset();
    amplitude.current = 0;
  }, [session, field, matrix]);
  useLayoutEffect(() => {
    const element = row.current;
    if (!element) return;
    const observer = new ResizeObserver(() =>
      setCount(barCount(element.clientWidth)),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const element = row.current;
    if (!element) return;
    const bars = element.children;
    let frame = 0;
    let previous = performance.now();
    const animate = (now: number) => {
      const dt = (now - previous) / 1000;
      const changed =
        skin !== "matrix" ||
        matrix.advance(dt, ready ? amplitude.current : 0, live);
      if (skin !== "matrix")
        field.advance(dt, ready ? amplitude.current : 0, count);
      previous = now;
      for (let i = 0; changed && i < bars.length; i++) {
        const bar = bars[i] as HTMLElement;
        if (skin === "matrix") {
          const start = (i + (live ? 75 : 0)) * 4;
          bar.style.backgroundColor = `rgb(${matrix.dots[start]} ${matrix.dots[start + 1]} ${matrix.dots[start + 2]})`;
          bar.style.opacity = String(matrix.dots[start + 3] / 255);
        } else bar.style.height = `${Math.max(2, field.bars[i] * 22)}px`;
      }
      frame = requestAnimationFrame(animate);
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [count, ready, skin, live, field, matrix]);
  return (
    <div
      ref={row}
      className={`wave ${skin} ${live ? "live" : ""} ${ready ? "ready" : "arming"}`}
      aria-hidden="true"
    >
      {Array.from(
        { length: skin === "matrix" ? (live ? 50 : 200) : count },
        (_, i) => (
          <i key={i} />
        ),
      )}
    </div>
  );
});

export function RecordingOverlay() {
  const { t } = useTranslation();
  const [presentation, setPresentation] = useState(empty);
  const [skin, setSkin] = useState<PillSkin>("wave");
  const [position, setPosition] = useState("bottom");
  const [text, setText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [working, setWorking] = useState(false);
  const [kind, setKind] = useState("transcribing");
  const [elapsed, setElapsed] = useState(0);
  const [session, setSession] = useState(0);
  const cap = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [overflowing, setOverflowing] = useState(false);

  useEffect(() => {
    let disposed = false;
    let revision = 0;
    const updated = { skin: false, position: false, theme: false };
    const releases: UnlistenFn[] = [];
    const register = async (promise: Promise<UnlistenFn>) => {
      const fn = await promise;
      if (disposed) fn();
      else releases.push(fn);
    };
    const reset = () => {
      setText({ committed: "", tentative: "" });
      setWorking(false);
      setKind("transcribing");
      setElapsed(0);
      setSession((value) => value + 1);
      pinned.current = true;
      setOverflowing(false);
    };
    const initialize = async () => {
      const apply = (value: OverlayPresentation) => {
        setPresentation(value);
        setText({ committed: value.committed, tentative: value.tentative });
        setWorking(value.working);
        setKind(value.work_kind || "transcribing");
      };
      const registrations = await Promise.allSettled([
        register(
          events.grainOverlayContext.listen(({ payload }) => {
            if (disposed) return;
            revision++;
            apply(payload);
          }),
        ),
        register(
          events.showOverlay.listen(({ payload }) => {
            if (disposed) return;
            revision++;
            if (payload === "recording" || payload === "streaming") reset();
            setPresentation((value) => ({
              ...value,
              visible: true,
              state: payload,
              ready:
                payload === "recording" || payload === "streaming"
                  ? false
                  : value.ready,
            }));
          }),
        ),
        register(
          events.hideOverlay.listen(() => {
            if (disposed) return;
            revision++;
            setPresentation((value) => ({
              ...value,
              visible: false,
              ready: false,
            }));
            setText({ committed: "", tentative: "" });
            setWorking(false);
            setElapsed(0);
          }),
        ),
        register(
          events.recordingReady.listen(() => {
            if (disposed) return;
            revision++;
            setElapsed(0);
            setPresentation((value) => ({ ...value, ready: true }));
          }),
        ),
        register(
          events.streamTextEvent.listen(({ payload }) => {
            if (!disposed) {
              revision++;
              setText(payload);
            }
          }),
        ),
        register(
          events.streamPhaseEvent.listen(({ payload }) => {
            if (!disposed) {
              revision++;
              setWorking(payload.phase === "working");
              if (payload.kind) setKind(payload.kind);
            }
          }),
        ),
        register(
          events.themeChanged.listen(({ payload }) => {
            if (!disposed) {
              updated.theme = true;
              document.documentElement.dataset.theme = payload.resolved;
            }
          }),
        ),
        register(
          events.grainOverlaySkin.listen(({ payload }) => {
            if (!disposed) {
              updated.skin = true;
              setSkin(payload);
            }
          }),
        ),
        register(
          events.grainOverlayPosition.listen(({ payload }) => {
            if (!disposed) {
              updated.position = true;
              setPosition(payload);
            }
          }),
        ),
      ]);
      for (const result of registrations)
        if (result.status === "rejected")
          console.error("Overlay listener registration failed", result.reason);
      const before = revision;
      const snapshot = await commands.overlaySnapshot();
      if (disposed) return;
      document.documentElement.style.setProperty(
        "--streaming-width",
        `${snapshot.streaming_width - 10}px`,
      );
      document.documentElement.style.setProperty(
        "--caption-height",
        `${snapshot.streaming_height - 44}px`,
      );
      if (!updated.skin) setSkin(snapshot.skin);
      if (!updated.position) setPosition(snapshot.position);
      if (!updated.theme)
        document.documentElement.dataset.theme = snapshot.theme.resolved;
      if (before === revision) apply(snapshot.presentation);
    };
    void initialize().catch(console.error);
    return () => {
      disposed = true;
      releases.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (
      !presentation.visible ||
      !presentation.ready ||
      presentation.state !== "streaming" ||
      working
    )
      return;
    const timer = setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => clearInterval(timer);
  }, [presentation.visible, presentation.ready, presentation.state, working]);

  useLayoutEffect(() => {
    const element = cap.current;
    if (!element) return;
    const update = () => {
      setOverflowing(element.scrollHeight > element.clientHeight + 1);
      if (pinned.current) element.scrollTop = element.scrollHeight;
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, [text, presentation.visible]);

  if (!presentation.visible) return null;
  const live = presentation.state === "streaming";
  const open = live && !!(text.committed || text.tentative);
  const busy =
    working ||
    presentation.state === "transcribing" ||
    presentation.state === "processing";
  const cancel = (
    <button
      className="cancel"
      aria-label="Cancel"
      onClick={() => void commands.overlayCancel()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4l8 8M12 4l-8 8"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );
  const workLabel =
    presentation.state === "processing" || kind === "polishing"
      ? t("overlay.processing")
      : t("overlay.transcribing");
  return (
    <div className={`stage ${position}`}>
      {presentation.notice ? (
        <div className="card notice" role="status">
          {presentation.notice}
        </div>
      ) : presentation.followup ? (
        <button
          className="card followup"
          onClick={() => void commands.overlayFollowup()}
        >
          {t("overlay.askFollowup")} <span>{presentation.followup}</span>
        </button>
      ) : (
        <div
          key={session}
          className={`card ${open ? "open" : "compact"} ${busy ? "working" : ""} ${skin}`}
        >
          {live && (
            <div className="text">
              <div className="text-clip">
                <div
                  ref={cap}
                  className={`caption ${overflowing ? "overflowing" : ""}`}
                  onScroll={() => {
                    const element = cap.current;
                    if (element)
                      pinned.current =
                        element.scrollHeight -
                          element.scrollTop -
                          element.clientHeight <=
                        16;
                  }}
                >
                  <p dir="auto">
                    {text.committed && <span>{text.committed} </span>}
                    <span>{text.tentative}</span>
                  </p>
                </div>
              </div>
            </div>
          )}
          <div className="base">
            <div className="identity" title={presentation.owner || undefined}>
              {busy ? (
                <span className="spinner" />
              ) : presentation.icon ? (
                <img src={presentation.icon} alt="" />
              ) : (
                <span
                  className={`dot ${presentation.ready ? "ready" : "arming"}`}
                />
              )}
            </div>
            {busy ? (
              <span className="work-label">{workLabel}</span>
            ) : (
              <Waveform
                ready={presentation.ready}
                session={session}
                skin={skin}
                live={open}
              />
            )}
            <div className="controls">
              {open && !busy && (
                <span className="timer">
                  {Math.floor(elapsed / 60)}:
                  {String(elapsed % 60).padStart(2, "0")}
                </span>
              )}
              {cancel}
            </div>
          </div>
          {presentation.owner && (
            <span className="owner">{presentation.owner}</span>
          )}
        </div>
      )}
    </div>
  );
}
