/* eslint-disable i18next/no-literal-string -- UI 2.0 prototype copy is a frozen visual contract until the cutover translation pass. */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Bot,
  BookOpen,
  ChevronRight,
  Code2,
  CornerDownRight,
  Plus,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import {
  commands,
  type ExtensionCard,
  type Snippet,
  type StoreEntry,
  type StoreView,
} from "@/bindings";
import { AgentSection } from "@/components/settings/experimentations/AgentSection";
import { ContextAwareSection } from "@/components/settings/experimentations/ContextAwareSection";
import { FeaturePanel } from "@/components/settings/experimentations/FeaturePanel";
import { ExtensionAnchor } from "@/components/settings/experimentations/ExtensionSettings";
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
    description: "Teach Grain the words that matter to you.",
    icon: BookOpen,
  },
  snippets: {
    title: "Snippets",
    description:
      "Create reusable text, links, signatures, and phrases that Grain can expand from your voice.",
    icon: Code2,
  },
  context: {
    title: "Context awareness",
    description:
      "Grain uses the active application to match terminology, tone, and formatting.",
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

type DictionaryDialogState =
  | { mode: "add" }
  | { mode: "edit"; original: string };

function ToolDialog({
  id,
  title,
  description,
  busy,
  onClose,
  children,
}: {
  id: string;
  title: string;
  description: string;
  busy: boolean;
  onClose: () => void;
  children: React.ReactNode;
}) {
  const dialogRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const returnFocusTo = document.activeElement as HTMLElement | null;
    dialogRef.current
      ?.querySelector<HTMLElement>("[data-dialog-initial-focus]")
      ?.focus();
    return () => returnFocusTo?.focus();
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
      if (event.key !== "Tab") return;
      const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
        "button:not(:disabled), input:not(:disabled), textarea:not(:disabled)",
      );
      if (!focusable?.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busy, onClose]);

  return (
    <div
      className="dictionary-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <section
        ref={dialogRef}
        className="dictionary-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={`${id}-title`}
        aria-describedby={`${id}-description`}
      >
        <button
          className="dictionary-dialog-close"
          type="button"
          aria-label="Close"
          disabled={busy}
          onClick={onClose}
        >
          <X size={16} aria-hidden="true" />
        </button>
        <header className="dictionary-dialog-header">
          <h2 id={`${id}-title`}>{title}</h2>
          <p id={`${id}-description`}>{description}</p>
        </header>
        {children}
      </section>
    </div>
  );
}

function DictionaryTermDialog({
  state,
  busy,
  onClose,
  onSave,
}: {
  state: DictionaryDialogState;
  busy: boolean;
  onClose: () => void;
  onSave: (value: string) => Promise<string | null>;
}) {
  const editing = state.mode === "edit";
  const [term, setTerm] = useState(editing ? state.original : "");
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const nextError = await onSave(term);
    if (nextError) {
      setError(nextError);
    } else {
      onClose();
    }
  };

  return (
    <ToolDialog
      id="dictionary-term-dialog"
      title={editing ? "Edit term" : "Add a term"}
      description={
        editing
          ? "Update this term in your personal dictionary."
          : "Help Grain recognize names, slang, or custom terms."
      }
      busy={busy}
      onClose={onClose}
    >
      <form
        className="dictionary-dialog-form"
        onSubmit={(event) => void submit(event)}
      >
        <div className="dictionary-dialog-fields">
          <input
            data-dialog-initial-focus
            className="dictionary-dialog-input"
            value={term}
            maxLength={50}
            spellCheck={false}
            autoComplete="off"
            disabled={busy}
            aria-label={editing ? "Term" : "The term you'll say"}
            aria-invalid={Boolean(error) || undefined}
            aria-describedby={error ? "dictionary-term-error" : undefined}
            placeholder={editing ? undefined : "The term you'll say"}
            onChange={(event) => {
              setTerm(event.target.value);
              setError(null);
            }}
          />
        </div>
        {error && (
          <span
            id="dictionary-term-error"
            className="dictionary-term-error"
            role="alert"
          >
            {error}
          </span>
        )}
        <div className="dictionary-dialog-actions">
          <button
            className="dictionary-save-button"
            type="submit"
            disabled={!term.trim() || busy}
          >
            {busy ? "Saving…" : editing ? "Save changes" : "Add"}
          </button>
        </div>
      </form>
    </ToolDialog>
  );
}

