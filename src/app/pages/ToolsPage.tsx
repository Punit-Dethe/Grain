/* eslint-disable i18next/no-literal-string -- UI 2.0 prototype copy is a frozen visual contract until the cutover translation pass. */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Bot, BookOpen, ChevronRight, Code2, Sparkles } from "lucide-react";
import {
  commands,
  type ExtensionCard,
  type StoreEntry,
  type StoreView,
} from "@/bindings";
import { CustomWords } from "@/components/settings/CustomWords";
import { AgentSection } from "@/components/settings/experimentations/AgentSection";
import { ContextAwareSection } from "@/components/settings/experimentations/ContextAwareSection";
import {
  FeaturePanel,
  useFeatureEnabled,
} from "@/components/settings/experimentations/FeaturePanel";
import { ExtensionAnchor } from "@/components/settings/experimentations/ExtensionSettings";
import { SnippetsSection } from "@/components/settings/experimentations/SnippetsSection";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import { hashForRoute, type ToolSectionId } from "../navigation";
import { StoreCard } from "../extensions/StoreCard";
import {
  matchToolRecommendations,
  unwrapResult,
  type ToolSection,
} from "../extensions/extensionRuntime";

const TOOL_COPY: Record<
  ToolSectionId,
  { title: string; description: string; icon: typeof BookOpen }
> = {
  dictionary: {
    title: "Dictionary",
    description:
      "Teach Grain names, product terms, acronyms, and specialist vocabulary ordinary speech models may miss.",
    icon: BookOpen,
  },
  snippets: {
    title: "Snippets",
    description:
      "Create reusable text, links, signatures, and phrases that Grain can expand from your voice.",
    icon: Code2,
  },
  context: {
    title: "Context",
    description:
      "Control what Grain can use from the focused application to improve terminology, casing, and insertion.",
    icon: Sparkles,
  },
  agent: {
    title: "Agent",
    description:
      "Configure Grain's local writing assistant and the actions available from selected text.",
    icon: Bot,
  },
};

function useToolCatalogue() {
  const [entries, setEntries] = useState<StoreEntry[]>([]);
  const [view, setView] = useState<StoreView | null>(null);
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [nextView, overview] = await Promise.all([
      commands.storeBrowse().then(unwrapResult),
      commands.extensionsOverview().then(unwrapResult),
    ]);
    setView(nextView);
    setEntries(nextView.entries);
    setCards(overview);
  }, []);

  useEffect(() => {
    let alive = true;
    void refresh()
      .catch((reason) => alive && setError(String(reason)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
      setEntries([]);
      setView(null);
      setCards([]);
      void commands.storeClose();
    };
  }, [refresh]);

  const install = useCallback(
    async (entry: StoreEntry) => {
      setInstalling(entry.id);
      setError(null);
      try {
        unwrapResult(await commands.storeInstall(entry.id, entry.version));
        await refresh();
      } catch (reason) {
        setError(String(reason));
      } finally {
        setInstalling(null);
      }
    },
    [refresh],
  );

  return { entries, cards, view, loading, error, installing, install };
}

