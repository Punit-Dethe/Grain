import React, { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import {
  ChevronLeft,
  Code2,
  Trash2,
  ExternalLink,
  FolderOpen,
  Package,
  Replace,
  ShieldCheck,
  Sliders,
  Store,
  X,
} from "lucide-react";
import {
  ANCHORS,
  ExtensionSettings,
  ExtensionShortcuts,
  type SettingRow,
  type SettingsSection,
} from "./ExtensionSettings";

/** Plain-language capability wording for the permission sheet (SPEC §1.3).
 * One map, phrased as what the extension can DO to the user — never the raw
 * capability name, which means nothing to the person approving it. */
const CAPABILITY_LABELS: Record<string, string> = {
  "events:sessions": "See when recording starts and stops",
  "events:transcripts": "Read what you dictate",
  "events:audio-levels": "See live microphone levels",
  "transform:transcript": "Rewrite your text before it is pasted",
  "session:start": "Start a recording session itself",
  storage: "Store its own data on this device",
  settings: "Save its own settings",
  llm: "Send text to your configured AI provider",
  embed: "Turn text into embeddings",
  "capture:selection": "Read your currently selected text",
  "open:url": "Open web links in your browser",
  "open:app": "Launch apps you choose",
};

const capabilityLabel = (cap: string) =>
  cap.startsWith("net:")
    ? `Send data to ${cap.slice("net:".length)}`
    : (CAPABILITY_LABELS[cap] ?? cap);

/** The backend holds a scripted extension at first enable and answers with a
 * structured `{"needsPermissions":[…]}` error (grain_commands.rs). Anything
 * else is a real failure and surfaces as one. */
function parseNeedsPermissions(e: unknown): string[] | null {
  try {
    const parsed = JSON.parse(String(e)) as { needsPermissions?: unknown };
    return Array.isArray(parsed?.needsPermissions)
      ? (parsed.needsPermissions as string[])
      : null;
  } catch {
    return null;
  }
}

/** Plain-language name for each exclusive position (SPEC §3.2). The user is
 * agreeing to hand over a *place in Grain*, so the prompt has to say which. */
const SLOT_LABELS: Record<string, string> = {
  "overlay.recording": "the recording overlay",
  "overlay.pointer": "the pointer overlay",
  "pill.theme": "the pill's look",
  "agent.reply-surface": "the Agent's reply panel",
  "output.destination": "where your text is sent",
};

const slotLabel = (slot: string) =>
  SLOT_LABELS[slot] ??
  (slot.startsWith("overrides:")
    ? `the “${slot.slice("overrides:".length)}” setting`
    : slot);

/** Reserved occupant id for Grain's own built-in behaviour (grain-core). */
const CORE_DEFAULT = "grain.core";

interface SlotConflict {
  slot: string;
  currentOccupant: string;
}

/** The registry refuses a contested slot with `{"slotConflict":{…}}` rather
 * than letting the newcomer win by load order (SPEC §3.2). */
function parseSlotConflict(e: unknown): SlotConflict | null {
  try {
    const parsed = JSON.parse(String(e)) as { slotConflict?: SlotConflict };
    return parsed?.slotConflict?.slot ? parsed.slotConflict : null;
  } catch {
    return null;
  }
}

/** Mirror of the Rust `ExtensionCard` (grain_commands.rs). Local type until the
 * next dev run regenerates bindings.ts — never hand-edit bindings. */
interface ExtensionCard {
  id: string;
  name: string;
  description: string;
  version: string;
  tier: "builtin" | "pack" | "scripted" | "native";
  trust: "core" | "community" | "dev";
  overrides_installed: boolean;
  overridden_version: string | null;
  enabled: boolean;
  /** u64 as string; "18446744073709551615" (u64::MAX) = never toggled. */
  toggle_seq: string;
  repository: string | null;
  /** The pack declares settings or shortcuts — it has a section of its own. */
  has_detail: boolean;
}

interface ExtensionDeveloperStatus {
  enabled: boolean;
  loaded: Array<{ id: string; path: string }>;
}

const NEVER_TOGGLED = "18446744073709551615";

/** Toggle-order sort (SPEC §4.4): enabled first, each group by the order the
 * user enabled them in; never-toggled sorts last, stable by name. */
function sortCards(cards: ExtensionCard[]): ExtensionCard[] {
  const seq = (c: ExtensionCard) =>
    c.toggle_seq === NEVER_TOGGLED
      ? Number.MAX_SAFE_INTEGER
      : Number(c.toggle_seq);
  return [...cards].sort((a, b) => {
    if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
    const d = seq(a) - seq(b);
    return d !== 0 ? d : a.name.localeCompare(b.name);
  });
}

/** [GRAIN] Extensions → Overview (SPEC §5.1): every installed extension,
 * enabled and disabled alike — name (jump), inline toggle, description on
 * hover, repository link, tier chip, and the (future) store entry point. */
export const OverviewSection: React.FC<{
  onJump: (id: string) => void;
  onDeveloperModeChange: (enabled: boolean) => void;
}> = ({ onJump, onDeveloperModeChange }) => {
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [developer, setDeveloper] = useState<ExtensionDeveloperStatus>({
    enabled: false,
    loaded: [],
  });
  const [developerBusy, setDeveloperBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  /** The extension held at first enable, awaiting the user's approval. */
  const [pending, setPending] = useState<{
    card: ExtensionCard;
    permissions: string[];
  } | null>(null);
  /** The extension held at a contested slot, awaiting an explicit takeover. */
  const [contested, setContested] = useState<{
    card: ExtensionCard;
    conflict: SlotConflict;
  } | null>(null);

  /** Enabled extensions' declared settings, so Overview knows which cards have
   * a section of their own to open. */
  const [sections, setSections] = useState<SettingsSection[]>([]);
  /** The extension whose own settings section is open, if any. */
  const [detail, setDetail] = useState<string | null>(null);
  /** The store slide-over (SPEC §5.3). A SHELL only for now — the index,
   * install-from-remote, and trust badges are gated behind
   * GATE-DISTRIBUTION-AND-DEVMODE.md, so this opens onto an honest empty state. */
  const [storeOpen, setStoreOpen] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [next, secs, dev] = await Promise.all([
        invoke<ExtensionCard[]>("extensions_overview"),
        invoke<SettingsSection[]>("extension_settings_sections").catch(
          () => [] as SettingsSection[],
        ),
        invoke<ExtensionDeveloperStatus>("extension_developer_status"),
      ]);
      setCards(sortCards(next));
      setSections(secs);
      setDeveloper(dev);
      onDeveloperModeChange(dev.enabled);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [onDeveloperModeChange]);

  /** SPEC §4.3: a setting with no anchor — or an anchor this build doesn't
   * know — belongs to the extension's own section. Settings are never lost. */
  const ownRows = useCallback(
    (id: string): SettingRow[] =>
      (sections.find((s) => s.id === id)?.rows ?? []).filter(
        (r) => !r.anchor || !(ANCHORS as readonly string[]).includes(r.anchor),
      ),
    [sections],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggle = async (card: ExtensionCard) => {
    setBusy(card.id);
    try {
      await invoke("extension_set_enabled", {
        id: card.id,
        enabled: !card.enabled,
      });
      await refresh();
    } catch (e) {
      // A scripted extension enabling for the first time is held until the
      // user approves its capabilities — show the sheet instead of an error.
      const needs = parseNeedsPermissions(e);
      const conflict = parseSlotConflict(e);
      if (needs) setPending({ card, permissions: needs });
      else if (conflict) setContested({ card, conflict });
      else setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const uninstall = async (card: ExtensionCard) => {
    // SPEC §6: default is to KEEP data; removal is a separate explicit step.
    if (
      !window.confirm(
        `Uninstall "${card.name}"?\n\nIts saved data is kept, so you can reinstall later.`,
      )
    )
      return;
    setBusy(card.id);
    try {
      await invoke("extension_uninstall", { id: card.id, purge: false });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const setDeveloperMode = async (enabled: boolean) => {
    setDeveloperBusy(true);
    try {
      await invoke("extension_set_developer_mode", { enabled });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setDeveloperBusy(false);
    }
  };

  const loadUnpacked = async () => {
    setDeveloperBusy(true);
    try {
      await invoke<string | null>("extension_load_unpacked");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setDeveloperBusy(false);
    }
  };

  const unloadDev = async (id: string) => {
    setDeveloperBusy(true);
    try {
      await invoke("extension_unload_dev", { id });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setDeveloperBusy(false);
    }
  };

  /** Who holds a slot, in words. Core defaults have no card to name. */
  const occupantName = useCallback(
    (id: string) =>
      id === CORE_DEFAULT
        ? "Grain's built-in default"
        : (cards.find((c) => c.id === id)?.name ?? id),
    [cards],
  );

  /** Take over → hand the slot across, then retry the enable that was held.
   * One more conflict can follow if the extension claims several slots; the
   * prompt simply reappears for the next one. */
  const takeOver = async () => {
    if (!contested) return;
    const { card, conflict } = contested;
    setContested(null);
    setBusy(card.id);
    try {
      await invoke("extension_take_slot", { id: card.id, slot: conflict.slot });
      await invoke("extension_set_enabled", { id: card.id, enabled: true });
      await refresh();
    } catch (e) {
      const next = parseSlotConflict(e);
      if (next) setContested({ card, conflict: next });
      else setError(String(e));
      await refresh();
    } finally {
      setBusy(null);
    }
  };

  /** Approve → record the grants, then retry the enable that was held. */
  const approve = async () => {
    if (!pending) return;
    const { card, permissions } = pending;
    setPending(null);
    setBusy(card.id);
    try {
      await invoke("extension_grant", { id: card.id, permissions });
      await invoke("extension_set_enabled", { id: card.id, enabled: true });
      await refresh();
    } catch (e) {
      // Permissions are checked before slots, so an approved extension can
      // still land on a contested position — hand it to that prompt.
      const conflict = parseSlotConflict(e);
      if (conflict) setContested({ card, conflict });
      else setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  // The extension's own settings section (SPEC §4.3 fallback). Rendered in
  // place of the list so the tab bar never grows with extension count.
  const openSection = sections.find((s) => s.id === detail);
  if (detail) {
    const card = cards.find((c) => c.id === detail);
    return (
      <div className="space-y-3">
        <button
          type="button"
          onClick={() => setDetail(null)}
          className="flex items-center gap-1 text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer"
        >
          <ChevronLeft width={13} height={13} />
          All extensions
        </button>
        <div className="px-1">
          <h2 className="text-sm font-medium text-ink">
            {openSection?.name ?? card?.name ?? detail}
          </h2>
          {card?.description && (
            <p className="text-xs text-ink-faint">{card.description}</p>
          )}
        </div>
        {/* An extension may contribute settings, shortcuts, or both; a disabled
            one contributes neither until it is turned back on (SPEC §6). */}
        {openSection && (
          <ExtensionSettings
            section={openSection}
            rows={ownRows(detail)}
            onChanged={() => void refresh()}
          />
        )}
        <ExtensionShortcuts id={detail} />
        {!openSection && card && !card.enabled && (
          <p className="px-1 text-xs text-ink-faint">
            Turn this extension on to see its settings.
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {error && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 text-red-600 text-sm">
          {error}
        </div>
      )}

      <div className="rounded-xl border border-line bg-paper-raised">
        <div className="flex items-center gap-3 px-4 py-3">
          <Code2 width={15} height={15} className="text-ink-faint" />
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium text-ink">Developer mode</div>
            <div className="text-xs text-ink-faint">
              Load extension code from a folder on this device.
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={developer.enabled}
            disabled={developerBusy}
            onClick={() => void setDeveloperMode(!developer.enabled)}
            className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer ${
              developer.enabled
                ? "bg-accent"
                : "bg-paper-sunken border border-line"
            } ${developerBusy ? "opacity-50" : ""}`}
          >
            <span
              className={`absolute top-0.5 w-4 h-4 rounded-full bg-paper-raised shadow transition-all ${
                developer.enabled ? "left-[18px]" : "left-0.5"
              }`}
            />
          </button>
        </div>

        {developer.enabled && (
          <div className="border-t border-line px-4 py-3 space-y-3">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-xs font-medium text-ink">
                  Load unpacked
                </div>
                <div className="text-[11px] text-ink-faint">
                  Local code has the same permission checks as installed
                  extensions.
                </div>
              </div>
              <button
                type="button"
                disabled={developerBusy}
                onClick={() => void loadUnpacked()}
                className="inline-flex items-center gap-1.5 rounded-lg border border-line px-2.5 py-1.5 text-xs text-ink hover:border-ink-faint disabled:opacity-50 cursor-pointer"
              >
                <FolderOpen width={13} height={13} />
                Choose folder…
              </button>
            </div>

            {developer.loaded.length > 0 && (
              <div className="space-y-1.5">
                {developer.loaded.map((entry) => (
                  <div
                    key={entry.id}
                    className="flex items-center gap-2 rounded-lg bg-paper-sunken px-2.5 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="text-xs font-medium text-ink truncate">
                        {entry.id}
                      </div>
                      <div
                        className="text-[10px] text-ink-faint truncate"
                        title={entry.path}
                      >
                        {entry.path}
                      </div>
                    </div>
                    <span className="rounded border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-amber-700 dark:text-amber-300">
                      dev
                    </span>
                    <button
                      type="button"
                      disabled={developerBusy}
                      onClick={() => void unloadDev(entry.id)}
                      className="text-ink-faint hover:text-ink disabled:opacity-50 cursor-pointer"
                      aria-label={`Unload ${entry.id}`}
                      title="Unload"
                    >
                      <X width={13} height={13} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {cards.length === 0 && !error && (
        <div className="rounded-xl border border-line bg-paper-raised px-4 py-6 text-sm text-ink-faint text-center">
          Loading extensions…
        </div>
      )}

      {/* [GRAIN] Active (enabled) above; installed-but-inactive below a labelled
          divider — the two states never share one list. */}
      {(
        [
          { key: "active", label: null, items: cards.filter((c) => c.enabled) },
          {
            key: "inactive",
            label: "Installed · not active",
            items: cards.filter((c) => !c.enabled),
          },
        ] as const
      ).map((group) =>
        group.items.length === 0 ? null : (
          <div key={group.key} className="space-y-2">
            {group.label && (
              <div className="flex items-center gap-2 px-1 text-[11px] uppercase tracking-wide text-ink-faint">
                <span>{group.label}</span>
                <span className="flex-1 border-t border-line" />
              </div>
            )}
            <div className="rounded-xl border border-line bg-paper-raised divide-y divide-line">
              {group.items.map((card) => (
                <div
                  key={card.id}
                  className="flex items-center gap-3 px-4 py-3 group"
                  title={card.description}
                >
                  <Package
                    width={15}
                    height={15}
                    className={card.enabled ? "text-accent" : "text-ink-faint"}
                  />
                  <div className="flex-1 min-w-0">
                    <button
                      type="button"
                      onClick={() =>
                        card.has_detail ? setDetail(card.id) : onJump(card.id)
                      }
                      className="text-sm font-medium text-ink hover:text-accent transition-colors cursor-pointer"
                    >
                      {card.name}
                    </button>
                    <div className="text-xs text-ink-faint truncate">
                      {card.description}
                    </div>
                    {card.overrides_installed && (
                      <div className="text-[10px] text-amber-700 dark:text-amber-300">
                        Installed
                        {card.overridden_version
                          ? ` v${card.overridden_version}`
                          : ""}{" "}
                        · Overridden by dev extension
                      </div>
                    )}
                  </div>
                  {card.trust === "dev" && (
                    <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded border border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300">
                      dev
                    </span>
                  )}
                  <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-paper-sunken text-ink-faint border border-line">
                    {card.tier === "builtin" ? "built-in" : card.tier} · v
                    {card.version}
                  </span>
                  {card.has_detail && (
                    <button
                      type="button"
                      onClick={() => setDetail(card.id)}
                      className="text-ink-faint hover:text-ink transition-colors cursor-pointer"
                      aria-label={`Settings for ${card.name}`}
                      title="Settings"
                    >
                      <Sliders width={13} height={13} />
                    </button>
                  )}
                  {card.repository && (
                    <a
                      href={card.repository}
                      target="_blank"
                      rel="noreferrer"
                      className="text-ink-faint hover:text-ink transition-colors"
                      aria-label="Repository"
                    >
                      <ExternalLink width={13} height={13} />
                    </a>
                  )}
                  {/* Uninstall — only real installed packs (not the three
                      settings-backed built-ins, and not load-unpacked dev
                      projects, which unload from the Developer panel). */}
                  {card.tier !== "builtin" && card.trust !== "dev" && (
                    <button
                      type="button"
                      disabled={busy === card.id}
                      onClick={() => void uninstall(card)}
                      className="text-ink-faint hover:text-red-600 transition-colors cursor-pointer disabled:opacity-50"
                      aria-label={`Uninstall ${card.name}`}
                      title="Uninstall"
                    >
                      <Trash2 width={13} height={13} />
                    </button>
                  )}
                  {/* Inline enable toggle. A scripted extension's first enable is
                held by the backend until the permission sheet below is
                approved (SPEC §6). */}
                  <button
                    type="button"
                    role="switch"
                    aria-checked={card.enabled}
                    disabled={busy === card.id}
                    onClick={() => void toggle(card)}
                    className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer ${
                      card.enabled
                        ? "bg-accent"
                        : "bg-paper-sunken border border-line"
                    } ${busy === card.id ? "opacity-50" : ""}`}
                  >
                    <span
                      className={`absolute top-0.5 w-4 h-4 rounded-full bg-paper-raised shadow transition-all ${
                        card.enabled ? "left-[18px]" : "left-0.5"
                      }`}
                    />
                  </button>
                </div>
              ))}
            </div>
          </div>
        ),
      )}

      {/* Store entry point (SPEC §5.3) — fills the content region full-width. */}
      <button
        type="button"
        onClick={() => setStoreOpen(true)}
        className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-xl border border-dashed border-line text-sm text-ink-soft hover:text-ink hover:border-ink-faint transition-colors"
      >
        <Store width={14} height={14} />
        Browse extensions
      </button>

      {storeOpen && (
        <StoreSlideOver
          onClose={() => setStoreOpen(false)}
          onChanged={() => void refresh()}
        />
      )}

      {/* Permission sheet (SPEC §6, the Chrome model): a scripted extension
          runs code, so nothing starts until the user approves what it asked
          for. Cancel simply leaves it disabled. */}
      {pending && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
          role="dialog"
          aria-modal="true"
          onClick={() => setPending(null)}
        >
          <div
            className="w-full max-w-sm rounded-xl border border-line bg-paper-raised shadow-lg p-4 space-y-3"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center gap-2">
              <ShieldCheck width={16} height={16} className="text-accent" />
              <h3 className="text-sm font-medium text-ink">
                Allow “{pending.card.name}”?
              </h3>
            </div>
            <p className="text-xs text-ink-faint">
              This extension runs its own code on your device. It is asking to:
            </p>
            <ul className="space-y-1.5">
              {pending.permissions.map((p) => (
                <li key={p} className="flex items-start gap-2 text-xs text-ink">
                  <span className="mt-[5px] w-1 h-1 rounded-full bg-accent shrink-0" />
                  <span>{capabilityLabel(p)}</span>
                </li>
              ))}
            </ul>
            <div className="flex justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={() => setPending(null)}
                className="px-3 py-1.5 rounded-lg text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void approve()}
                className="px-3 py-1.5 rounded-lg text-xs font-medium bg-accent text-white hover:opacity-90 transition-opacity cursor-pointer"
              >
                Allow and enable
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Takeover prompt (SPEC §3.2): one enabled occupant per slot. The
          incumbent is named and switched off explicitly — never displaced
          silently, and never by whichever extension happened to load first. */}
      {contested && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
          role="dialog"
          aria-modal="true"
          onClick={() => setContested(null)}
        >
          <div
            className="w-full max-w-sm rounded-xl border border-line bg-paper-raised shadow-lg p-4 space-y-3"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center gap-2">
              <Replace width={16} height={16} className="text-accent" />
              <h3 className="text-sm font-medium text-ink">
                Replace {slotLabel(contested.conflict.slot)}?
              </h3>
            </div>
            <p className="text-xs text-ink-faint">
              Only one extension can control{" "}
              {slotLabel(contested.conflict.slot)}. It is currently{" "}
              <span className="text-ink">
                {occupantName(contested.conflict.currentOccupant)}
              </span>
              .
            </p>
            <p className="text-xs text-ink-faint">
              Turning on “{contested.card.name}” will switch{" "}
              {contested.conflict.currentOccupant === CORE_DEFAULT
                ? "Grain's own version off"
                : `“${occupantName(contested.conflict.currentOccupant)}” off`}
              . You can switch back at any time.
            </p>
            <div className="flex justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={() => setContested(null)}
                className="px-3 py-1.5 rounded-lg text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer"
              >
                Keep current
              </button>
              <button
                type="button"
                onClick={() => void takeOver()}
                className="px-3 py-1.5 rounded-lg text-xs font-medium bg-accent text-white hover:opacity-90 transition-opacity cursor-pointer"
              >
                Replace and enable
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

/** [GRAIN] One entry from the verified index (mirror of the Rust `StoreEntry`;
 * raw-invoke local type until a dev run regenerates bindings.ts). */
type StoreEntry = {
  id: string;
  name: string;
  version: string;
  tier: string;
  trust: string;
  capabilities: string[];
  size: string;
  author: string;
  reviewed_at: string;
  reviewed_commit: string;
  revocation: string | null;
  flags: string[];
};
type StoreView = {
  status: string; // "fresh" | "offline" | "needs-newer-client"
  can_install: boolean;
  entries: StoreEntry[];
};

const TRUST_BADGE: Record<string, { label: string; cls: string }> = {
  core: { label: "Core", cls: "bg-accent/15 text-accent" },
  verified: { label: "Verified", cls: "bg-emerald-500/15 text-emerald-600" },
  experimental: {
    label: "Experimental",
    cls: "bg-amber-500/15 text-amber-600",
  },
  dev: { label: "Community", cls: "bg-line text-ink-soft" },
};

/** [GRAIN] The store slide-over (SPEC §5.3): a Zen-Mods-style panel that slides
 * in from the right INSIDE the settings window. Backed by the verified,
 * signed catalogue via `store_browse` (Phase 5A/5B) — install verifies the
 * artifact hash before unpacking, and trust is shown from the signed index. */
const StoreSlideOver: React.FC<{
  onClose: () => void;
  onChanged?: () => void;
}> = ({ onClose, onChanged }) => {
  const [view, setView] = useState<StoreView | null>(null);
  const [installed, setInstalled] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [installing, setInstalling] = useState<string | null>(null);
  // Measured position: fill everything right of the sidebar and below the
  // titlebar, read from the live DOM so it survives UI scaling and never
  // hard-codes the sidebar width.
  const [box, setBox] = useState<{ left: number; top: number }>({
    left: 240,
    top: 36,
  });

  useEffect(() => {
    const measure = () => {
      const bar = document
        .getElementById("grain-sidebar")
        ?.getBoundingClientRect();
      setBox({ left: bar ? bar.right : 240, top: bar ? bar.top : 36 });
    };
    measure();
    window.addEventListener("resize", measure);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const reload = useCallback(async () => {
    const [v, cards] = await Promise.all([
      invoke<StoreView>("store_browse"),
      invoke<{ id: string; version: string }[]>("extensions_overview").catch(
        () => [] as { id: string; version: string }[],
      ),
    ]);
    setView(v);
    setInstalled(Object.fromEntries(cards.map((c) => [c.id, c.version])));
  }, []);

  // Fetch on open; drop the parsed index on close (the overhead rule §5.3).
  useEffect(() => {
    let alive = true;
    reload()
      .catch((e) => alive && setError(String(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      void invoke("store_close").catch(() => {});
    };
  }, [reload]);

  const install = useCallback(
    async (entry: StoreEntry) => {
      setInstalling(entry.id);
      setError(null);
      try {
        await invoke("store_install", { id: entry.id, version: entry.version });
        await reload();
        onChanged?.();
      } catch (e) {
        setError(String(e));
      } finally {
        setInstalling(null);
      }
    },
    [reload, onChanged],
  );

  const entries = (view?.entries ?? []).filter((e) => {
    const q = query.trim().toLowerCase();
    return (
      !q ||
      e.name.toLowerCase().includes(q) ||
      e.id.toLowerCase().includes(q) ||
      e.author.toLowerCase().includes(q)
    );
  });

  return createPortal(
    // [GRAIN] Portaled to <body> and positioned from the MEASURED sidebar edge,
    // so it fills everything right of the sidebar / below the titlebar without a
    // hard-coded offset and regardless of the app's UI-scale transform.
    <div
      className="fixed right-0 bottom-0 z-40 bg-paper flex flex-col"
      style={{ left: box.left, top: box.top }}
      role="dialog"
      aria-modal="true"
      aria-label="Extension store"
    >
      <div className="flex items-center justify-between px-6 py-4 border-b border-line">
        <button
          type="button"
          onClick={onClose}
          className="inline-flex items-center gap-1.5 text-sm text-ink-soft hover:text-ink transition-colors cursor-pointer"
        >
          <ChevronLeft width={15} height={15} />
          All extensions
        </button>
        <div className="flex items-center gap-2 text-sm font-medium text-ink">
          <Store width={15} height={15} />
          Extension store
        </div>
      </div>

      {/* Honest connection state (§2.1): offline serves cache, refuses installs. */}
      {view && view.status !== "fresh" && (
        <div className="px-6 py-2 text-[11px] text-ink-faint bg-line/40 border-b border-line">
          {view.status === "needs-newer-client"
            ? "This store needs a newer version of Grain."
            : "Offline — showing the last catalogue. New installs are paused until reconnected."}
        </div>
      )}

      <div className="px-6 py-3 border-b border-line">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search extensions"
          className="w-full max-w-md px-3 py-1.5 rounded-lg bg-paper-raised border border-line text-sm text-ink placeholder:text-ink-faint focus:outline-none focus:border-ink-faint"
        />
      </div>

      <div className="flex-1 overflow-y-auto px-6 py-5">
        {loading && (
          <div className="flex flex-col items-center justify-center gap-2 py-16 text-ink-faint">
            <Package width={24} height={24} />
            <span className="text-xs">Loading the catalogue…</span>
          </div>
        )}
        {error && (
          <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-600">
            {error}
          </div>
        )}
        {!loading && !error && entries.length === 0 && (
          <div className="flex flex-col items-center justify-center gap-2 py-16 px-8 text-center text-ink-faint">
            <Package width={24} height={24} />
            <span className="text-sm text-ink">No extensions yet</span>
            <p className="text-xs leading-relaxed max-w-sm">
              The catalogue is empty right now. You can also import a{" "}
              <span className="font-mono">.grainpack</span> you trust from the
              Extensions header.
            </p>
          </div>
        )}
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 xl:grid-cols-3">
          {entries.map((e) => {
            const badge = TRUST_BADGE[e.trust] ?? TRUST_BADGE.dev;
            const revoked = e.revocation === "revoked";
            const deprecated = e.revocation === "deprecated";
            return (
              <div
                key={`${e.id}@${e.version}`}
                className="rounded-xl border border-line bg-paper-raised p-4 flex flex-col gap-2"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-sm font-medium text-ink truncate">
                        {e.name}
                      </span>
                      <span
                        className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${badge.cls}`}
                      >
                        {e.trust === "verified" && (
                          <ShieldCheck
                            width={9}
                            height={9}
                            className="inline mr-0.5 -mt-0.5"
                          />
                        )}
                        {badge.label}
                      </span>
                    </div>
                    <div className="text-[11px] text-ink-faint truncate">
                      {e.author ? `${e.author} · ` : ""}v{e.version}
                    </div>
                  </div>
                  {(() => {
                    const have = installed[e.id];
                    const isInstalled = have != null;
                    const upToDate = have === e.version;
                    const busyThis = installing === e.id;
                    const label = busyThis
                      ? "Installing…"
                      : isInstalled && upToDate
                        ? "Installed"
                        : isInstalled
                          ? "Update"
                          : "Install";
                    const disabled =
                      busyThis ||
                      revoked ||
                      (isInstalled && upToDate) ||
                      !view?.can_install;
                    return (
                      <button
                        type="button"
                        disabled={disabled}
                        onClick={() => void install(e)}
                        className={`shrink-0 px-2.5 py-1 rounded-lg border text-xs transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
                          isInstalled && upToDate
                            ? "border-line text-ink-faint"
                            : "border-line text-ink hover:border-ink-faint cursor-pointer"
                        }`}
                      >
                        {label}
                      </button>
                    );
                  })()}
                </div>

                {e.reviewed_at && (
                  <div className="text-[10px] text-ink-faint">
                    Reviewed {e.reviewed_at}
                    {e.reviewed_commit
                      ? ` at ${e.reviewed_commit.slice(0, 7)}`
                      : ""}
                  </div>
                )}

                {/* Flagged combinations (§3.3): what the reviewer was warned of. */}
                {e.flags.map((f) => (
                  <div
                    key={f}
                    className="text-[10px] text-amber-600 flex items-center gap-1"
                  >
                    <ShieldCheck width={9} height={9} /> {f}
                  </div>
                ))}

                {revoked && (
                  <div className="text-[10px] text-red-600">
                    Revoked — install disabled.
                  </div>
                )}
                {deprecated && (
                  <div className="text-[10px] text-ink-faint">
                    Deprecated — no longer maintained.
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>,
    document.body,
  );
};
