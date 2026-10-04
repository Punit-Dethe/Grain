import { type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  commands,
  events,
  type OverlayPresentation,
  type StreamTextEvent,
} from "@/bindings";
import { Waveform } from "./Waveform";
import { useTranslation } from "react-i18next";
import i18n from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";

const empty: OverlayPresentation = {
  visible: false,
  state: "recording",
  ready: false,
  session_id: 0,
  agent: false,
  prompt_recording: false,
  app_icon_enabled: true,
  owner: null,
  icon: null,
  notice: null,
  followup: null,
  committed: "",
  tentative: "",
  working: false,
  work_kind: "transcribing",
};

export function RecordingOverlay() {
  const { t } = useTranslation();
  const [presentation, setPresentation] = useState(empty);
  const [position, setPosition] = useState("bottom");
  const [hideCompactClose, setHideCompactClose] = useState(false);
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
  const workRow = useRef<HTMLDivElement>(null);
  const workText = useRef<HTMLSpanElement>(null);
  const hasText = text.committed.length > 0 || text.tentative.length > 0;

  useEffect(() => {
    let disposed = false;
    let revision = 0;
    let acceptsLiveText = false;
    const updated = {
      position: false,
      theme: false,
      close: false,
    };
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
        acceptsLiveText = value.visible && value.state === "streaming";
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
            acceptsLiveText = payload === "streaming";
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
              prompt_recording:
                payload === "recording" || payload === "streaming"
                  ? false
                  : value.prompt_recording,
            }));
          }),
        ),
        register(
          events.hideOverlay.listen(() => {
            if (disposed) return;
            acceptsLiveText = false;
            revision++;
            setPresentation((value) => ({
              ...value,
              visible: false,
              ready: false,
              prompt_recording: false,
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
            if (!disposed && acceptsLiveText) {
              revision++;
              setText(payload);
            }
          }),
        ),
        register(
          events.streamPhaseEvent.listen(({ payload }) => {
            if (!disposed && acceptsLiveText) {
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
          events.grainOverlayPosition.listen(({ payload }) => {
            if (!disposed) {
              updated.position = true;
              setPosition(payload);
            }
          }),
        ),
        register(
          events.grainOverlayCompactCloseHidden.listen(({ payload }) => {
            if (!disposed) {
              updated.close = true;
              setHideCompactClose(payload);
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
      if (!updated.position) setPosition(snapshot.position);
      if (!updated.close) setHideCompactClose(snapshot.pill_hide_close_button);
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
      presentation.state !== "streaming"
    )
      return;
    const timer = setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => clearInterval(timer);
  }, [presentation.visible, presentation.ready, presentation.state]);

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
  }, [
    text,
    presentation.visible,
    presentation.notice,
    presentation.followup,
    session,
  ]);

  useLayoutEffect(() => {
    const row = workRow.current;
    const label = workText.current;
    const card = row?.closest<HTMLElement>(".scard");
    if (!row || !label || !card || card.classList.contains("open")) return;
    // Measure intrinsic text, independent of the card's animated width. Keep
    // the same explicit gap as recording, including translated working labels.
    const update = () => {
      const style = getComputedStyle(row);
      const right = row.querySelector<HTMLElement>(".sbase-r");
      const width =
        label.offsetWidth +
        (row.firstElementChild as HTMLElement).offsetWidth +
        (right?.offsetWidth ?? 0) +
        (right ? 2 : 1) * parseFloat(style.columnGap) +
        parseFloat(style.paddingLeft) +
        parseFloat(style.paddingRight);
      card.style.setProperty("--ov-work-w", `${width}px`);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(label);
    return () => observer.disconnect();
  }, [
    presentation.visible,
    presentation.state,
    presentation.notice,
    presentation.followup,
    working,
    kind,
    hasText,
    hideCompactClose,
    session,
    t,
  ]);

  if (!presentation.visible) return null;
  const showClose =
    !hideCompactClose || (presentation.state === "streaming" && hasText);
  const closeClass = showClose ? "" : "no-close";
  const agentClass = presentation.agent ? "agent" : "";
  const promptClass = presentation.prompt_recording ? "prompt-recording" : "";
  const direction = getLanguageDirection(i18n.language);
  const handleScroll = () => {
    const element = cap.current;
    if (element)
      pinned.current =
        element.scrollHeight - element.scrollTop - element.clientHeight <= 16;
  };
  const cancelBtn = (
    <button
      className="sx"
      aria-label="cancel"
      onClick={() => void commands.overlayCancel()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );
  const listeningRow = (showTimer: boolean) => (
    <div className="sbase">
      {(presentation.app_icon_enabled || showTimer) && (
        <div className="sbase-l" title={presentation.owner || undefined}>
          {presentation.app_icon_enabled && presentation.icon ? (
            <img src={presentation.icon} alt="" />
          ) : (
            <span
              className={`sdot ${presentation.ready ? "ready" : "arming"}`}
            />
          )}
        </div>
      )}
      <Waveform ready={presentation.ready} session={session} />
      {showClose && (
        <div className="sbase-r">
          {showTimer && (
            <span className="stimer">
              {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, "0")}
            </span>
          )}
          {cancelBtn}
        </div>
      )}
    </div>
  );
  const workingRow = (label: string) => (
    <div ref={workRow} className="sbase work-row">
      <div className="sbase-l">
        <span className="sspinner" />
      </div>
      <span className="swork-label">
        <span ref={workText}>{label}</span>
      </span>
      {showClose && <div className="sbase-r">{cancelBtn}</div>}
    </div>
  );

  if (presentation.notice || presentation.followup) {
    return (
      <div dir={direction} className={`ov-stage ${position}`}>
        {presentation.notice ? (
          <div className="grain-notice" role="status">
            {presentation.notice}
          </div>
        ) : (
          <button
            className="grain-followup"
            onClick={() => void commands.overlayFollowup()}
          >
            {t("overlay.askFollowup")} <span>{presentation.followup}</span>
          </button>
        )}
      </div>
    );
  }

  // Retain Handy-derived Live/Minimal states; Grain owns listening controls,
  // optional icon/close slots and capture tint.
  if (presentation.state === "streaming") {
    const open = hasText;
    const collapsed = working && !open;
    const iconClass =
      !presentation.app_icon_enabled && !open && !working ? "no-icon" : "";
    return (
      <div dir={direction} className={`ov-stage ${position}`}>
        <div
          key={session}
          className={`scard ${agentClass} ${promptClass} ${closeClass} ${iconClass} ${open ? "open" : ""} ${collapsed ? "working" : ""}`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                ref={cap}
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                onScroll={handleScroll}
              >
                <p>
                  <span className="committed">
                    {text.committed ? text.committed + " " : ""}
                  </span>
                  <span className="tentative">{text.tentative}</span>
                  {!working && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                kind === "polishing"
                  ? t("overlay.processing")
                  : t("overlay.transcribing"),
              )
            : listeningRow(open)}
        </div>
      </div>
    );
  }

  const busy =
    presentation.state === "transcribing" ||
    presentation.state === "processing";
  const workLabel =
    presentation.state === "processing"
      ? t("overlay.processing")
      : t("overlay.transcribing");
  const iconClass = !presentation.app_icon_enabled && !busy ? "no-icon" : "";
  return (
    <div dir={direction} className={`ov-stage ${position} ov-fade show`}>
      <div
        className={`scard compact ${agentClass} ${promptClass} ${closeClass} ${iconClass} ${busy ? "cworking" : ""}`}
      >
        {busy ? workingRow(workLabel) : listeningRow(false)}
      </div>
    </div>
  );
}
