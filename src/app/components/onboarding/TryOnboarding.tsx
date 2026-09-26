import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronLeft, Loader2, Mic, Square } from "lucide-react";
import { commands, events, type OnboardingTestMode } from "@/bindings";
import { OnboardingLayout } from "./OnboardingLayout";

interface TryOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
  availableModes: OnboardingTestMode[];
}

type TestStatus =
  "idle" | "starting" | "recording" | "processing" | "result" | "error";

const getErrorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

const TryOnboarding: React.FC<TryOnboardingProps> = ({
  onBack,
  onComplete,
  availableModes,
}) => {
  const { t } = useTranslation();
  const [mode, setMode] = useState<OnboardingTestMode>(
    availableModes[0] ?? "standard",
  );
  const [status, setStatus] = useState<TestStatus>("idle");
  const [output, setOutput] = useState("");
  const [error, setError] = useState("");
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const generationRef = useRef(0);
  const mountedRef = useRef(true);
  const navigatingRef = useRef(false);

  const stopTimer = useCallback(() => {
    if (timerRef.current) clearInterval(timerRef.current);
    timerRef.current = null;
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    const unlistenPromise = events.streamTextEvent.listen((event) => {
      if (!mountedRef.current || mode !== "streaming") return;
      setOutput(`${event.payload.committed}${event.payload.tentative}`.trim());
    });

    return () => {
      mountedRef.current = false;
      generationRef.current += 1;
      stopTimer();
      void unlistenPromise
        .then((unlisten) => unlisten())
        .catch(() => undefined);
      void commands.cancelOnboardingTranscriptionTest().catch(() => undefined);
    };
  }, [mode, stopTimer]);

  const chooseMode = async (nextMode: OnboardingTestMode) => {
    if (
      nextMode === mode ||
      navigatingRef.current ||
      status === "starting" ||
      status === "processing"
    )
      return;
    if (status === "recording") {
      setStatus("processing");
      generationRef.current += 1;
      stopTimer();
      try {
        const result = await commands.cancelOnboardingTranscriptionTest();
        if (!mountedRef.current) return;
        if (result.status === "error") throw new Error(result.error);
      } catch (cancelError) {
        if (mountedRef.current) {
          setError(getErrorMessage(cancelError));
          setStatus("error");
        }
        return;
      }
    }
    setMode(nextMode);
    setStatus("idle");
    setOutput("");
    setError("");
    setElapsedSeconds(0);
  };

  const startTest = async () => {
    if (
      status === "starting" ||
      status === "processing" ||
      !availableModes.includes(mode)
    )
      return;
    const generation = ++generationRef.current;
    setStatus("starting");
    setOutput("");
    setError("");
    setElapsedSeconds(0);
    try {
      const result = await commands.startOnboardingTranscriptionTest(mode);
      if (!mountedRef.current || generation !== generationRef.current) {
        await commands.cancelOnboardingTranscriptionTest();
        return;
      }
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
      if (!mountedRef.current || generation !== generationRef.current) return;
      setError(getErrorMessage(startError));
      setStatus("error");
    }
  };

  const stopTest = async () => {
    const generation = generationRef.current;
    stopTimer();
    setStatus("processing");
    try {
      const result = await commands.stopOnboardingTranscriptionTest();
      if (!mountedRef.current || generation !== generationRef.current) return;
      if (result.status === "error") {
        setError(result.error);
        setStatus("error");
        return;
      }
      if (!result.data.trim()) {
        setError(t("onboarding.setup.try.emptyResult"));
        setStatus("error");
      } else {
        setOutput(result.data);
        setStatus("result");
      }
    } catch (stopError) {
      if (!mountedRef.current || generation !== generationRef.current) return;
      setError(getErrorMessage(stopError));
      setStatus("error");
    }
  };

  const leaveTest = async (next: () => void) => {
    if (
      navigatingRef.current ||
      status === "starting" ||
      status === "processing"
    )
      return;
    navigatingRef.current = true;
    generationRef.current += 1;
    stopTimer();
    setStatus("processing");
    try {
      const result = await commands.cancelOnboardingTranscriptionTest();
      if (!mountedRef.current) return;
      if (result.status === "error") throw new Error(result.error);
      next();
    } catch (cancelError) {
      if (mountedRef.current) {
        setError(getErrorMessage(cancelError));
        setStatus("error");
      }
    } finally {
      navigatingRef.current = false;
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
    <OnboardingLayout
      step={3}
      topAction={
        <button
          type="button"
          className="onboarding-skip"
          disabled={status === "starting" || status === "processing"}
          onClick={() => void leaveTest(onComplete)}
        >
          {t("onboarding.setup.try.skip")}
        </button>
      }
      footer={
        <>
          <button
            type="button"
            className="onboarding-back"
            disabled={status === "starting" || status === "processing"}
            onClick={() => void leaveTest(onBack)}
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
            onClick={() => void leaveTest(onComplete)}
          >
            {t("onboarding.setup.try.continue")}
          </button>
        </>
      }
    >
      <section className="onboarding-try-step">
        <div className="onboarding-heading">
          <h1>{t("onboarding.setup.try.title")}</h1>
          <p>{t("onboarding.setup.try.description")}</p>
        </div>

        <div
          className="onboarding-test-tabs"
          role="group"
          aria-label={t("onboarding.setup.try.modeLabel")}
        >
          {availableModes.map((item) => (
            <button
              key={item}
              type="button"

              aria-pressed={item === mode}
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
    </OnboardingLayout>
  );
};

export default TryOnboarding;
