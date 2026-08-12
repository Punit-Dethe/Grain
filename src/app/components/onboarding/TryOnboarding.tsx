import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronLeft, Loader2, Mic, Square } from "lucide-react";
import { commands, events, type OnboardingTestMode } from "@/bindings";
import "./onboarding.css";

interface TryOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
}

type TestStatus =
  | "idle"
  | "starting"
  | "recording"
  | "processing"
  | "result"
  | "error";

const TEST_MODES: OnboardingTestMode[] = ["flow", "standard", "streaming"];

const getErrorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

const TryOnboarding: React.FC<TryOnboardingProps> = ({
  onBack,
  onComplete,
}) => {
  const { t } = useTranslation();
  const [mode, setMode] = useState<OnboardingTestMode>("flow");
  const [status, setStatus] = useState<TestStatus>("idle");
  const [output, setOutput] = useState("");
  const [error, setError] = useState("");
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopTimer = useCallback(() => {
    if (timerRef.current) clearInterval(timerRef.current);
    timerRef.current = null;
  }, []);

  useEffect(() => {
    const unlistenPromise = events.streamTextEvent.listen((event) => {
      if (mode !== "streaming") return;
      setOutput(`${event.payload.committed}${event.payload.tentative}`.trim());
    });

    return () => {
      stopTimer();
      void unlistenPromise
        .then((unlisten) => unlisten())
        .catch(() => undefined);
      void commands.cancelOnboardingTranscriptionTest();
    };
  }, [mode, stopTimer]);

  const chooseMode = async (nextMode: OnboardingTestMode) => {
    if (status === "starting" || status === "processing") return;
    if (status === "recording") {
      const result = await commands.cancelOnboardingTranscriptionTest();
      if (result.status === "error") {
        setError(result.error);
        setStatus("error");
        return;
      }
      stopTimer();
    }
    setMode(nextMode);
    setStatus("idle");
    setOutput("");
    setError("");
    setElapsedSeconds(0);
  };

  const startTest = async () => {
    if (status === "starting" || status === "processing") return;
    setStatus("starting");
    setOutput("");
    setError("");
    setElapsedSeconds(0);
    try {
      const result = await commands.startOnboardingTranscriptionTest(mode);
      if (result.status === "error") {
        setError(result.error);
        setStatus("error");
        return;
      }

      setStatus("recording");
      const startedAt = Date.now();
      timerRef.current = setInterval(() => {
        setElapsedSeconds(Math.floor((Date.now() - startedAt) / 1000));
      }, 250);
    } catch (startError) {
      setError(getErrorMessage(startError));
      setStatus("error");
    }
  };

  const stopTest = async () => {
    stopTimer();
    setStatus("processing");
    try {
      const result = await commands.stopOnboardingTranscriptionTest();
      if (result.status === "error") {
        setError(result.error);
        setStatus("error");
        return;
      }
      setOutput(result.data);
      setStatus("result");
    } catch (stopError) {
      setError(getErrorMessage(stopError));
      setStatus("error");
    }
  };

  const elapsedLabel = `${Math.floor(elapsedSeconds / 60)
    .toString()
    .padStart(2, "0")}:${(elapsedSeconds % 60).toString().padStart(2, "0")}`;

  const statusLabel =
    status === "starting"
      ? t("onboarding.setup.try.opening")
      : status === "recording"
        ? t("onboarding.setup.try.listening")
        : status === "processing"
          ? t("onboarding.setup.try.processing")
          : status === "result"
            ? t("onboarding.setup.try.complete")
            : status === "error"
              ? t("onboarding.setup.try.failed")
              : t("onboarding.setup.try.ready");

  return (
    <div className="onboarding-shell">
      <header className="onboarding-topbar">
        <div
          className="onboarding-brand"
          aria-label={t("onboarding.setup.brand")}
        >
          <strong>{t("onboarding.setup.brand")}</strong>
          <span>{t("onboarding.setup.label")}</span>
        </div>

        <ol
          className="onboarding-stepper"
          aria-label={t("onboarding.setup.progress")}
        >
          {(["microphone", "modes", "models", "try", "shortcuts"] as const).map(
            (step, index) => (
              <li
                key={step}
                className={
                  index === 3 ? "active" : index < 3 ? "done" : undefined
                }
                aria-current={index === 3 ? "step" : undefined}
              >
                <span className="onboarding-stepper-line" aria-hidden="true" />
                <span>{t(`onboarding.setup.steps.${step}`)}</span>
              </li>
            ),
          )}
        </ol>

        <button
          type="button"
          className="onboarding-skip"
          disabled={status === "starting" || status === "processing"}
          onClick={onComplete}
        >
          {t("onboarding.setup.try.skip")}
        </button>
      </header>

      <main className="onboarding-stage">
        <section className="onboarding-try-step">
          <div className="onboarding-heading">
            <h1>{t("onboarding.setup.try.title")}</h1>
            <p>{t("onboarding.setup.try.description")}</p>
          </div>

          <div
            className="onboarding-test-tabs"
            role="tablist"
            aria-label={t("onboarding.setup.try.modeLabel")}
          >
            {TEST_MODES.map((item) => (
              <button
                key={item}
                type="button"
                role="tab"
                aria-selected={item === mode}
                className={item === mode ? "active" : undefined}
                disabled={status === "starting" || status === "processing"}
                onClick={() => void chooseMode(item)}
              >
                {t(`onboarding.setup.try.modes.${item}.title`)}
              </button>
            ))}
          </div>

          <div className={`onboarding-transcription-test ${status}`}>
            <div className="onboarding-test-status">
              <span>
                <i aria-hidden="true" />
                {statusLabel}
              </span>
              <strong>
                {status === "recording"
                  ? elapsedLabel
                  : t(`onboarding.setup.try.modes.${mode}.speed`)}
              </strong>
            </div>

            <div
              className={`onboarding-test-output${!output && !error ? " placeholder" : ""}${error ? " error" : ""}`}
              aria-live="polite"
            >
              {error || output || t("onboarding.setup.try.placeholder")}
              {mode === "streaming" && status === "recording" ? (
                <i className="onboarding-demo-cursor" aria-hidden="true" />
              ) : null}
            </div>

            <div className="onboarding-test-controls">
              <button
                type="button"
                className={status === "recording" ? "recording" : undefined}
                disabled={status === "starting" || status === "processing"}
                onClick={status === "recording" ? stopTest : startTest}
              >
                {status === "starting" || status === "processing" ? (
                  <Loader2 className="spin" aria-hidden="true" />
                ) : status === "recording" ? (
                  <Square aria-hidden="true" />
                ) : status === "result" ? (
                  <Check aria-hidden="true" />
                ) : (
                  <Mic aria-hidden="true" />
                )}
                {status === "recording"
                  ? t("onboarding.setup.try.stop")
                  : status === "result"
                    ? t("onboarding.setup.try.again")
                    : t("onboarding.setup.try.start")}
              </button>
              <span>{t(`onboarding.setup.try.modes.${mode}.hint`)}</span>
            </div>
          </div>
        </section>
      </main>

      <footer className="onboarding-footer">
        <div className="onboarding-footer-inner">
          <button
            type="button"
            className="onboarding-back"
            disabled={status === "starting" || status === "processing"}
            onClick={onBack}
          >
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.try.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            disabled={
              status === "starting" ||
              status === "recording" ||
              status === "processing"
            }
            onClick={onComplete}
          >
            {t("onboarding.setup.try.continue")}
          </button>
        </div>
      </footer>
    </div>
  );
};

export default TryOnboarding;
