import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Replace, Sparkles, Zap } from "lucide-react";
import { SnippetsSection } from "./SnippetsSection";
import { ActionsSection } from "./ActionsSection";
import { ContextAwareSection } from "./ContextAwareSection";

type TabKey = "snippets" | "actions" | "context";

/** [GRAIN] Experimentations hub. Each experimental feature is isolated in its own
 * top-bar sub-tab so they never bleed into one another: Snippets (verbatim voice
 * expansions), Actions (spoken trigger → open apps/sites), and Context Aware
 * (adaptive post-processing + user modes). */
export const ExperimentationsSettings: React.FC = () => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<TabKey>("snippets");

  const tabs: { key: TabKey; label: string; icon: React.ReactNode }[] = [
    {
      key: "snippets",
      label: t("settings.experimentations.snippets.title"),
      icon: <Replace width={15} height={15} />,
    },
    {
      key: "actions",
      label: "Actions",
      icon: <Zap width={15} height={15} />,
    },
    {
      key: "context",
      label: "Context Aware",
      icon: <Sparkles width={15} height={15} />,
    },
  ];

  return (
    <div className="max-w-4xl w-full mx-auto space-y-5">
      {/* Top-bar sub-tabs. */}
      <div className="flex items-center gap-1 border-b border-[color-mix(in_srgb,var(--color-mid-gray)_25%,transparent)]">
        {tabs.map((tb) => {
          const active = tab === tb.key;
          return (
            <button
              key={tb.key}
              type="button"
              onClick={() => setTab(tb.key)}
              className={`flex items-center gap-1.5 px-3.5 py-2 text-sm -mb-px border-b-2 transition-colors cursor-pointer ${
                active
                  ? "border-[var(--color-accent)] text-ink font-medium"
                  : "border-transparent text-ink-soft hover:text-ink"
              }`}
            >
              {tb.icon}
              {tb.label}
            </button>
          );
        })}
      </div>

      {tab === "snippets" ? (
        <SnippetsSection />
      ) : tab === "actions" ? (
        <ActionsSection />
      ) : (
        <ContextAwareSection />
      )}
    </div>
  );
};
