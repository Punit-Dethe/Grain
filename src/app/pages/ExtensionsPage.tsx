/* eslint-disable i18next/no-literal-string -- UI 2.0 prototype copy is a frozen visual contract until the cutover translation pass. */
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type MouseEvent,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Code2,
  Eye,
  PackageOpen,
  ShieldCheck,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import {
  commands,
  type ExtensionCard,
  type ExtensionDeveloperStatus,
  type ExtensionSettingField,
  type ExtensionSettingRow,
  type ExtensionSettingsSection,
  type StoreEntry,
  type StoreMedia,
  type StoreView,
} from "@/bindings";
import { MediaArtwork, StoreCard } from "../extensions/StoreCard";
import { Markdown } from "@/components/markdown/Markdown";
import "@/components/markdown/markdown.css";
import { DeveloperSection } from "@/components/settings/experimentations/DeveloperSection";
import {
  ANCHORS,
  ExtensionSettings,
  ExtensionShortcuts,
  type SettingField,
  type SettingRow,
  type SettingsSection,
} from "@/components/settings/experimentations/ExtensionSettings";
import { useSettings } from "@/hooks/useSettings";
import { hashForRoute, type ExtensionViewId } from "../navigation";
import {
  capabilityLabel,
  extensionDestination,
  filterExtensions,
  nextMediaIndex,
  parseApprovalRequest,
  parseSlotConflict,
  promptLayerScope,
  slotLabel,
  sortExtensionCards,
  unwrapResult,
  type ApprovalRequest,
  type SlotConflict,
} from "../extensions/extensionRuntime";

const CORE_DEFAULT = "grain.core";

const SETTING_KINDS = new Set<SettingRow["kind"]>([
  "bool",
  "string",
  "secret",
  "number",
  "select",
  "shortcut",
  "color",
  "slider",
  "app_path",
  "url",
  "list",
  "panel",
  "unsupported",
]);

function adaptSettingField(field: ExtensionSettingField): SettingField {
  return {
    ...field,
    kind: SETTING_KINDS.has(field.kind as SettingRow["kind"])
      ? (field.kind as SettingRow["kind"])
      : "unsupported",
    fields: field.fields.map(adaptSettingField),
  };
}

function adaptSettingRow(row: ExtensionSettingRow): SettingRow {
  return {
    ...row,
    kind: SETTING_KINDS.has(row.kind as SettingRow["kind"])
      ? (row.kind as SettingRow["kind"])
      : "unsupported",
    fields: row.fields.map(adaptSettingField),
  };
}

interface InstalledController {
  cards: ExtensionCard[];
  sections: ExtensionSettingsSection[];
  covers: Record<string, StoreMedia>;
  loading: boolean;
  error: string | null;
  busy: string | null;
  refresh: () => Promise<void>;
  toggle: (card: ExtensionCard) => Promise<void>;
  uninstall: (card: ExtensionCard) => Promise<void>;
  permissionDialog: React.ReactNode;
  conflictDialog: React.ReactNode;
}

