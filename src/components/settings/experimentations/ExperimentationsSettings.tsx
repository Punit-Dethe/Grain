import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Bot,
  Code2,
  LayoutGrid,
  NotebookPen,
  Replace,
  Sparkles,
  Upload,
} from "lucide-react";
import { OverviewSection } from "./OverviewSection";
import { SnippetsSection } from "./SnippetsSection";
import { ContextAwareSection } from "./ContextAwareSection";
import { AgentSection } from "./AgentSection";
import { DeveloperSection } from "./DeveloperSection";
import { FeaturePanel, useFeatureEnabled } from "./FeaturePanel";
import { ExtensionAnchor } from "./ExtensionSettings";
import { GrainSpaceSettings } from "../grain-space/GrainSpaceSettings";
import { McpBridge } from "../grain-space/McpBridge";

type TabKey =
  | "overview"
  | "snippets"
  | "context"
  | "agent"
  | "grainspace"
  | "developer";

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
  {
    key: "grainspace",
    label: "Grain Space",
    icon: <NotebookPen width={15} height={15} />,
  },
];

const DEVELOPER_TAB = {
  key: "developer" as const,
  label: "Developer",
  icon: <Code2 width={15} height={15} />,
};

/** Where clicking a name in Overview lands (SPEC §5.1). Keyed by BOTH the
 * settings anchor an extension declares — the general case, so a new extension
 * anchored at `context.after` jumps to Context with no change here — and a few
 * core ids that have a tab but no anchor of their own.
 *
 * An anchor missing from this map is not an error: `jumpTo` returns false and
 * the caller opens the extension's own page instead. `models.after` is one such
 * — it renders in the Speech-to-Text section of the sidebar, not in this hub. */