function DictionaryTool() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [query, setQuery] = useState("");
  const [dialog, setDialog] = useState<DictionaryDialogState | null>(null);
  const words = getSetting("custom_words") ?? [];
  const busy = isUpdating("custom_words");
  const closeDialog = useCallback(() => setDialog(null), []);
  const filteredWords = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) return words;
    return words.filter((word) =>
      word.toLocaleLowerCase().includes(normalizedQuery),
    );
  }, [query, words]);

  const saveTerm = async (value: string) => {
    const candidate = value.trim().replace(/[<>"']/g, "");
    if (!candidate) return "Enter a term to continue.";
    if (/\s/.test(candidate)) return "Use one word or a hyphenated term.";
    if (candidate.length > 50) return "Keep the term under 50 characters.";

    const original = dialog?.mode === "edit" ? dialog.original : null;
    const duplicate = words.some(
      (word) =>
        word !== original &&
        word.toLocaleLowerCase() === candidate.toLocaleLowerCase(),
    );
    if (duplicate) return `“${candidate}” is already in your dictionary.`;

    if (original === null) {
      await updateSetting("custom_words", [...words, candidate]);
      return null;
    }

    const index = words.indexOf(original);
    if (index < 0) return "This term is no longer in your dictionary.";
    const nextWords = [...words];
    nextWords[index] = candidate;
    await updateSetting("custom_words", nextWords);
    return null;
  };

  const removeTerm = async (word: string) => {
    const index = words.indexOf(word);
    if (index < 0) {
      return;
    }
    await updateSetting(
      "custom_words",
      words.filter((_, wordIndex) => wordIndex !== index),
    );
  };

  return (
    <section className="dictionary-workspace" aria-label="Personal dictionary">
      <div className="dictionary-guide">
        <div className="dictionary-guide-copy">
          <h2>How to use your personal dictionary</h2>
          <p>
            Add names, product terms, and specialist words Grain should
            recognize. Your dictionary stays on this device.
          </p>
          <button type="button" className="dictionary-learn-more" disabled>
            Learn more
          </button>
        </div>
        <div className="dictionary-guide-examples" aria-hidden="true">
          <span>Grain</span>
          <span>Tauri</span>
          <span>MacBook Pro</span>
        </div>
      </div>

      <div className="dictionary-toolbar">
        <label className="dictionary-search tool-search">
          <Search size={17} aria-hidden="true" />
          <span className="sr-only">Search dictionary</span>
          <input
            className="tool-search-input"
            type="text"
            inputMode="search"
            role="searchbox"
            value={query}
            placeholder="Search"
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <button
          className="dictionary-add-button"
          type="button"
          onClick={() => setDialog({ mode: "add" })}
        >
          <Plus size={16} aria-hidden="true" />
          Add term
        </button>
      </div>

      {filteredWords.length > 0 ? (
        <div className="dictionary-term-grid" aria-label="Dictionary terms">
          {filteredWords.map((word) => (
            <div key={word} className="dictionary-term-card">
              <button
                className="dictionary-term"
                type="button"
                aria-label={`Edit ${word}`}
                onClick={() => setDialog({ mode: "edit", original: word })}
              >
                {word}
              </button>
              <button
                className="tool-card-delete"
                type="button"
                aria-label={`Remove ${word}`}
                disabled={busy}
                onClick={() => void removeTerm(word)}
              >
                <Trash2 size={15} aria-hidden="true" />
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div className="dictionary-empty-state">
          <strong>{query.trim() ? "No matching terms" : "No terms yet"}</strong>
          <span>
            {query.trim()
              ? "Try another search."
              : "Add a term Grain should recognize."}
          </span>
        </div>
      )}

      {dialog && (
        <DictionaryTermDialog
          state={dialog}
          busy={busy}
          onClose={closeDialog}
          onSave={saveTerm}
        />
      )}
    </section>
  );
}

const MAX_SNIPPET_TRIGGER_LENGTH = 100;

const normalizeSnippetTrigger = (trigger: string) =>
  trigger.toLocaleLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

type SnippetDialogState = { mode: "add" } | { mode: "edit"; snippet: Snippet };

function SnippetDialog({
  state,
  busy,
  onClose,
  onSave,
}: {
  state: SnippetDialogState;
  busy: boolean;
  onClose: () => void;
  onSave: (trigger: string, replacement: string) => Promise<string | null>;
}) {
  const editing = state.mode === "edit";
  const [trigger, setTrigger] = useState(editing ? state.snippet.trigger : "");
  const [replacement, setReplacement] = useState(
    editing ? state.snippet.replacement : "",
  );
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const nextError = await onSave(trigger, replacement);
    if (nextError) {
      setError(nextError);
    } else {
      onClose();
    }
  };

  return (
    <ToolDialog
      id="snippet-dialog"
      title={editing ? "Edit shortcut" : "Add a shortcut"}
      description="Say the trigger phrase and Grain types the saved text."
      busy={busy}
      onClose={onClose}
    >
      <form
        className="dictionary-dialog-form"
        onSubmit={(event) => void submit(event)}
      >
        <div className="dictionary-dialog-fields">
          <label>
            <span className="sr-only">Trigger phrase</span>
            <input
              data-dialog-initial-focus
              className="dictionary-dialog-input"
              value={trigger}
              maxLength={MAX_SNIPPET_TRIGGER_LENGTH}
              autoComplete="off"
              spellCheck={false}
              disabled={busy}
              aria-invalid={Boolean(error) || undefined}
              placeholder="The phrase you'll say"
              onChange={(event) => {
                setTrigger(event.target.value);
                setError(null);
              }}
            />
          </label>
          <label>
            <span className="sr-only">Replacement text</span>
            <textarea
              className="dictionary-dialog-input dictionary-dialog-textarea"
              value={replacement}
              disabled={busy}
              aria-invalid={Boolean(error) || undefined}
              placeholder="What Grain should type"
              onChange={(event) => {
                setReplacement(event.target.value);
                setError(null);
              }}
            />
          </label>
        </div>
        {error && (
          <span className="dictionary-term-error" role="alert">
            {error}
          </span>
        )}
        <div className="dictionary-dialog-actions">
          <button
            className="dictionary-save-button"
            type="submit"
            disabled={!trigger.trim() || !replacement.trim() || busy}
          >
            {busy ? "Saving…" : editing ? "Save changes" : "Add"}
          </button>
        </div>
      </form>
    </ToolDialog>
  );
}

function SnippetsMasterToggle() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("snippets_enabled") ?? false;
  const busy = isUpdating("snippets_enabled");

  return (
    <label className="tool-master-toggle" title="Turn snippets on or off">
      <span className="sr-only">Enable snippets</span>
      <input
        type="checkbox"
        checked={enabled}
        disabled={busy}
        onChange={(event) =>
          void updateSetting("snippets_enabled", event.target.checked)
        }
      />
      <span className="tool-master-toggle-track" aria-hidden="true" />
    </label>
  );
}

function ContextMasterToggle() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("context_awareness_enabled") ?? false;
  const busy = isUpdating("context_awareness_enabled");

  return (
    <label
      className="tool-master-toggle"
      title="Turn context awareness on or off"
    >
      <span className="sr-only">Enable context awareness</span>
      <input
        type="checkbox"
        checked={enabled}
        disabled={busy}
        onChange={(event) =>
          void updateSetting("context_awareness_enabled", event.target.checked)
        }
      />
      <span className="tool-master-toggle-track" aria-hidden="true" />
    </label>
  );
}

function SnippetsTool() {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("snippets_enabled") ?? false;
  const [query, setQuery] = useState("");
  const [dialog, setDialog] = useState<SnippetDialogState | null>(null);
  const snippets = getSetting("snippets") ?? [];
  const busy = isUpdating("snippets");
  const closeDialog = useCallback(() => setDialog(null), []);
  const filteredSnippets = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) return snippets;
    return snippets.filter((snippet) =>
      `${snippet.trigger}\n${snippet.replacement}`
        .toLocaleLowerCase()
        .includes(normalizedQuery),
    );
  }, [query, snippets]);

  const saveSnippet = async (trigger: string, replacement: string) => {
    const candidate = trigger.trim();
    if (!candidate) return "Enter a trigger phrase to continue.";
    if (candidate.length > MAX_SNIPPET_TRIGGER_LENGTH) {
      return `Keep the trigger under ${MAX_SNIPPET_TRIGGER_LENGTH} characters.`;
    }
    if (!replacement.trim()) return "Enter the text Grain should type.";

    const editingId = dialog?.mode === "edit" ? dialog.snippet.id : null;
    const normalized = normalizeSnippetTrigger(candidate);
    if (!normalized) return "Use at least one letter or number in the trigger.";
    const duplicate = snippets.some(
      (snippet) =>
        snippet.id !== editingId &&
        normalizeSnippetTrigger(snippet.trigger) === normalized,
    );
    if (duplicate) return `“${candidate}” already has a shortcut.`;

    const nextSnippets: Snippet[] = editingId
      ? snippets.map((snippet) =>
          snippet.id === editingId
            ? { ...snippet, trigger: candidate, replacement }
            : snippet,
        )
      : [
          ...snippets,
          {
            id: crypto.randomUUID(),
            trigger: candidate,
            replacement,
            enabled: true,
          },
        ];
    await updateSetting("snippets", nextSnippets);
    return null;
  };

  const removeSnippet = async (id: string) => {
    await updateSetting(
      "snippets",
      snippets.filter((snippet) => snippet.id !== id),
    );
  };

  if (!enabled) return null;

  return (
    <>
      <section className="dictionary-workspace" aria-label="Personal shortcuts">
        <div className="dictionary-guide snippets-guide">
          <div className="dictionary-guide-copy">
            <h2>How to use snippets</h2>
            <p>
              Say a short trigger phrase and Grain replaces it with your saved
              text. Snippets stay on this device.
            </p>
            <button type="button" className="dictionary-learn-more" disabled>
              Learn more
            </button>
          </div>
          <div className="dictionary-guide-examples" aria-hidden="true">
            <span>my email</span>
            <span>meeting link</span>
            <span>sign off</span>
          </div>
        </div>

        <div className="dictionary-toolbar">
          <label className="dictionary-search tool-search">
            <Search size={17} aria-hidden="true" />
            <span className="sr-only">Search shortcuts</span>
            <input
              className="tool-search-input"
              type="text"
              inputMode="search"
              role="searchbox"
              value={query}
              placeholder="Search"
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <button
            className="dictionary-add-button"
            type="button"
            onClick={() => setDialog({ mode: "add" })}
          >
            <Plus size={16} aria-hidden="true" />
            Add shortcut
          </button>
        </div>

        {filteredSnippets.length > 0 ? (
          <div className="snippet-card-grid" aria-label="Saved shortcuts">
            {filteredSnippets.map((snippet) => (
              <div key={snippet.id} className="snippet-card">
                <button
                  className="snippet-card-main"
                  type="button"
                  aria-label={`Edit ${snippet.trigger}`}
                  onClick={() =>
                    setDialog({ mode: "edit", snippet: { ...snippet } })
                  }
                >
                  <strong>{snippet.trigger}</strong>
                  <span className="snippet-card-replacement">
                    <CornerDownRight size={14} aria-hidden="true" />
                    <span>{snippet.replacement}</span>
                  </span>
                </button>
                <button
                  className="tool-card-delete"
                  type="button"
                  aria-label={`Remove ${snippet.trigger}`}
                  disabled={busy}
                  onClick={() => void removeSnippet(snippet.id)}
                >
                  <Trash2 size={15} aria-hidden="true" />
                </button>
              </div>
            ))}
          </div>
        ) : (
          <div className="dictionary-empty-state">
            <strong>
              {query.trim() ? "No matching shortcuts" : "No shortcuts yet"}
            </strong>
            <span>
              {query.trim()
                ? "Try another search."
                : "Add a phrase Grain should expand."}
            </span>
          </div>
        )}

        {dialog && (
          <SnippetDialog
            state={dialog}
            busy={busy}
            onClose={closeDialog}
            onSave={saveSnippet}
          />
        )}
      </section>
      <ExtensionAnchor anchor="snippets.after" />
      <ToolRecommendations tool="snippets" />
    </>
  );
}

function ContextTool() {
  const { getSetting } = useSettings();
  const enabled = getSetting("context_awareness_enabled") ?? false;

  return (
    <>
      {enabled && (
        <section className="context-awareness-workspace">
          <ContextAwareSection />
        </section>
      )}
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
                <header
                  className={`tool-main-heading ${section === "snippets" || section === "context" ? "has-toggle" : ""}`}
                >
                  <div>
                    <h1 id="next-tool-title">
                      {section === "dictionary"
                        ? "Personal Dictionary"
                        : copy.title}
                    </h1>
                    <p>{copy.description}</p>
                  </div>
                  {section === "snippets" && <SnippetsMasterToggle />}
                  {section === "context" && <ContextMasterToggle />}
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
