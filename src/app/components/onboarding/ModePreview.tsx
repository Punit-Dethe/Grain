import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Mic, Play, RotateCcw } from "lucide-react";
import type { OnboardingTestMode } from "@/bindings";
import { emptyPreview, modePreviewFrames } from "./previewSequence";

const BARS = [
  10, 18, 27, 16, 32, 22, 14, 29, 20, 34, 16, 24, 12, 30, 18, 26, 14, 33, 21,
  13, 28, 17, 24, 12,
];

export function ModePreview({ mode }: { mode: OnboardingTestMode }) {
  const { t } = useTranslation();
  const phrase = t("onboarding.setup.modes.preview.phrase");
  const [frame, setFrame] = useState(emptyPreview);
  const [reducedMotion, setReducedMotion] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [manualIndex, setManualIndex] = useState(-1);
  const timersRef = useRef<ReturnType<typeof setTimeout>[]>([]);
  const clearTimers = useCallback(() => {
    timersRef.current.forEach(clearTimeout);
    timersRef.current = [];
  }, []);
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReducedMotion(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useEffect(() => {
    clearTimers();
    setFrame(emptyPreview());
    setManualIndex(-1);
    return clearTimers;
  }, [clearTimers, mode, phrase, reducedMotion]);

  const play = () => {
    clearTimers();
    const frames = modePreviewFrames(mode, phrase, reducedMotion);
    if (reducedMotion) {
      const next =
        manualIndex < 0 || manualIndex === frames.length - 1
          ? 0
          : manualIndex + 1;
      setManualIndex(next);
      setFrame(frames[next]);
      return;
    }
    setFrame(frames[0]);
    timersRef.current = frames.slice(1).map((next) =>
      setTimeout(() => {
        setFrame(next);
        if (next.phase === "complete") timersRef.current = [];
      }, next.at),
    );
  };
  const playing = frame.phase === "recording" || frame.phase === "finishing";
  const label =
    frame.phase === "complete"
      ? "replay"
      : reducedMotion && playing
        ? "next"
        : "play";
  const statusKey =
    frame.phase === "ready"
      ? "ready"
      : frame.phase === "complete"
        ? "complete"
        : `${mode}.${frame.phase}`;

  return (
    <div className="onboarding-mode-preview" data-phase={frame.phase}>
      <div className="onboarding-preview-toolbar">
        <span>{t("onboarding.setup.modes.preview.label")}</span>
        <button
          type="button"
          className="onboarding-preview-play"
          onClick={play}
          disabled={!reducedMotion && playing}
        >
          {frame.phase === "complete" ? (
            <RotateCcw aria-hidden="true" />
          ) : (
            <Play aria-hidden="true" />
          )}
          {t(`onboarding.setup.modes.preview.${label}`)}
        </button>
      </div>
      <div className="onboarding-preview-audio" aria-hidden="true">
        <span className="onboarding-preview-mic">
          {frame.phase === "complete" ? <Check /> : <Mic />}
        </span>
        <div className="onboarding-preview-lanes">
          <div className="onboarding-preview-wave">
            {BARS.map((height, index) => (
              <i
                key={index}
                className={
                  frame.captured > index / BARS.length ? "captured" : undefined
                }
                style={{ height }}
              />
            ))}
          </div>
          <div className="onboarding-preview-work">
            <i style={{ transform: `scaleX(${frame.processed})` }} />
          </div>
        </div>
      </div>
      <p className="onboarding-preview-transcript" aria-live="off">
        {frame.transcript || (
          <span>
            {t(
              frame.phase === "ready"
                ? "onboarding.setup.modes.preview.placeholder"
                : frame.phase === "finishing"
                  ? "onboarding.setup.modes.preview.finishingText"
                  : "onboarding.setup.modes.preview.waitingText",
            )}
          </span>
        )}
      </p>
      <p className="onboarding-preview-status" role="status">
        {t(`onboarding.setup.modes.preview.${statusKey}`)}
        {frame.phase === "complete" && (
          <span className="sr-only">{phrase}</span>
        )}
      </p>
    </div>
  );
}
