import { ArrowRight } from "lucide-react";
import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { formatKeyPart } from "@/lib/utils/keyboard";
import { hashForRoute } from "../navigation";

const SHORTCUTS = [
  { mode: "standard", bindingId: "transcribe" },
  { mode: "streaming", bindingId: "transcribe_native_asr" },
  { mode: "agent", bindingId: "summon_agent" },
] as const;

const CAPTURE_SETTINGS = hashForRoute({ page: "settings", section: "capture" });

function go(hash: string) {
  window.location.hash = hash.slice(1);
}

function ShortcutKeys({ combination }: { combination: string }) {
  const keys = combination.split("+").map(formatKeyPart).filter(Boolean);
  // Budget glyphs, key padding and separators in ems so the entire chord
  // scales to its available width without measuring or wrapping individual keys.
  const emWidth =
    keys.reduce(
      (width, key) => width + Math.max(3.84, key.length * 0.66 + 1.92),
      0,
    ) +
    Math.max(0, keys.length - 1) * 1.5;
  // Shared kbd styles belong to individual keys, never to the entire chord.
  return (
    <span
      className="overview-shortcut-keys"
      data-adaptive={keys.length > 2 ? "true" : undefined}
      style={
        {
          "--shortcut-em-width": Math.max(1, emWidth),
          "--shortcut-border-width": `${keys.length * 2}px`,
        } as CSSProperties
      }
    >
      {keys.map((key, index) => (
        <span className="overview-shortcut-key-part" key={`${key}-${index}`}>
          {index > 0 && <span className="overview-shortcut-plus">+</span>}
          <kbd className="overview-shortcut-key">{key}</kbd>
        </span>
      ))}
    </span>
  );
}

export function OverviewCards() {
  const { t } = useTranslation();
  const { getSetting, isLoading } = useSettings();
  const bindings = getSetting("bindings") ?? {};
  const agentEnabled = getSetting("agent_enabled") ?? false;

  return (
    <div className="overview-shortcut-strip">
      <div className="overview-shortcut-layout">
        <aside className="overview-shortcut-info">
          <div>
            <strong>{t("overview.shortcuts.infoTitle")}</strong>
            <p>{t("overview.shortcuts.infoDescription")}</p>
          </div>
          <button
            className="button overview-shortcut-settings"
            type="button"
            onClick={() => go(CAPTURE_SETTINGS)}
          >
            {t("overview.shortcuts.infoAction")}
            <ArrowRight size={13} aria-hidden="true" />
          </button>
        </aside>
        <div className="overview-shortcuts">
          {SHORTCUTS.map(({ mode, bindingId }) => {
            const label = t(`overview.shortcuts.${mode}`);
            const combination = bindings[bindingId]?.current_binding?.trim();
            const shortcut = combination
              ? combination
                  .split("+")
                  .map(formatKeyPart)
                  .filter(Boolean)
                  .join(" + ")
              : t("overview.shortcuts.notSet");
            return (
              <section className="overview-shortcut" key={mode}>
                <div className="overview-shortcut-heading">
                  <h3>{label}</h3>
                  {mode === "agent" && !isLoading && !agentEnabled && (
                    <span className="overview-shortcut-status">
                      {t("overview.shortcuts.off")}
                    </span>
                  )}
                </div>
                <button
                  className="overview-shortcut-trigger"
                  type="button"
                  disabled={isLoading}
                  aria-label={t("overview.shortcuts.configure", {
                    mode: label,
                    shortcut,
                  })}
                  onClick={() =>
                    go(
                      mode === "agent"
                        ? hashForRoute({ page: "agent" })
                        : CAPTURE_SETTINGS,
                    )
                  }
                >
                  {isLoading ? (
                    <span className="overview-shortcut-empty">
                      {t("settings.general.shortcut.loading")}
                    </span>
                  ) : combination ? (
                    <ShortcutKeys combination={combination} />
                  ) : (
                    <span className="overview-shortcut-empty">
                      {t("overview.shortcuts.notSet")}
                    </span>
                  )}
                </button>
              </section>
            );
          })}
        </div>
      </div>
    </div>
  );
}