function useInstalledExtensions(): InstalledController {
  const { refreshSettings } = useSettings();
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [sections, setSections] = useState<ExtensionSettingsSection[]>([]);
  const [covers, setCovers] = useState<Record<string, StoreMedia>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [pending, setPending] = useState<
    ({ card: ExtensionCard } & ApprovalRequest) | null
  >(null);
  const [contested, setContested] = useState<{
    card: ExtensionCard;
    conflict: SlotConflict;
  } | null>(null);

  const refresh = useCallback(async () => {
    const [overview, nextSections] = await Promise.all([
      commands.extensionsOverview().then(unwrapResult),
      commands
        .extensionSettingsSections()
        .then(unwrapResult)
        .catch(() => []),
    ]);
    const nextCards = sortExtensionCards(overview);
    const nextCovers = nextCards.length
      ? await commands
          .storeCovers(nextCards.map((card) => card.id))
          .then(unwrapResult)
          .catch(() => [])
      : [];
    setCards(nextCards);
    setSections(nextSections);
    setCovers(
      Object.fromEntries(
        nextCovers.map((cover) => [
          cover.id,
          { sha256: cover.sha256, kind: cover.kind },
        ]),
      ),
    );
    setError(null);
    await refreshSettings();
  }, [refreshSettings]);

  useEffect(() => {
    let alive = true;
    void refresh()
      .catch((reason) => alive && setError(String(reason)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [refresh]);

  const toggle = useCallback(
    async (card: ExtensionCard) => {
      setBusy(card.id);
      setError(null);
      try {
        unwrapResult(
          await commands.extensionSetEnabled(card.id, !card.enabled),
        );
        await refresh();
      } catch (reason) {
        const message =
          reason instanceof Error ? reason.message : String(reason);
        const approval = parseApprovalRequest(message);
        const conflict = parseSlotConflict(message);
        if (approval) setPending({ card, ...approval });
        else if (conflict) setContested({ card, conflict });
        else setError(message);
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  const uninstall = useCallback(
    async (card: ExtensionCard) => {
      if (
        !window.confirm(
          `Uninstall "${card.name}"?\n\nIts saved data is kept for a future reinstall.`,
        )
      ) {
        return;
      }
      setBusy(card.id);
      setError(null);
      try {
        unwrapResult(await commands.extensionUninstall(card.id, false));
        await refresh();
      } catch (reason) {
        setError(String(reason));
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  const occupantName = useCallback(
    (id: string) =>
      id === CORE_DEFAULT
        ? "Grain's built-in default"
        : (cards.find((card) => card.id === id)?.name ?? id),
    [cards],
  );

  const approve = useCallback(async () => {
    if (!pending) return;
    const { card, permissions } = pending;
    setPending(null);
    setBusy(card.id);
    try {
      unwrapResult(await commands.extensionGrant(card.id, permissions));
      unwrapResult(await commands.extensionSetEnabled(card.id, true));
      await refresh();
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      const conflict = parseSlotConflict(message);
      if (conflict) setContested({ card, conflict });
      else setError(message);
    } finally {
      setBusy(null);
    }
  }, [pending, refresh]);

  const takeOver = useCallback(async () => {
    if (!contested) return;
    const { card, conflict } = contested;
    setContested(null);
    setBusy(card.id);
    try {
      unwrapResult(await commands.extensionTakeSlot(card.id, conflict.slot));
      unwrapResult(await commands.extensionSetEnabled(card.id, true));
      await refresh();
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      const next = parseSlotConflict(message);
      if (next) setContested({ card, conflict: next });
      else setError(message);
      await refresh();
    } finally {
      setBusy(null);
    }
  }, [contested, refresh]);

  const permissionDialog = pending ? (
    <div
      className="extension-confirm-backdrop"
      role="presentation"
      onClick={() => setPending(null)}
    >
      <div
        className="extension-confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="permission-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="extension-confirm-title">
          <ShieldCheck size={17} />
          <h2 id="permission-title">Allow “{pending.card.name}”?</h2>
        </div>
        {pending.permissions.length > 0 && (
          <>
            <p>This extension runs code on your device and is asking to:</p>
            <ul>
              {pending.permissions.map((permission) => (
                <li key={permission}>{capabilityLabel(permission)}</li>
              ))}
            </ul>
          </>
        )}
        {pending.promptLayers.length > 0 && (
          <>
            {/*
              The exact wording, verbatim, never a summary. This text goes to
              the model in front of what the user dictates, so paraphrasing it
              here would mean approving something other than what was read —
              and the reason it is asked at all is that a later update can
              change it, which is how approved extensions have turned hostile
              elsewhere.
            */}
            <p>
              It supplies these reviewed prompt instructions when you dictate.
              Replacement entries take only the named prompt position; Prompt
              Record remains private to Grain and always has higher priority.
            </p>
            <ul className="extension-prompt-layers">
              {pending.promptLayers.map((layer) => (
                <li key={layer.id}>
                  <span className="extension-prompt-layer-scope">
                    {promptLayerScope(layer)}
                  </span>
                  <q>{layer.text}</q>
                </li>
              ))}
            </ul>
          </>
        )}
        {pending.card.kind === "searchable" && (
          <>
            {/*
              G1. Under V1 the chosen extension receives the WHOLE transcript,
              not extracted parameters — a materially wider disclosure than any
              capability in the list above expresses, and one no capability can
              express, because it is inherent in being pickable at all. Stated
              here as a consequence of choosing it, not offered as a toggle.
            */}
            <p className="extension-confirm-disclosure">
              When you pick this extension in Extension Mode, Grain hands it
              everything you said — the whole request, word for word.
              {pending.card.recommend?.purpose
                ? ` It asks to be offered for: ${pending.card.recommend.purpose}.`
                : ""}
            </p>
            {pending.card.needs.includes("semantic") && (
              <p className="extension-confirm-disclosure">
                It uses Grain's understanding model, a one-time download of
                about 130 MB.
              </p>
            )}
          </>
        )}
        {pending.actions.length > 0 && (
          <>
            {/*
              Listed by title — never by the phrases the extension listens for.
              What the user is deciding is what this can do, and a wall of
              utterances is the fastest way to train someone to scroll past the
              part that matters.
            */}
            <p>It can do these things when you ask out loud:</p>
            <ul className="extension-actions">
              {pending.actions.map((action) => (
                <li key={action.id}>
                  <span className="extension-action-titles">
                    {action.title}
                  </span>
                  {action.confirms && (
                    <span className="extension-action-confirms">
                      asks you first
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </>
        )}
        <div className="extension-confirm-actions">
          <button
            className="button"
            type="button"
            onClick={() => setPending(null)}
          >
            Cancel
          </button>
          <button
            className="button primary"
            type="button"
            onClick={() => void approve()}
          >
            Allow and enable
          </button>
        </div>
      </div>
    </div>
  ) : null;

  const conflictDialog = contested ? (
    <div
      className="extension-confirm-backdrop"
      role="presentation"
      onClick={() => setContested(null)}
    >
      <div
        className="extension-confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="conflict-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="extension-confirm-title">
          <ShieldCheck size={17} />
          <h2 id="conflict-title">
            Replace {slotLabel(contested.conflict.slot)}?
          </h2>
        </div>
        <p>
          {occupantName(contested.conflict.currentOccupant)} currently controls{" "}
          {slotLabel(contested.conflict.slot)}. Enabling {contested.card.name}{" "}
          will switch it off.
        </p>
        <div className="extension-confirm-actions">
          <button
            className="button"
            type="button"
            onClick={() => setContested(null)}
          >
            Keep current
          </button>
          <button
            className="button primary"
            type="button"
            onClick={() => void takeOver()}
          >
            Replace and enable
          </button>
        </div>
      </div>
    </div>
  ) : null;

  return {
    cards,
    sections,
    covers,
    loading,
    error,
    busy,
    refresh,
    toggle,
    uninstall,
    permissionDialog,
    conflictDialog,
  };
}

type DrawerSelection =
  | { source: "installed"; card: ExtensionCard }
  | { source: "store"; entry: StoreEntry };

function ExtensionDrawer({
  selection,
  controller,
  onClose,
  onInstall,
  installing,
  canInstall,
}: {
  selection: DrawerSelection;
  controller: InstalledController;
  onClose: () => void;
  onInstall: (entry: StoreEntry) => Promise<void>;
  installing: string | null;
  canInstall: boolean;
}) {
  const [catalogueEntry, setCatalogueEntry] = useState<StoreEntry | null>(
    selection.source === "store" ? selection.entry : null,
  );
  const [mediaIndex, setMediaIndex] = useState(0);
  const [readme, setReadme] = useState<string | null>(null);
  const [readmeLoading, setReadmeLoading] = useState(false);
  const [readmeOpen, setReadmeOpen] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  useEffect(() => {
    let alive = true;
    setMediaIndex(0);
    setReadme(null);
    setReadmeOpen(false);
    if (selection.source === "installed") {
      setCatalogueEntry(null);
      void commands
        .storeEntry(selection.card.id)
        .then(unwrapResult)
        .then((entry) => alive && setCatalogueEntry(entry))
        .catch(() => alive && setCatalogueEntry(null));
    } else {
      setCatalogueEntry(selection.entry);
    }
    return () => {
      alive = false;
      setCatalogueEntry(null);
      setReadme(null);
    };
  }, [selection]);

  const card = selection.source === "installed" ? selection.card : null;
  const entry = selection.source === "store" ? selection.entry : catalogueEntry;
  const name = card?.name ?? entry?.name ?? "Extension";
  const description = card?.description ?? entry?.description ?? "";
  const media = entry?.media ?? [];
  const capabilities = entry?.capabilities ?? card?.capabilities ?? [];
  const installedVersion = controller.cards.find(
    (candidate) => candidate.id === (card?.id ?? entry?.id),
  )?.version;
  const current = !!entry && installedVersion === entry.version;
  const destination = card
    ? extensionDestination(card, controller.sections)
    : null;

  // Fetch as soon as there is a README to fetch. It is still lazy in the sense
  // that matters — nothing is fetched until a drawer actually opens, and the
  // text is dropped again when it closes.
  const readmeHash = entry?.readme;
  useEffect(() => {
    if (!readmeHash) return;
    let live = true;
    setReadmeLoading(true);
    commands
      .storeReadme(readmeHash)
      .then((result) => {
        if (live) setReadme(unwrapResult(result));
      })
      .catch(() => {
        if (live) setReadme("");
      })
      .finally(() => {
        if (live) setReadmeLoading(false);
      });
    return () => {
      live = false;
    };
  }, [readmeHash]);

  return (
    <>
      {/* Clicking away is how everyone closes a drawer. Escape and the X were
          the only exits, which left the panel feeling stuck. */}
      <div
        className="extension-drawer-scrim"
        onClick={onClose}
        aria-hidden="true"
      />
      <aside
        className="extension-panel extension-preview-panel open"
        role="dialog"
        aria-modal="true"
        aria-labelledby="extension-drawer-title"
      >
        <div className="extension-panel-head">
          <div>
            <div className="eyebrow">Extension preview</div>
            <h2 id="extension-drawer-title">{name}</h2>
          </div>
          <button
            className="icon-button"
            type="button"
            aria-label="Close preview"
            onClick={onClose}
          >
            <X size={17} />
          </button>
        </div>

        <div className="extension-drawer-scroll">
          <div className="drawer-media-wrap">
            <MediaArtwork
              media={media[mediaIndex]}
              name={name}
              className="extension-panel-hero refined-panel-hero preview-media"
            />
            {media.length > 1 && (
              <>
                <button
                  className="media-arrow media-arrow-left"
                  type="button"
                  aria-label="Previous preview"
                  onClick={() =>
                    setMediaIndex((index) =>
                      nextMediaIndex(index, media.length, -1),
                    )
                  }
                >
                  <ChevronLeft size={18} />
                </button>
                <button
                  className="media-arrow media-arrow-right"
                  type="button"
                  aria-label="Next preview"
                  onClick={() =>
                    setMediaIndex((index) =>
                      nextMediaIndex(index, media.length, 1),
                    )
                  }
                >
                  <ChevronRight size={18} />
                </button>
                <span className="media-count">
                  {mediaIndex + 1} / {media.length}
                </span>
              </>
            )}
          </div>

          <div className="extension-panel-meta">
            <span>
              {entry?.author ||
                (card?.trust === "dev" ? "Local developer" : "Local pack")}
            </span>
            <span>Version {card?.version ?? entry?.version}</span>
            <span>{entry?.trust ?? card?.trust}</span>
          </div>
          <p>{description}</p>

          {/* Fetched with the drawer, revealed on demand. The fetch and the
              disclosure used to be the same gesture, which cost two clicks to
              read anything; separating them costs one, on already-loaded text. */}
          {(entry?.readme || readme || readmeLoading) && (
            <>
              <div className="panel-section-label">About this extension</div>
              <button
                className="readme-disclosure"
                type="button"
                aria-expanded={readmeOpen}
                onClick={() => setReadmeOpen((open) => !open)}
              >
                <BookOpen size={15} aria-hidden="true" />
                <span>
                  {readmeLoading ? "Loading README…" : "Read the full README"}
                </span>
                <ChevronRight
                  className="readme-chevron"
                  size={15}
                  aria-hidden="true"
                />
              </button>
              {readmeOpen && (
                <div className="extension-readme">
                  {readme ? (
                    <Markdown markdown={readme} softBreaks />
                  ) : (
                    <div className="drawer-muted">
                      {readmeLoading
                        ? "Loading README…"
                        : "The README could not be loaded."}
                    </div>
                  )}
                </div>
              )}
            </>
          )}

          <div className="panel-section-label">Permissions</div>
          <div className="permission-list">
            {capabilities.length ? (
              capabilities.map((capability) => (
                <div className="permission-row" key={capability}>
                  <ShieldCheck size={15} />
                  <span>{capabilityLabel(capability)}</span>
                  <strong>Required</strong>
                </div>
              ))
            ) : (
              <div className="drawer-muted">
                This pack declares no runtime permissions.
              </div>
            )}
          </div>
        </div>

        <div className="extension-drawer-actions">
          {entry && selection.source === "store" && (
            <button
              className="button primary"
              type="button"
              disabled={
                current ||
                installing === entry.id ||
                entry.revocation === "revoked" ||
                !canInstall
              }
              onClick={() => void onInstall(entry)}
            >
              {installing === entry.id
                ? "Installing…"
                : current
                  ? "Installed"
                  : installedVersion
                    ? "Update"
                    : "Install"}
            </button>
          )}
          {card && destination && destination.kind !== "preview" && (
            <button
              className="button primary"
              type="button"
              onClick={() => routeToDestination(destination)}
            >
              Open settings
            </button>
          )}
          {card?.trust !== "dev" && card && (
            <button
              className="button danger"
              type="button"
              disabled={controller.busy === card.id}
              onClick={() => void controller.uninstall(card)}
            >
              <Trash2 size={14} />
              Uninstall
            </button>
          )}
        </div>
      </aside>
    </>
  );
}

function routeToDestination(
  destination: ReturnType<typeof extensionDestination>,
) {
  if (destination.kind === "tools") {
    window.location.hash = hashForRoute({
      page: "tools",
      section: destination.section,
    }).slice(1);
  } else if (destination.kind === "settings") {
    window.location.hash = hashForRoute({
      page: "settings",
      section: destination.section,
    }).slice(1);
  } else if (destination.kind === "notes-settings") {
    window.location.hash = "/notes?settings=1";
  } else if (destination.kind === "extension-settings") {
    window.location.hash = hashForRoute({
      page: "extension-settings",
      extensionId: destination.extensionId,
    }).slice(1);
  }
}

function InstalledList({
  controller,
  query,
  onPreview,
  onBrowseStore,
}: {
  controller: InstalledController;
  query: string;
  onPreview: (selection: DrawerSelection) => void;
  onBrowseStore: () => void;
}) {
  const entries = filterExtensions(controller.cards, query);
  if (controller.loading)
    return (
      <div className="extension-state" role="status">
        Loading installed extensions…
      </div>
    );
  if (!entries.length && query)
    return (
      <div className="extension-state">
        No installed extensions match your search.
      </div>
    );
  if (!entries.length)
    return (
      <div className="extension-state extension-empty-state">
        <span className="extension-empty-symbol" aria-hidden="true">
          <PackageOpen size={22} />
        </span>
        <strong>No extensions installed yet</strong>
        <p>Browse focused additions and install only what you need.</p>
        <button className="button" type="button" onClick={onBrowseStore}>
          Browse Store
        </button>
      </div>
    );

  return (
    <div className="installed-extension-list">
      {entries.map((card) => {
        const destination = extensionDestination(card, controller.sections);
        const openCard = async () => {
          let resolvedDestination = destination;

          // Disabled packs are intentionally absent from the aggregate anchor
          // command. Read just this pack's schema before routing so its card
          // still opens the tool/settings surface it contributes to.
          if (
            !card.enabled &&
            card.has_detail &&
            destination.kind === "extension-settings"
          ) {
            try {
              const rows = unwrapResult(
                await commands.extensionSettingsSchema(card.id),
              );
              resolvedDestination = extensionDestination(card, [
                ...controller.sections,
                { id: card.id, name: card.name, rows },
              ]);
            } catch {
              // The standalone extension settings page presents the backend
              // error if the pack itself is unreadable.
            }
          }

          if (resolvedDestination.kind === "preview")
            onPreview({ source: "installed", card });
          else routeToDestination(resolvedDestination);
        };
        return (
          <article
            className={`extension-card installed-extension-card${card.enabled ? "" : " extension-disabled"}`}
            key={card.id}
            tabIndex={0}
            role="button"
            aria-label={`Open ${card.name}`}
            onClick={() => void openCard()}
            onKeyDown={(event) => {
              if (event.target !== event.currentTarget) return;
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                void openCard();
              }
            }}
          >
            <MediaArtwork
              media={controller.covers[card.id]}
              name={card.name}
              className="extension-artwork"
            />
            <div className="installed-extension-copy">
              <div className="installed-extension-heading">
                <strong>{card.name}</strong>
                {card.trust === "dev" && <span>Dev</span>}
              </div>
              <p>{card.description}</p>
              {/* Whether an extension is actually doing anything is the first
                  thing this list is asked; a switch alone makes you decode it. */}
              <span className="installed-extension-state">
                {card.enabled ? "Enabled" : "Disabled"}
              </span>
            </div>
            <div className="installed-extension-actions">
              <button
                className="icon-button extension-preview-button"
                type="button"
                title={`Preview ${card.name}`}
                aria-label={`Preview ${card.name}`}
                onClick={(event) => {
                  event.stopPropagation();
                  onPreview({ source: "installed", card });
                }}
              >
                <Eye size={16} />
              </button>
              {/* Removing an extension was reachable only by opening its
                  preview and scrolling to the footer, so the list you manage
                  extensions from was the one place you could not remove one.
                  A dev override is unloaded from the developer tools that
                  loaded it, not uninstalled. */}
              {card.trust !== "dev" && (
                <button
                  className="icon-button extension-uninstall-button"
                  type="button"
                  title={`Uninstall ${card.name}`}
                  aria-label={`Uninstall ${card.name}`}
                  disabled={controller.busy === card.id}
                  onClick={(event) => {
                    event.stopPropagation();
                    void controller.uninstall(card);
                  }}
                >
                  <Trash2 size={15} />
                </button>
              )}
              <button
                className={`toggle${card.enabled ? " on" : ""}`}
                type="button"
                role="switch"
                aria-checked={card.enabled}
                aria-label={`${card.enabled ? "Disable" : "Enable"} ${card.name}`}
                disabled={controller.busy === card.id}
                onClick={(event) => {
                  event.stopPropagation();
                  void controller.toggle(card);
                }}
              >
                <span />
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );
}

function StoreGrid({
  query,
  controller,
  onPreview,
  onStoreChange,
}: {
  query: string;
  controller: InstalledController;
  onPreview: (selection: DrawerSelection) => void;
  onStoreChange: (state: {
    view: StoreView | null;
    installing: string | null;
    install: (entry: StoreEntry) => Promise<void>;
  }) => void;
}) {
  const [view, setView] = useState<StoreView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [category, setCategory] = useState("all");
  const [installing, setInstalling] = useState<string | null>(null);
  const refreshInstalled = controller.refresh;

  const load = useCallback(async () => {
    setView(unwrapResult(await commands.storeBrowse()));
  }, []);

  useEffect(() => {
    let alive = true;
    void load()
      .catch((reason) => alive && setError(String(reason)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
      setView(null);
      void commands.storeClose();
    };
  }, [load]);

  const install = useCallback(
    async (entry: StoreEntry) => {
      setInstalling(entry.id);
      setError(null);
      try {
        unwrapResult(await commands.storeInstall(entry.id, entry.version));
        await Promise.all([load(), refreshInstalled()]);
      } catch (reason) {
        setError(String(reason));
      } finally {
        setInstalling(null);
      }
    },
    [load, refreshInstalled],
  );

  useEffect(() => {
    onStoreChange({ view, installing, install });
  }, [install, installing, onStoreChange, view]);

  const installed = useMemo(
    () => new Map(controller.cards.map((card) => [card.id, card.version])),
    [controller.cards],
  );
  const categories = useMemo(
    () =>
      [
        ...new Set((view?.entries ?? []).flatMap((entry) => entry.categories)),
      ].sort(),
    [view?.entries],
  );
  const entries = filterExtensions(view?.entries ?? [], query).filter(
    (entry) => category === "all" || entry.categories.includes(category),
  );

  if (loading)
    return (
      <div className="extension-state" role="status">
        Loading the extension store…
      </div>
    );

  return (
    <>
      {view && view.status !== "fresh" && (
        <div className="extension-store-notice">
          {view.status === "needs-newer-client"
            ? "This store requires a newer version of Grain."
            : "Offline — showing the last verified catalogue. Installs are paused."}
        </div>
      )}
      {error && <div className="extension-inline-error">{error}</div>}
      <div className="store-intro polished-store-intro">
        <div>
          <strong>Extension store</strong>
          <span>Discover focused additions for Grain.</span>
        </div>
        <select
          className="select"
          value={category}
          onChange={(event) => setCategory(event.target.value)}
          aria-label="Filter extension category"
        >
          <option value="all">All extensions</option>
          {categories.map((item) => (
            <option value={item} key={item}>
              {item}
            </option>
          ))}
        </select>
      </div>
      {!entries.length ? (
        <div className="extension-state">
          No store extensions match these filters.
        </div>
      ) : (
        <div className="polished-store-grid">
          {entries.map((entry) => (
            <StoreCard
              key={`${entry.id}@${entry.version}`}
              entry={entry}
              installedVersion={installed.get(entry.id)}
              busy={installing === entry.id}
              canInstall={Boolean(view?.can_install)}
              onInstall={(target) => void install(target)}
              onPreview={(target) =>
                onPreview({ source: "store", entry: target })
              }
            />
          ))}
        </div>
      )}
    </>
  );
}

export function ExtensionsPage({ view }: { view: ExtensionViewId }) {
  const controller = useInstalledExtensions();
  const [query, setQuery] = useState(() => {
    const raw = window.location.hash.split("?", 2)[1] ?? "";
    return new URLSearchParams(raw).get("q") ?? "";
  });
  const [drawer, setDrawer] = useState<DrawerSelection | null>(null);
  const [developer, setDeveloper] = useState<ExtensionDeveloperStatus | null>(
    null,
  );
  const [developerOpen, setDeveloperOpen] = useState(false);
  const [developerBusy, setDeveloperBusy] = useState(false);
  const [importBusy, setImportBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [storeState, setStoreState] = useState<{
    view: StoreView | null;
    installing: string | null;
    install: (entry: StoreEntry) => Promise<void>;
  }>({ view: null, installing: null, install: async () => {} });

  useEffect(() => {
    setDrawer(null);
    setStoreState({ view: null, installing: null, install: async () => {} });
  }, [view]);

  useEffect(() => {
    let alive = true;
    void commands
      .extensionDeveloperStatus()
      .then(unwrapResult)
      .then((status) => alive && setDeveloper(status))
      .catch(() => alive && setDeveloper(null));
    return () => {
      alive = false;
    };
  }, []);

  const openDeveloper = async () => {
    setDeveloperBusy(true);
    setNotice(null);
    try {
      if (!developer?.enabled) {
        unwrapResult(await commands.extensionSetDeveloperMode(true));
        setDeveloper((current) => ({
          enabled: true,
          loaded: current?.loaded ?? [],
        }));
      }
      setDeveloperOpen(true);
    } catch (reason) {
      setNotice(String(reason));
    } finally {
      setDeveloperBusy(false);
    }
  };

  const disableDeveloper = async () => {
    setDeveloperBusy(true);
    try {
      unwrapResult(await commands.extensionSetDeveloperMode(false));
      setDeveloper({ enabled: false, loaded: [] });
      setDeveloperOpen(false);
      await controller.refresh();
    } catch (reason) {
      setNotice(String(reason));
    } finally {
      setDeveloperBusy(false);
    }
  };

  const importPack = async () => {
    setImportBusy(true);
    setNotice(null);
    try {
      const selected = await open({
        title: "Import Grain extension pack",
        multiple: false,
        directory: false,
        filters: [{ name: "Grain extension pack", extensions: ["grainpack"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      const id = unwrapResult(await commands.extensionImportPack(selected));
      setNotice(`Imported ${id}. Review its permissions before enabling it.`);
      await controller.refresh();
    } catch (reason) {
      setNotice(String(reason));
    } finally {
      setImportBusy(false);
    }
  };

  const onStoreChange = useCallback(
    (next: {
      view: StoreView | null;
      installing: string | null;
      install: (entry: StoreEntry) => Promise<void>;
    }) => setStoreState(next),
    [],
  );

  return (
    <section
      className="page active extensions-workspace-page"
      data-page-panel="extensions"
    >
      <div className="page-wrap extensions-page-wrap">
        <div className="page-header">
          <div className="extensions-page-heading">
            <div className="eyebrow">Capability management</div>
            <h1>Extensions</h1>
            <p className="page-subtitle">
              Install, enable, update, and remove focused additions. Settings
              stay beside the capability each extension extends.
            </p>
          </div>
          <div className="header-actions">
            <button
              className="button"
              type="button"
              disabled={importBusy}
              onClick={() => void importPack()}
            >
              <Upload size={15} />
              {importBusy ? "Importing…" : "Import pack"}
            </button>
            <button
              className={`button${developer?.enabled ? " developer-on" : ""}`}
              type="button"
              disabled={developerBusy}
              aria-pressed={developer?.enabled ?? false}
              onClick={() => void openDeveloper()}
            >
              <Code2 size={15} />
              Developer
            </button>
          </div>
        </div>

        {notice && <div className="extension-store-notice">{notice}</div>}
        {controller.error && (
          <div className="extension-inline-error">{controller.error}</div>
        )}

        <div className="extension-toolbar">
          <div className="segmented" aria-label="Extension collection">
            <button
              className={view === "installed" ? "active" : ""}
              type="button"
              onClick={() => {
                window.location.hash = hashForRoute({
                  page: "extensions",
                  view: "installed",
                }).slice(1);
              }}
            >
              Installed <span>{controller.cards.length}</span>
            </button>
            <button
              className={view === "store" ? "active" : ""}
              type="button"
              onClick={() => {
                window.location.hash = hashForRoute({
                  page: "extensions",
                  view: "store",
                }).slice(1);
              }}
            >
              Store
            </button>
          </div>
          <label className="search-field">
            <svg className="icon sm" aria-hidden="true">
              <use href="#i-search" />
            </svg>
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={`Search ${view === "installed" ? "installed" : "store"} extensions`}
            />
          </label>
        </div>

        {view === "installed" ? (
          <InstalledList
            controller={controller}
            query={query}
            onPreview={setDrawer}
            onBrowseStore={() => {
              window.location.hash = hashForRoute({
                page: "extensions",
                view: "store",
              }).slice(1);
            }}
          />
        ) : (
          <StoreGrid
            query={query}
            controller={controller}
            onPreview={setDrawer}
            onStoreChange={onStoreChange}
          />
        )}
      </div>

      {developerOpen && (
        <div
          className="developer-drawer"
          role="dialog"
          aria-modal="true"
          aria-label="Extension developer tools"
        >
          <div className="developer-drawer-head">
            <div>
              <div className="eyebrow">Local extension tools</div>
              <h2>Developer</h2>
            </div>
            <button
              className="icon-button"
              type="button"
              aria-label="Close developer tools"
              onClick={() => setDeveloperOpen(false)}
            >
              <X size={17} />
            </button>
          </div>
          <div className="developer-drawer-scroll">
            <DeveloperSection />
          </div>
          <button
            className="button danger"
            type="button"
            disabled={developerBusy}
            onClick={() => void disableDeveloper()}
          >
            Turn off developer mode
          </button>
        </div>
      )}

      {drawer && (
        <ExtensionDrawer
          selection={drawer}
          controller={controller}
          onClose={() => setDrawer(null)}
          onInstall={storeState.install}
          installing={storeState.installing}
          canInstall={storeState.view?.can_install ?? false}
        />
      )}
      {controller.permissionDialog}
      {controller.conflictDialog}
    </section>
  );
}

export function ExtensionSettingsPage({
  extensionId,
}: {
  extensionId: string;
}) {
  const [sections, setSections] = useState<ExtensionSettingsSection[]>([]);
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [rows, nextCards] = await Promise.all([
      commands.extensionSettingsSchema(extensionId).then(unwrapResult),
      commands.extensionsOverview().then(unwrapResult),
    ]);
    const cardName = nextCards.find((card) => card.id === extensionId)?.name;
    setSections([{ id: extensionId, name: cardName ?? extensionId, rows }]);
    setCards(nextCards);
  }, [extensionId]);

  useEffect(() => {
    let alive = true;
    void refresh()
      .catch((reason) => alive && setError(String(reason)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [refresh]);

  const section = sections.find((candidate) => candidate.id === extensionId);
  const card = cards.find((candidate) => candidate.id === extensionId);
  const ownRows =
    section?.rows.filter(
      (row) =>
        !row.anchor || !(ANCHORS as readonly string[]).includes(row.anchor),
    ) ?? [];
  const adaptedSection: SettingsSection | null = section
    ? {
        id: section.id,
        name: section.name,
        rows: section.rows.map(adaptSettingRow),
      }
    : null;
  const adaptedOwnRows = ownRows.map(adaptSettingRow);

  return (
    <section
      className="page active extension-settings-workspace"
      data-page-panel="extension-settings"
    >
      <div className="page-wrap extension-settings-page-wrap">
        <div className="extension-settings-toolbar">
          <button
            className="button ghost"
            type="button"
            onClick={() => {
              window.location.hash = hashForRoute({
                page: "extensions",
                view: "installed",
              }).slice(1);
            }}
          >
            <ChevronLeft size={15} />
            Extensions
          </button>
        </div>
        <div className="extension-settings-content">
          <header className="extension-settings-heading">
            <div className="eyebrow">Extension settings</div>
            <h1>{card?.name ?? section?.name ?? extensionId}</h1>
            <p>
              Configuration provided by this extension. Identity, version,
              README, and permissions remain in its preview.
            </p>
          </header>
          {loading ? (
            <div className="extension-state" role="status">
              Loading extension settings…
            </div>
          ) : error ? (
            <div className="extension-inline-error">{error}</div>
          ) : adaptedSection && adaptedOwnRows.length ? (
            <ExtensionSettings
              section={adaptedSection}
              rows={adaptedOwnRows}
              onChanged={() => void refresh()}
            />
          ) : (
            <div className="extension-state">
              This extension has no standalone settings.
            </div>
          )}
          {card && <ExtensionShortcuts id={card.id} />}
        </div>
      </div>
    </section>
  );
}
