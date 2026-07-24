import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  Code2,
  LayoutGrid,
  Replace,
  Sparkles,
  Upload,
} from "lucide-react";
import { OverviewSection } from "./OverviewSection";
import { SnippetsSection } from "./SnippetsSection";
import { ContextAwareSection } from "./ContextAwareSection";
import { AgentSection } from "./AgentSection";
import { DeveloperSection } from "./DeveloperSection";
import { ExtensionAnchor } from "./ExtensionSettings";

type TabKey = "overview" | "snippets" | "context" | "agent" | "developer";

// User-facing section name is "Extensions" (the internal folder/route keeps the
// legacy "experimentations" id). Constant so the i18n lint treats brand chrome
// as an identifier, not translatable copy.
const SECTION_TITLE = "Extensions";
const DEVELOPER_MODE_LABEL = "Developer mode";

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

const DEVELOPER_TAB = {
  key: "developer" as const,
  label: "Developer",
  icon: <Code2 width={15} height={15} />,
};

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
  const [developerMode, setDeveloperMode] = useState(false);
  const [overviewRevision, setOverviewRevision] = useState(0);
  const [importBusy, setImportBusy] = useState(false);
  const [importNotice, setImportNotice] = useState<{
    kind: "success" | "error";
    text: string;
  } | null>(null);

  useEffect(() => {
    void invoke<{ enabled: boolean }>("extension_developer_status")
      .then((status) => setDeveloperMode(status.enabled))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!developerMode && tab === "developer") setTab("overview");
  }, [developerMode, tab]);

  const tabs = developerMode ? [...TABS, DEVELOPER_TAB] : TABS;

  const importPack = async () => {
    setImportBusy(true);
    setImportNotice(null);
    try {
      const selected = await open({
        title: "Import Grain extension pack",
        multiple: false,
        directory: false,
        filters: [{ name: "Grain extension pack", extensions: ["grainpack"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      const id = await invoke<string>("extension_import_pack", {
        path: selected,
      });
      setTab("overview");
      setOverviewRevision((revision) => revision + 1);
      setImportNotice({ kind: "success", text: `Imported ${id}` });
    } catch (error) {
      setImportNotice({ kind: "error", text: String(error) });
    } finally {
      setImportBusy(false);
    }
  };

  return (
    <div className="max-w-4xl w-full mx-auto space-y-6">
      {/* Page title — set larger than the other consoles; the tabs sit directly
          beneath it, no subtitle, so the section opens clean. */}
      <div className="flex items-center justify-between gap-3 px-1">
        <h1 className="text-[1.7rem] font-semibold tracking-tight leading-none">
          {SECTION_TITLE}
        </h1>
        <div className="flex items-center gap-2">
          <button
            type="button"
            disabled={importBusy}
            onClick={() => void importPack()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-line px-2.5 py-1.5 text-xs font-medium text-ink hover:border-ink-faint disabled:opacity-50 cursor-pointer"
          >
            <Upload width={13} height={13} />
            {importBusy ? "Importing…" : "Import pack"}
          </button>
          {developerMode && (
            <div className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/30 bg-amber-500/10 px-2.5 py-1 text-[11px] font-medium text-amber-700 dark:text-amber-300">
              <Code2 width={12} height={12} />
              {DEVELOPER_MODE_LABEL}
            </div>
          )}
        </div>
      </div>

      {importNotice && (
        <div
          className={`rounded-lg px-3 py-2 text-sm ${
            importNotice.kind === "success"
              ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
              : "bg-red-500/10 text-red-600"
          }`}
        >
          {importNotice.text}
        </div>
      )}

      {/* Segmented sub-tab selector — a recessed track of instrument buttons;
          the active one fills solid (ink) with an accent icon. */}
      <div
        role="tablist"
        aria-label={SECTION_TITLE}
        className="flex items-center gap-1 p-1 rounded-xl bg-paper-sunken border border-line"
      >
        {tabs.map((tb) => {
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
          key={overviewRevision}
          onJump={(id) => setTab(JUMP_TARGETS[id] ?? "overview")}
          onDeveloperModeChange={setDeveloperMode}
        />
      ) : tab === "snippets" ? (
        <div className="space-y-8">
          <SnippetsSection />
          {/* Voice Actions is now its own extension — its editor lives in the
              Voice Actions extension page (Overview → Voice Actions), not here.
              SPEC §4.3: this anchor stays for snippet packs that extend here. */}
          <ExtensionAnchor anchor="snippets.after" />
        </div>
      ) : tab === "context" ? (
        <div className="space-y-8">
          <ContextAwareSection />
          <ExtensionAnchor anchor="context.after" />
        </div>
      ) : tab === "agent" ? (
        <div className="space-y-8">
          <AgentSection />
          <ExtensionAnchor anchor="agent.after" />
        </div>
      ) : (
        <DeveloperSection />
      )}
    </div>
  );
};
