import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

const workingPhraseKeys = [
  "agent.workingPhrases.acknowledged",
  "agent.workingPhrases.working",
  "agent.workingPhrases.thinking",
  "agent.workingPhrases.continuing",
] as const;

/** Lives only inside a visible working pill; no timer survives that surface. */
export function AgentWorkingLabel() {
  const { t } = useTranslation();
  const [phrase, setPhrase] = useState(0);
  useEffect(() => {
    const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
    let timer: ReturnType<typeof setInterval> | undefined;
    const update = () => {
      clearInterval(timer);
      timer = undefined;
      if (!document.hidden && !reducedMotion.matches) {
        timer = setInterval(
          () => setPhrase((value) => (value + 1) % workingPhraseKeys.length),
          3800,
        );
      }
    };
    update();
    document.addEventListener("visibilitychange", update);
    reducedMotion.addEventListener("change", update);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", update);
      reducedMotion.removeEventListener("change", update);
    };
  }, []);
  return (
    <>
      <span className="agent-working-status" role="status">
        {t("agent.thinking")}
      </span>
      <span className="agent-working-phrases" aria-hidden="true">
        {workingPhraseKeys.map((key, index) => (
          <span key={key} className={index === phrase ? "active" : ""}>
            {t(key)}
          </span>
        ))}
      </span>
    </>
  );
}
