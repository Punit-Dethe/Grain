import React, { useState } from "react";
import { Bot, Replace, Sparkles, Zap } from "lucide-react";
import { SnippetsSection } from "./SnippetsSection";
import { ActionsSection } from "./ActionsSection";
import { ContextAwareSection } from "./ContextAwareSection";
import { AgentSection } from "./AgentSection";

type TabKey = "snippets" | "actions" | "context" | "agent";

// User-facing section name is "Extensions" (the internal folder/route keeps the
// legacy "experimentations" id). Constant so the i18n lint treats brand chrome
// as an identifier, not translatable copy.
const SECTION_TITLE = "Extensions";

const TABS: {
  key: TabKey;
  label: string;
  icon: React.ReactNode;
}[] = [
  {
    key: "snippets",
    label: "Snippets",
    icon: <Replace width={15} height={15} />,
  },
  {
    key: "actions",
    label: "Actions",
    icon: <Zap width={15} height={15} />,
  },
  {
    key: "context",
    label: "Context",
    icon: <Sparkles width={15} height={15} />,
  },
  {
    key: "agent",
    label: "Agent",
    icon: <Bot width={15} height={15} />,
  },
];

/** [GRAIN] Extensions hub. Each extension is isolated in its own segmented
 * sub-tab so their (growing) content never bleeds together: Snippets (verbatim
 * voice expansions), Actions (spoken trigger → open apps/sites), Context
 * (adaptive post-processing + user modes), and Agent (reply handling, Quick
 * Agent, the follow-up shortcut, and the Agent's own context awareness). */
export const ExperimentationsSettings: React.FC = () => {
  const [tab, setTab] = useState<TabKey>("snippets");

  return (
    <div className="max-w-4xl w-full mx-auto space-y-6">
      {/* Page title — set larger than the other consoles; the tabs sit directly
          beneath it, no subtitle, so the section opens clean. */}
      <h1 className="px-1 text-[1.7rem] font-semibold tracking-tight leading-none">
        {SECTION_TITLE}
      </h1>

      {/* Segmented sub-tab selector — a recessed track of instrument buttons;
          the active one fills solid (ink) with an accent icon. */}
      <div
        role="tablist"
        aria-label={SECTION_TITLE}
        className="flex items-center gap-1 p-1 rounded-xl bg-paper-sunken border border-line"
      >
        {TABS.map((tb) => {
          const isActive = tab === tb.key;
          return (
            <button
              key={tb.key}
              type="button"
              role="tab"
              aria-selected={isActive}
              onClick={() => setTab(tb.key)}
              className={`group relative flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-sm transition-all duration-150 cursor-pointer ${
                isActive
                  ? "bg-[var(--color-ink)] text-[var(--color-paper)] shadow-[0_1px_3px_rgba(0,0,0,0.22)]"
                  : "text-ink-soft hover:text-ink hover:bg-[rgba(20,19,18,0.04)]"
              }`}
            >
              <span
                className={`transition-colors ${
                  isActive
                    ? "text-accent"
                    : "text-ink-faint group-hover:text-ink-soft"
                }`}
              >
                {tb.icon}
              </span>
              <span className="font-medium tracking-[0.01em]">{tb.label}</span>
            </button>
          );
        })}
      </div>

      {tab === "snippets" ? (
        <SnippetsSection />
      ) : tab === "actions" ? (
        <ActionsSection />
      ) : tab === "context" ? (
        <ContextAwareSection />
      ) : (
        <AgentSection />
      )}
    </div>
  );
};