const JUMP_TARGETS: Record<string, TabKey> = {
  "snippets.after": "snippets",
  "context.after": "context",
  "dictation.pipeline.after": "context",
  "agent.after": "agent",
  "grainspace.after": "grainspace",
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
  const [developerBusy, setDeveloperBusy] = useState(false);
  /** An extension's own page is a PAGE, not a panel inside the hub — while one
   * is open the hub hides its title, tab bar and import button. */
  const [detailOpen, setDetailOpen] = useState(false);
  const [overviewRevision, setOverviewRevision] = useState(0);
  const [importBusy, setImportBusy] = useState(false);
  // The snippets EDITOR is a rich surface, not a settings row, so it lives
  // outside the feature's well and needs the flag directly to disappear with it.
  const snippetsOn = useFeatureEnabled("snippets_enabled");
  const grainSpaceOn = useFeatureEnabled("grain_space_enabled");
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

  /** Developer mode is a HEADER control, not a row in the list of things you
   * installed — it is a property of Grain, and a full-width card explaining it
   * sat above every user's extensions to be read once and then ignored. As an
   * icon it stays one click away and costs a normal user nothing. */
  const toggleDeveloperMode = async () => {
    const next = !developerMode;
    setDeveloperBusy(true);
    try {
      await invoke("extension_set_developer_mode", { enabled: next });
      setDeveloperMode(next);
    } catch {
      // Leave the button where it was; the backend rejected the change.
    } finally {
      setDeveloperBusy(false);
    }
  };

  /** Jump to a tab by anchor or extension id. False = nowhere here to go. */
  const jumpTo = (target: string): boolean => {
    const next = JUMP_TARGETS[target];
    if (!next) return false;
    setTab(next);
    return true;
  };

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
          beneath it, no subtitle, so the section opens clean. Hidden while an
          extension's own page is open: that page has its own title and its own
          "All extensions" way back. */}
      {!detailOpen && (
      <>
      <div className="flex items-center justify-between gap-3 px-1">
        <h1 className="text-[1.7rem] font-semibold tracking-tight leading-none">
          {SECTION_TITLE}
        </h1>
        <div className="flex items-center gap-2">
          {/* On/off is carried by the button itself — filled means on. A
              separate badge announcing the state of the control next to it is
              the same fact twice. */}
          <button
            type="button"
            aria-pressed={developerMode}
            aria-label={DEVELOPER_MODE_LABEL}
            title={
              developerMode
                ? `${DEVELOPER_MODE_LABEL} · on`
                : DEVELOPER_MODE_LABEL
            }
            disabled={developerBusy}
            onClick={() => void toggleDeveloperMode()}
            className={`inline-flex items-center justify-center rounded-lg border p-1.5 transition-colors disabled:opacity-50 cursor-pointer ${
              developerMode
                ? "border-ink bg-ink text-paper"
                : "border-line text-ink-faint hover:border-ink-faint hover:text-ink"
            }`}
          >
            <Code2 width={13} height={13} />
          </button>
          <button
            type="button"
            disabled={importBusy}
            onClick={() => void importPack()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-line px-2.5 py-1.5 text-xs font-medium text-ink hover:border-ink-faint disabled:opacity-50 cursor-pointer"
          >
            <Upload width={13} height={13} />
            {importBusy ? "Importing…" : "Import pack"}
          </button>
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
      </>
      )}

      {tab === "overview" ? (
        <OverviewSection
          key={overviewRevision}
          onJump={jumpTo}
          onDetailOpenChange={setDetailOpen}
        />
      ) : tab === "snippets" ? (
        <div className="space-y-6">
          {/* Grain's own features carry their master switch as the FIRST ROW of
              their own tab — they are not installed packs and no longer appear
              in Overview — and everything they govern follows it. */}
          <FeaturePanel
            settingKey="snippets_enabled"
            title="Snippets"
            info="Speak a trigger word and Grain expands it into your saved text, before anything is pasted. Fully local — no AI call, and the trigger is stripped from what you get."
          />
          {snippetsOn && <SnippetsSection untitled />}
          {/* SPEC §4.3: an extension's settings render next to the feature they
              extend. Renders nothing when nothing anchors here. Grain's own
              built-in Actions used to sit at this exact spot; the Voice Actions
              extension now anchors here instead, which is what the position was
              always holding open.

              It is NOT hidden with the feature: an extension anchored here is a
              separate thing with its own switch, and Voice Actions works
              whether or not Grain's snippets do. */}
          <ExtensionAnchor anchor="snippets.after" />
        </div>
      ) : tab === "context" ? (
        <div className="space-y-6">
          <FeaturePanel
            settingKey="context_awareness_enabled"
            title="Context awareness"
            info="Detects the app you're dictating into and adapts AI formatting to it — an IDE keeps technical terms, chat stays casual, email gets a little more polished. Requires post-processing to be on."
          >
            <ContextAwareSection />
          </FeaturePanel>
          <ExtensionAnchor anchor="context.after" />
        </div>
      ) : tab === "grainspace" ? (
        <div className="space-y-6">
          {/* Grain Space is BUILT IN, like the three above it. It briefly
              shipped as a `builtin`-tier extension you could install and
              uninstall, which was theatre: the implementation is compiled into
              Grain — a local embedding engine, a vector index, a native window
              — and the "install" was a registry row in front of it. A feature
              that cannot actually be removed should not offer to be. */}
          <FeaturePanel
            settingKey="grain_space_enabled"
            title="Grain Space"
            info="A local notebook you dictate into and ask questions across. Notes are plain Markdown on your disk; search runs on this machine over full text, meaning and the things your notes mention."
          />
          {grainSpaceOn && (
            <>
              <GrainSpaceSettings embedded />
              <McpBridge />
            </>
          )}
          {/* Extensions extend Grain Space here — the MCP bridge is the first. */}
          <ExtensionAnchor anchor="grainspace.after" />
        </div>
      ) : tab === "agent" ? (
        <div className="space-y-6">
          <FeaturePanel
            settingKey="agent_enabled"
            title="Agent"
            info="Summon a voice-first AI assistant on your current selection — ask a question, rewrite what you highlighted, or run a follow-up conversation without leaving the app you're in."
          >
            <AgentSection />
          </FeaturePanel>
          <ExtensionAnchor anchor="agent.after" />
        </div>
      ) : (
        <DeveloperSection />
      )}
    </div>
  );
};
