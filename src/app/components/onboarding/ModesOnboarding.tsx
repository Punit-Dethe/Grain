import { useEffect, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { ChevronLeft, Mic } from "lucide-react";
import "./onboarding.css";

interface ModesOnboardingProps {
  onBack: () => void;
  onComplete: () => void;
}

type ModeId = "standard" | "flow" | "streaming";
type DemoPhase = "recording" | "processing" | "result";

const MODES: ModeId[] = ["standard", "flow", "streaming"];
const WAVE_BARS = Array.from({ length: 22 }, (_, index) => index);
const STREAMING_WORDS = [
  "this",
  "is",
  "what",
  "streaming",
  "looks",
  "like",
  "while",
  "you",
  "are",
  "still",
  "speaking",
];

const ModesOnboarding: React.FC<ModesOnboardingProps> = ({
  onBack,
  onComplete,
}) => {
  const { t } = useTranslation();
  const [modeIndex, setModeIndex] = useState(0);
  const [phase, setPhase] = useState<DemoPhase>("recording");
  const [streamingWordCount, setStreamingWordCount] = useState(0);
  const [demoCycle, setDemoCycle] = useState(0);

  const mode = MODES[modeIndex];

  useEffect(() => {
    let active = true;
    let interval: ReturnType<typeof setInterval> | null = null;
    const timeouts: ReturnType<typeof setTimeout>[] = [];
    const schedule = (callback: () => void, delay: number) => {
      timeouts.push(
        setTimeout(() => {
          if (active) callback();
        }, delay),
      );
    };

    setPhase("recording");
    setStreamingWordCount(0);

    if (mode === "streaming") {
      let visibleWords = 0;
      interval = setInterval(() => {
        if (!active) return;
        visibleWords += 1;
        setStreamingWordCount(Math.min(visibleWords, STREAMING_WORDS.length));
        if (visibleWords < STREAMING_WORDS.length) return;
        if (interval) clearInterval(interval);
        interval = null;
        setPhase("result");
        schedule(() => setDemoCycle((current) => current + 1), 1900);
      }, 185);
    } else {
      schedule(() => setPhase("processing"), 1250);
      schedule(() => setPhase("result"), mode === "standard" ? 3000 : 1750);
      schedule(
        () => setDemoCycle((current) => current + 1),
        mode === "standard" ? 5600 : 4250,
      );
    }

    return () => {
      active = false;
      if (interval) clearInterval(interval);
      timeouts.forEach(clearTimeout);
    };
  }, [demoCycle, mode]);

  const moveForward = () => {
    if (modeIndex < MODES.length - 1) {
      setModeIndex((current) => current + 1);
      return;
    }
    onComplete();
  };

  const transcript =
    mode === "streaming"
      ? STREAMING_WORDS.slice(0, streamingWordCount).join(" ")
      : phase === "result"
        ? t(`onboarding.setup.modes.${mode}.result`)
        : "";

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
                  index === 1 ? "active" : index < 1 ? "done" : undefined
                }
                aria-current={index === 1 ? "step" : undefined}
              >
                <span className="onboarding-stepper-line" aria-hidden="true" />
                <span>{t(`onboarding.setup.steps.${step}`)}</span>
              </li>
            ),
          )}
        </ol>

        <button type="button" className="onboarding-skip" onClick={onComplete}>
          {t("onboarding.setup.modes.skip")}
        </button>
      </header>

      <main className="onboarding-stage">
        <section className="onboarding-modes-step">
          <div className="onboarding-heading">
            <h1>{t("onboarding.setup.modes.title")}</h1>
            <p>{t("onboarding.setup.modes.description")}</p>
          </div>

          <div className="onboarding-mode-tour">
            <div className="onboarding-mode-progress">
              <div className="onboarding-mode-dots">
                {MODES.map((item, index) => (
                  <button
                    key={item}
                    type="button"
                    className={index === modeIndex ? "active" : undefined}
                    aria-label={t("onboarding.setup.modes.showMode", {
                      mode: t(`onboarding.setup.modes.${item}.title`),
                    })}
                    aria-current={index === modeIndex ? "step" : undefined}
                    onClick={() => setModeIndex(index)}
                  />
                ))}
              </div>
              <span>
                {modeIndex + 1} / {MODES.length}
              </span>
            </div>

            <div className="onboarding-mode-showcase" data-mode={mode}>
              <div className="onboarding-mode-head">
                <div>
                  <strong>{t(`onboarding.setup.modes.${mode}.title`)}</strong>
                  <span>{t(`onboarding.setup.modes.${mode}.subtitle`)}</span>
                </div>
                <div className="onboarding-mode-traits">
                  <span>{t(`onboarding.setup.modes.${mode}.traitOne`)}</span>
                  <span>{t(`onboarding.setup.modes.${mode}.traitTwo`)}</span>
                </div>
              </div>

              <div className="onboarding-mode-demo">
                <div className="onboarding-demo-capture">
                  <div className={`onboarding-demo-pill ${phase}`}>
                    <span className="onboarding-demo-mic">
                      <Mic aria-hidden="true" />
                    </span>
                    <span className="onboarding-demo-wave" aria-hidden="true">
                      {WAVE_BARS.map((bar) => (
                        <i
                          key={bar}
                          style={{ "--wave-index": bar } as CSSProperties}
                        />
                      ))}
                    </span>
                    <span className="onboarding-demo-time">
                      {mode === "standard"
                        ? "03:00"
                        : mode === "flow"
                          ? "10:00"
                          : `00:0${Math.min(8, 3 + Math.floor(streamingWordCount / 3))}`}
                    </span>
                  </div>
                </div>

                <div className="onboarding-demo-result" aria-live="polite">
                  <span className="onboarding-demo-label">
                    {t(`onboarding.setup.modes.${mode}.label`)}
                  </span>
                  {phase === "processing" ? (
                    <div className="onboarding-demo-processing">
                      <i aria-hidden="true" />
                      <span>
                        {t(`onboarding.setup.modes.${mode}.processing`)}
                      </span>
                    </div>
                  ) : (
                    <div className="onboarding-demo-transcript">
                      {transcript || (
                        <span className="ghost">
                          {t("onboarding.setup.modes.recording")}
                        </span>
                      )}
                      {mode === "streaming" && phase !== "result" ? (
                        <i
                          className="onboarding-demo-cursor"
                          aria-hidden="true"
                        />
                      ) : null}
                    </div>
                  )}
                  <p>{t(`onboarding.setup.modes.${mode}.footnote`)}</p>
                </div>
              </div>
            </div>
          </div>
        </section>
      </main>

      <footer className="onboarding-footer">
        <div className="onboarding-footer-inner">
          <button type="button" className="onboarding-back" onClick={onBack}>
            <ChevronLeft aria-hidden="true" />
            {t("onboarding.setup.modes.back")}
          </button>
          <button
            type="button"
            className="onboarding-primary"
            onClick={moveForward}
          >
            {modeIndex < MODES.length - 1
              ? t("onboarding.setup.modes.next")
              : t("onboarding.setup.modes.chooseModels")}
          </button>
        </div>
      </footer>
    </div>
  );
};

export default ModesOnboarding;
