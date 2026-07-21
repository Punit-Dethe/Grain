import React, { useState } from "react";
import { Bot, LayoutGrid, Replace, Sparkles } from "lucide-react";
import { OverviewSection } from "./OverviewSection";
import { SnippetsSection } from "./SnippetsSection";
import { ActionsSection } from "./ActionsSection";
import { ContextAwareSection } from "./ContextAwareSection";
import { AgentSection } from "./AgentSection";

type TabKey = "overview" | "snippets" | "context" | "agent";

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
    key: "overview",
    label: "Overview",
    icon: <LayoutGrid width={15} height={15} />,
  },
  {
    key: "snippets",
    label: "Snippets",
    icon: <Replace width={15} height={15} />,
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

/** Where each extension id's settings live (SPEC §5.1: clicking a name in
 * Overview jumps to its settings). Pack-tier ids without a tab of their own
 * land on the most related core tab. */
const JUMP_TARGETS: Record<string, TabKey> = {
  "grain.snippets": "snippets",
  "grain.context-awareness": "context",
  "grain.agent": "agent",
  "grain.agent-center-layout": "agent",
};

/** [GRAIN] Extensions hub (SPEC §5). The FIRST tab is Overview — every
 * installed extension with its toggle; the tab bar itself never grows with
 * extension count (remaining tabs are core feature groups only). Snippets and
 * Actions are ONE concept and share a tab (SPEC §5.4): Actions renders below
 * Snippets, exactly where its extension successor will anchor
 * (`snippets.after`), so the UI never moves twice. */
export const ExperimentationsSettings: React.FC = () => {
  const [tab, setTab] = useState<TabKey>("overview");

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
                  ? "bg-paper-raised text-ink shadow-[0_1px_2px_rgba(0,0,0,0.14)] border border-accent/30"
                  : "border border-transparent text-ink-soft hover:text-ink hover:bg-[rgba(20,19,18,0.04)]"
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

      {tab === "overview" ? (
        <OverviewSection
          onJump={(id) => setTab(JUMP_TARGETS[id] ?? "overview")}
        />
      ) : tab === "snippets" ? (
        <div className="space-y-8">
          <SnippetsSection />
          {/* SPEC §5.4: one thin divider, then Actions — the position its
              extension form will occupy via anchor "snippets.after". */}
          <div className="border-t border-line" />
          <ActionsSection />
        </div>
      ) : tab === "context" ? (
        <ContextAwareSection />
      ) : (
        <AgentSection />
      )}
    </div>
  );
};