function ToolRecommendations({ tool }: { tool: ToolSection }) {
  const catalogue = useToolCatalogue();
  const installed = useMemo(
    () => new Map(catalogue.cards.map((card) => [card.id, card.version])),
    [catalogue.cards],
  );
  const recommendations = useMemo(
    () =>
      matchToolRecommendations(
        catalogue.entries,
        tool,
        new Set(installed.keys()),
      ),
    [catalogue.entries, installed, tool],
  );
  const title = TOOL_COPY[tool].title;

  return (
    <section
      className="extension-recommendations"
      data-recommendations={tool}
      aria-busy={catalogue.loading || undefined}
    >
      <div className="recommendation-heading">
        <div>
          <h2>Enhance {title}</h2>
          <p>
            Focused extensions that add capability without changing where this
            tool lives.
          </p>
        </div>
        <button
          className="text-button"
          type="button"
          onClick={() => {
            window.location.hash = `${hashForRoute({ page: "extensions", view: "store" })}?q=${encodeURIComponent(title)}`;
          }}
        >
          Browse all
        </button>
      </div>

      {catalogue.error && (
        <div className="tool-inline-error">{catalogue.error}</div>
      )}
      {catalogue.view && catalogue.view.status !== "fresh" && (
        <div className="tool-muted-state">
          {catalogue.view.status === "needs-newer-client"
            ? "These recommendations require a newer version of Grain."
            : "Offline — recommendations use the last verified catalogue and installs are paused."}
        </div>
      )}
      {catalogue.loading ? (
        <div className="tool-muted-state" role="status">
          Loading recommendations…
        </div>
      ) : recommendations.length === 0 ? (
        <div className="tool-muted-state">
          No matching store extensions are available.
        </div>
      ) : (
        <div
          className="recommendation-grid"
          data-recommendation-count={recommendations.length}
        >
          {recommendations.map((entry) => (
            <StoreCard
              key={`${entry.id}@${entry.version}`}
              entry={entry}
              busy={catalogue.installing === entry.id}
              canInstall={Boolean(catalogue.view?.can_install)}
              onInstall={(target) => void catalogue.install(target)}
              onPreview={() => {
                // Studio has no drawer of its own; the store page is where an
                // extension is read about in full.
                window.location.hash = "/extensions/store";
              }}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function DictionaryTool() {
  return (
    <>
      <section className="tool-section">
        <div className="tool-section-head">
          <div>
            <h2>Personal dictionary</h2>
            <p>Terms stay local and are used across every capture mode.</p>
          </div>
        </div>
        <div className="tool-component-host">
          <SettingsGroup>
            <CustomWords descriptionMode="tooltip" grouped />
          </SettingsGroup>
        </div>
      </section>
      {/* No "Enhance Dictionary" row: no host surface maps to the dictionary, so
          nothing can ever be recommended here — an always-empty section reads as
          broken. The other three tools do have surfaces and keep theirs. */}
    </>
  );
}

function SnippetsTool() {
  const enabled = useFeatureEnabled("snippets_enabled");
  return (
    <>
      <section className="tool-section">
        <div className="tool-section-head">
          <div>
            <h2>Snippet library</h2>
            <p>Create and manage phrases Grain can expand from your voice.</p>
          </div>
        </div>
        <div className="tool-component-host space-y-6">
          <FeaturePanel
            settingKey="snippets_enabled"
            title="Snippets"
            info="Speak a trigger phrase and Grain expands it into saved text locally before paste."
          />
          {enabled && <SnippetsSection untitled />}
        </div>
      </section>
      <ExtensionAnchor anchor="snippets.after" />
      <ToolRecommendations tool="snippets" />
    </>
  );
}

function ContextTool() {
  return (
    <>
      <section className="tool-section">
        <div className="tool-section-head">
          <div>
            <h2>Context awareness</h2>
            <p>
              Context is processed locally and only during an eligible capture.
            </p>
          </div>
        </div>
        <div className="tool-component-host space-y-6">
          <FeaturePanel
            settingKey="context_awareness_enabled"
            title="Context awareness"
            info="Use local application context to make terminology and insertion fit naturally."
          >
            <ContextAwareSection />
          </FeaturePanel>
        </div>
      </section>
      <ExtensionAnchor anchor="context.after" />
      <ToolRecommendations tool="context" />
    </>
  );
}

function AgentTool() {
  return (
    <>
      <section className="tool-section">
        <div className="tool-section-head">
          <div>
            <h2>Agent behaviour</h2>
            <p>Choose how Agent opens, responds, and uses the current app.</p>
          </div>
        </div>
        <div className="tool-component-host space-y-6">
          <FeaturePanel
            settingKey="agent_enabled"
            title="Agent"
            info="Summon a voice-first assistant over the current selection without leaving the active app."
          >
            <AgentSection />
          </FeaturePanel>
        </div>
      </section>
      <ExtensionAnchor anchor="agent.after" />
      <ToolRecommendations tool="agent" />
    </>
  );
}

export function ToolsPage({ section }: { section: ToolSectionId }) {
  const copy = TOOL_COPY[section];
  return (
    <section
      className="page active tools-workspace-page"
      data-page-panel="tools"
    >
      <div className="page-wrap tools-page-wrap">
        <div className="tools-shell">
          <aside className="tools-sidebar-pane">
            <div className="tools-pane-header">
              <div>
                <strong>Studio</strong>
                <span>Make Grain work your way</span>
              </div>
            </div>
            <nav className="tools-nav" aria-label="Grain Studio">
              {(Object.keys(TOOL_COPY) as ToolSectionId[]).map((id) => {
                const ItemIcon = TOOL_COPY[id].icon;
                return (
                  <button
                    key={id}
                    type="button"
                    className={id === section ? "active" : ""}
                    aria-current={id === section ? "page" : undefined}
                    onClick={() => {
                      window.location.hash = hashForRoute({
                        page: "tools",
                        section: id,
                      }).slice(1);
                    }}
                  >
                    <ItemIcon size={16} aria-hidden="true" />
                    <span>{TOOL_COPY[id].title}</span>
                  </button>
                );
              })}
            </nav>
            <div className="tools-sidebar-spacer" />
            <button
              className="tools-browse"
              type="button"
              onClick={() => {
                window.location.hash = hashForRoute({
                  page: "extensions",
                  view: "store",
                }).slice(1);
              }}
            >
              <span className="tools-browse-copy">
                <strong>Browse extensions</strong>
                <span>Add capabilities to Studio</span>
              </span>
              <ChevronRight size={14} aria-hidden="true" />
            </button>
          </aside>
          <section className="tools-canvas" aria-labelledby="next-tool-title">
            <div className="tools-scroll">
              <div className="tools-content next-settings-content">
                <header className="tool-main-heading">
                  <h1 id="next-tool-title">{copy.title}</h1>
                  <p>{copy.description}</p>
                </header>
                {section === "dictionary" ? (
                  <DictionaryTool />
                ) : section === "snippets" ? (
                  <SnippetsTool />
                ) : section === "context" ? (
                  <ContextTool />
                ) : (
                  <AgentTool />
                )}
              </div>
            </div>
          </section>
        </div>
      </div>
    </section>
  );
}
