import React, { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import {
  ChevronLeft,
  Trash2,
  Download,
  Package,
  Replace,
  ShieldCheck,
  Sliders,
  Store,
} from "lucide-react";
import {
  ANCHORS,
  ExtensionSettings,
  ExtensionShortcuts,
  type SettingRow,
  type SettingsSection,
} from "./ExtensionSettings";
import { ExtensionDetail, Cover, type DetailMeta } from "./ExtensionDetail";
import { useSettings } from "../../../hooks/useSettings";

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
  "capture:app": "See which app you're currently using",
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

/** Compact count: 1234 → "1.2k". */
const fmtStars = (n: number) =>
  n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n);

/** [GRAIN] The extension's cover, thumbnail-sized, at the head of its row. The
 * picture is how you actually recognise an extension in a list — a generic
 * package glyph on every row told you only that they were all extensions. Falls
 * back to that glyph for a locally imported pack with no catalogue entry. */
const RowCover: React.FC<{
  media: StoreMedia | undefined;
  name: string;
  dim: boolean;
}> = ({ media, name, dim }) => {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!media) {
      setUrl(null);
      return;
    }
    let alive = true;
    invoke<string>("store_media", { sha256: media.sha256, kind: media.kind })
      .then((u) => alive && setUrl(u))
      .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [media?.sha256, media?.kind]);

  return (
    <div
      className={`w-[72px] h-[46px] shrink-0 rounded-lg overflow-hidden border border-line bg-paper-sunken grid place-items-center transition-opacity ${
        dim ? "opacity-45" : ""
      }`}
    >
      {url ? (
        <img
          src={url}
          alt={`${name} cover`}
          className="w-full h-full object-cover"
        />
      ) : (
        <Package width={15} height={15} className="text-ink-faint/60" />
      )}
    </div>
  );
};

/** [GRAIN] Extensions → Overview (SPEC §5.1): every installed extension,
 * enabled and disabled alike — name (jumps to wherever its settings render),
 * gear (its detail page), inline toggle, repository link, and the store.
 *
 * Grain's own always-present features (Snippets, Context Awareness, Agent) are
 * NOT listed here: they have a tab each, and their on/off switch sits at the top
 * of that tab. Overview is for things you installed. */
export const OverviewSection: React.FC<{
  /** Jump to where `target` lives — an extension id or a settings anchor.
   * Returns false when this build has nowhere to jump to, so the caller can fall
   * back to the extension's own page. */
  onJump: (target: string) => boolean;
  /** True while a detail page is open, so the hub can hide its own chrome. */
  onDetailOpenChange?: (open: boolean) => void;
}> = ({ onJump, onDetailOpenChange }) => {
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  /** Enabling an extension can rewrite settings the rest of the app is already
   * showing — a prompt pack's entries land in the post-processing list. Those
   * writes happen in Rust and the settings store only listens for
   * `model-state-changed`, so without this the new prompts do not appear until
   * the app is restarted. Anything that installs, enables, disables or removes
   * an extension re-reads settings afterwards. */
  const { refreshSettings } = useSettings();
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
  /** Cover reference per installed id, read from the CACHED index in a single
   * parse and dropped with the component — the catalogue never stays resident
   * behind the list (SPEC §5.3), it just lends it its pictures. */
  const [covers, setCovers] = useState<Record<string, StoreMedia>>({});
  /** The extension whose own settings section is open, if any. */
  const [detail, setDetail] = useState<string | null>(null);
  /** Catalogue metadata for the open detail (cover/README/installs). Fetched
   * from the CACHED index only while a detail is open, and dropped on close —
   * the catalogue never stays resident for the installed list. */
  const [detailMeta, setDetailMeta] = useState<StoreEntry | null>(null);
  /** The store slide-over (SPEC §5.3). A SHELL only for now — the index,
   * install-from-remote, and trust badges are gated behind
   * GATE-DISTRIBUTION-AND-DEVMODE.md, so this opens onto an honest empty state. */
  const [storeOpen, setStoreOpen] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [next, secs] = await Promise.all([
        invoke<ExtensionCard[]>("extensions_overview"),
        invoke<SettingsSection[]>("extension_settings_sections").catch(
          () => [] as SettingsSection[],
        ),
      ]);
      setCards(sortCards(next));
      setSections(secs);
      setError(null);
      await refreshSettings();
    } catch (e) {
      setError(String(e));
    }
  }, [refreshSettings]);

  /** The anchor an extension's settings actually render at, if any — read from
   * its declared rows rather than a hard-coded id map, so a new extension that
   * anchors at `context.after` jumps to Context without anyone editing this
   * file. `null` when its settings live on its own page. */
  const anchorTabOf = useCallback(
    (id: string): string | null =>
      (sections.find((s) => s.id === id)?.rows ?? []).find(
        (r) => r.anchor && (ANCHORS as readonly string[]).includes(r.anchor),
      )?.anchor ?? null,
    [sections],
  );

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

  // Covers for whatever is installed, in one call, whenever the set changes.
  const cardIds = cards.map((c) => c.id).join(",");
  useEffect(() => {
    if (!cardIds) return;
    let alive = true;
    invoke<{ id: string; sha256: string; kind: string }[]>("store_covers", {
      ids: cardIds.split(","),
    })
      .then(
        (list) =>
          alive &&
          setCovers(
            Object.fromEntries(
              list.map((c) => [c.id, { sha256: c.sha256, kind: c.kind }]),
            ),
          ),
      )
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [cardIds]);

  // Tell the hub when a detail page is open so it can drop its title, tab bar
  // and import button: inside an extension you are on THAT page, and the hub's
  // chrome above it just reads as leftover furniture.
  useEffect(() => {
    onDetailOpenChange?.(!!detail);
  }, [detail, onDetailOpenChange]);
  useEffect(() => () => onDetailOpenChange?.(false), [onDetailOpenChange]);

  // Load the open extension's catalogue metadata; drop it as soon as the detail
  // closes so nothing from the store stays in memory behind the list.
  useEffect(() => {
    if (!detail) {
      setDetailMeta(null);
      return;
    }
    let alive = true;
    invoke<StoreEntry | null>("store_entry", { id: detail })
      .then((e) => alive && setDetailMeta(e))
      .catch(() => alive && setDetailMeta(null));
    return () => {
      alive = false;
    };
  }, [detail]);

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

  // Built once and rendered in BOTH the list and the extension page, so an
  // enable toggle on either surface can raise the sheet it needs.
  const pendingModal = pending && (
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
  );

  const contestedModal = contested && (
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
  );

  if (detail) {
    const card = cards.find((c) => c.id === detail);
    // Catalogue metadata (cover, README, installs) for an extension that came
    // from the store; a local/built-in one simply has none and the header
    // renders from the card alone.
    const meta: DetailMeta = {
      id: detail,
      name: card?.name ?? openSection?.name ?? detail,
      description: card?.description ?? detailMeta?.description ?? "",
      version: card?.version ?? detailMeta?.version ?? "",
      tier: card?.tier ?? detailMeta?.tier ?? "pack",
      trust: card?.trust ?? detailMeta?.trust ?? "dev",
      repository: card?.repository ?? (detailMeta?.repo || null),
      installs: detailMeta?.installs ?? 0,
      readme: detailMeta?.readme ?? "",
      media: detailMeta?.media ?? [],
    };
    return (
      <>
        <ExtensionDetail
          meta={meta}
          onBack={() => setDetail(null)}
          installed={{
            enabled: card?.enabled ?? false,
            busy: busy === detail,
            onToggle: () => card && void toggle(card),
            section: openSection,
            ownRows: ownRows(detail),
            onChanged: () => void refresh(),
          }}
        />
        {pendingModal}
        {contestedModal}
      </>
    );
  }

  return (
    <div className="space-y-3">
      {error && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 text-red-600 text-sm">
          {error}
        </div>
      )}

      {/* NOTE: developer mode is NOT a row here. It is a property of Grain, not
          something you installed, and as a full-width card it sat above every
          user's extensions to be read once and ignored forever. It is now an
          icon in the Extensions header, and loading unpacked code lives in the
          Developer tab that icon reveals. */}

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
            {/* Each extension is its own card with air around it, rather than a
                row in one undivided slab. A list of separate things should look
                like separate things, and the height gives the picture room —
                which is what you actually recognise an extension by. */}
            <div className="space-y-2">
              {group.items.map((card) => (
                <div
                  key={card.id}
                  className="flex items-center gap-3.5 px-3 py-3 rounded-xl border border-line bg-paper-raised hover:border-ink-faint/40 transition-colors group"
                >
                  <RowCover
                    media={covers[card.id]}
                    name={card.name}
                    dim={!card.enabled}
                  />
                  <div className="flex-1 min-w-0 pe-4">
                    {/* Name and gear go to DIFFERENT places (SPEC §5.1). The
                        name means "take me to this extension's settings" — and
                        for one anchored at a feature (Voice Actions below
                        Snippets, App Modes below Context) that place is the
                        feature's tab, not a page of its own. The gear always
                        means "tell me about this extension". */}
                    <button
                      type="button"
                      onClick={() => {
                        const anchor = anchorTabOf(card.id);
                        if (anchor && onJump(anchor)) return;
                        if (onJump(card.id)) return;
                        setDetail(card.id);
                      }}
                      className="text-sm font-medium text-ink hover:text-accent transition-colors cursor-pointer"
                    >
                      {card.name}
                    </button>
                    {/* Stops short of the controls instead of running under
                        them — a description that reaches the right edge reads
                        as clipped rather than summarised. */}
                    <div
                      className="mt-0.5 text-xs text-ink-faint line-clamp-2 max-w-[46ch]"
                      title={card.description}
                    >
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
                  {/* NOTE: no tier/version chip. "SCRIPTED · V1.0.0" is
                      packaging trivia on a list whose job is "what do I have,
                      and is it on" — it lives on the detail page, where the
                      question is actually about the extension.

                      The gear is unconditional. It used to appear only when an
                      extension declared settings or shortcuts, which left a
                      pack like Agent center layout — one that contributes a
                      surface and nothing else — with no way in at all, and no
                      way to read what it is. Every installed extension has an
                      identity and a README to show, so every row opens.

                      Its repository is NOT here: it is one line down on that
                      page, and a link out of the app is a strange thing to
                      offer from a list whose job is "what do I have". */}
                  <button
                    type="button"
                    onClick={() => setDetail(card.id)}
                    className="text-ink-faint hover:text-ink transition-colors cursor-pointer"
                    aria-label={`About ${card.name}`}
                    title="About this extension"
                  >
                    <Sliders width={13} height={13} />
                  </button>
                  {/* Uninstall — everything except load-unpacked dev projects,
                      which unload from the Developer panel instead. A builtin
                      like Grain Space uninstalls too: its implementation is
                      compiled in, but the INSTALL is a real registry record, so
                      removing it switches the feature off and takes the row out
                      of the list. Its notes are not in the extension's storage,
                      so they survive; the store puts it back. */}
                  {card.trust !== "dev" && (
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

      {/* Permission sheet (SPEC §6) and takeover prompt (SPEC §3.2) — see the
          shared consts above; both also render on the extension page. */}
      {pendingModal}
      {contestedModal}
    </div>
  );
};

/** [GRAIN] One entry from the verified index (mirror of the Rust `StoreEntry`;
 * raw-invoke local type until a dev run regenerates bindings.ts). */
type StoreMedia = { sha256: string; kind: string };
type StoreEntry = {
  id: string;
  name: string;
  version: string;
  tier: string;
  trust: string;
  capabilities: string[];
  description: string;
  repo: string;
  size: string;
  author: string;
  reviewed_at: string;
  reviewed_commit: string;
  installs: number;
  readme: string;
  media: StoreMedia[];
  categories: string[];
  revocation: string | null;
  flags: string[];
};
type StoreView = {
  status: string; // "fresh" | "offline" | "needs-newer-client"
  can_install: boolean;
  entries: StoreEntry[];
};

/** [GRAIN] The trust rung, as shown ON the cover image (see the card below).
 * There is no "Core": the backend reports a first-party pack as `verified`,
 * because that is exactly what it promises the person installing it. */
const TRUST_BADGE: Record<string, { label: string; cls: string }> = {
  verified: {
    label: "Verified",
    cls: "bg-emerald-500/90 text-white",
  },
  experimental: {
    label: "Experimental",
    cls: "bg-amber-500/90 text-white",
  },
  dev: { label: "Community", cls: "bg-black/60 text-white" },
};

/** The filter row. Short on purpose: these answer "what KIND of thing is this",
 * which is the only question a filter can usefully ask before you have opened
 * anything. Mirrors `CATEGORIES` in grain-sdk, which is what a submission
 * declares and CI validates against. */
const CATEGORY_FILTERS: { key: string; label: string }[] = [
  { key: "all", label: "All" },
  { key: "visual", label: "Visual" },
  { key: "prompts", label: "Prompts" },
  { key: "dictation", label: "Dictation" },
  { key: "tools", label: "Tools" },
  { key: "installed", label: "Installed" },
];

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
  const [category, setCategory] = useState("all");
  const [installing, setInstalling] = useState<string | null>(null);
  /** The store entry whose detail page is open (null = the grid). */
  const [opened, setOpened] = useState<string | null>(null);
  /** How far down the window content starts — everything below the titlebar is
   * ours. Browsing a catalogue is a MODE, not a panel beside the settings you
   * were reading: it takes the whole window (the sidebar included) the way the
   * Quick Panel does, and "All extensions" puts you back exactly where you were.
   * Measured from the live DOM so it survives UI scaling. */
  const [top, setTop] = useState(36);

  useEffect(() => {
    const measure = () => {
      const bar = document
        .getElementById("grain-sidebar")
        ?.getBoundingClientRect();
      setTop(bar ? bar.top : 36);
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

  const all = view?.entries ?? [];
  const entries = all.filter((e) => {
    const q = query.trim().toLowerCase();
    const matchesQuery =
      !q ||
      e.name.toLowerCase().includes(q) ||
      e.id.toLowerCase().includes(q) ||
      e.description.toLowerCase().includes(q);
    const matchesCategory =
      category === "all" ||
      (category === "installed"
        ? installed[e.id] != null
        : e.categories.includes(category));
    return matchesQuery && matchesCategory;
  });

  /** Counts sit on the chips, so an empty filter is visibly empty before you
   * click it rather than after. */
  const countFor = (key: string) =>
    key === "all"
      ? all.length
      : key === "installed"
        ? all.filter((e) => installed[e.id] != null).length
        : all.filter((e) => e.categories.includes(key)).length;

  /** The opened store entry — the SAME unified detail the installed list uses,
   * so an extension's picture, words, and links are authored and read once. */
  const openEntry = entries.find((e) => e.id === opened) ?? null;
  const openLabel = openEntry
    ? installing === openEntry.id
      ? "Installing…"
      : installed[openEntry.id] === openEntry.version
        ? "Installed"
        : installed[openEntry.id] != null
          ? "Update"
          : "Install"
    : "";

  return createPortal(
    // [GRAIN] Portaled to <body> and pinned to the whole window below the
    // MEASURED titlebar, so the catalogue gets the full width to lay out in
    // regardless of the app's UI-scale transform.
    <div
      className="fixed left-0 right-0 bottom-0 z-40 bg-paper flex flex-col"
      style={{ top }}
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

      {!openEntry && (
        <div className="px-6 pt-6 pb-4 border-b border-line">
          <div className="max-w-[1600px] mx-auto space-y-4">
            <div>
              <h1 className="text-[1.7rem] font-semibold tracking-tight leading-none text-ink">
                Extension store
              </h1>
              <p className="mt-2 text-sm text-ink-soft max-w-2xl leading-relaxed">
                Everything here is built from pinned source by our own CI and
                signed before it reaches you.
              </p>
            </div>
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search extensions"
              className="w-full px-3.5 py-2.5 rounded-xl bg-paper-raised border border-line text-sm text-ink placeholder:text-ink-faint focus:outline-none focus:border-ink-faint"
            />
            {/* One row of chips, no sort controls. With a catalogue this size a
                sort is a control that changes nothing; the filter is the part
                that answers a real question. */}
            <div className="flex flex-wrap items-center gap-1.5">
              {CATEGORY_FILTERS.map((f) => {
                const n = countFor(f.key);
                const active = category === f.key;
                return (
                  <button
                    key={f.key}
                    type="button"
                    disabled={n === 0 && f.key !== "all"}
                    onClick={() => setCategory(f.key)}
                    className={`px-3 py-1.5 rounded-full text-xs font-medium border transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
                      active
                        ? "border-ink bg-ink text-paper"
                        : "border-line text-ink-soft hover:text-ink hover:border-ink-faint cursor-pointer"
                    }`}
                  >
                    {f.label}
                    <span className={active ? "opacity-60" : "text-ink-faint"}>
                      {" "}
                      ({n})
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-6 py-5">
        {/* An opened entry replaces the grid with the SAME detail component the
            installed list uses — one header, one set of authored content. */}
        {openEntry ? (
          <div className="max-w-3xl mx-auto">
            <ExtensionDetail
              meta={{
                id: openEntry.id,
                name: openEntry.name,
                description: openEntry.description,
                version: openEntry.version,
                tier: openEntry.tier,
                trust: openEntry.trust,
                repository: openEntry.repo || null,
                installs: openEntry.installs,
                readme: openEntry.readme,
                media: openEntry.media,
              }}
              onBack={() => setOpened(null)}
              install={{
                label: openLabel,
                disabled:
                  installing === openEntry.id ||
                  openEntry.revocation === "revoked" ||
                  installed[openEntry.id] === openEntry.version ||
                  !view?.can_install,
                onInstall: () => void install(openEntry),
              }}
            />
          </div>
        ) : (
        <>
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
        {/* The full window buys another column before the cards get wide enough
            to look stretched; the max-width stops the grid running away on a
            very wide display. */}
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 max-w-[1600px] mx-auto">
          {entries.map((e) => {
            const badge = TRUST_BADGE[e.trust] ?? TRUST_BADGE.dev;
            const revoked = e.revocation === "revoked";
            const deprecated = e.revocation === "deprecated";
            return (
              <div
                key={`${e.id}@${e.version}`}
                className="group rounded-xl border border-line bg-paper-raised overflow-hidden flex flex-col hover:border-ink-faint/50 transition-colors"
              >
                {/* Cover image on top — the card's most important element. The
                    trust rung rides ON it: it is the one thing you want to know
                    before reading anything, and up here it costs no height and
                    never competes with the name for the eye. */}
                <button
                  type="button"
                  onClick={() => setOpened(e.id)}
                  className="relative block w-full text-left cursor-pointer"
                  aria-label={`Open ${e.name}`}
                >
                  {e.media.length > 0 ? (
                    <Cover media={e.media[0]} name={e.name} rounded="rounded-none" />
                  ) : (
                    <div className="w-full aspect-[16/9] bg-paper-sunken flex items-center justify-center border-b border-line">
                      <Package width={22} height={22} className="text-ink-faint/50" />
                    </div>
                  )}
                  <span
                    className={`absolute top-2 right-2 inline-flex items-center gap-1 px-2 py-1 rounded-lg text-[10px] font-semibold backdrop-blur-sm ${badge.cls}`}
                  >
                    {e.trust === "verified" && (
                      <ShieldCheck width={10} height={10} />
                    )}
                    {badge.label}
                  </span>
                </button>

                {/* Deliberately shallow: the cover carries the card, and every
                    detail (author, version, review date, capabilities, README)
                    is one click away. A tall block of small grey metadata under
                    each title made the grid heavy and told nobody anything they
                    could act on. Title · trust · installs on ONE line, the
                    description, the button. */}
                <div className="p-3 flex flex-col gap-1.5 flex-1">
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0 flex items-center gap-1.5 flex-wrap">
                    <button
                      type="button"
                      onClick={() => setOpened(e.id)}
                      className="text-sm font-medium text-ink truncate hover:text-accent transition-colors cursor-pointer text-left"
                    >
                      {e.name}
                    </button>
                    {e.installs > 0 && (
                      <span className="inline-flex items-center gap-0.5 text-[11px] text-ink-faint">
                        <Download width={10} height={10} />
                        {fmtStars(e.installs)}
                      </span>
                    )}
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

                {/* The description carries the install decision — a name alone
                    is too vague to install from (DISTRIBUTION-PLAN §2.3). */}
                {e.description && (
                  <p className="text-xs text-ink-soft leading-relaxed line-clamp-2">
                    {e.description}
                  </p>
                )}

                {/* Flagged combinations (§3.3): what the reviewer was warned of.
                    These STAY on the card — unlike a review date, a flag is a
                    reason not to click Install. */}
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
              </div>
            );
          })}
        </div>
        </>
        )}
      </div>
    </div>,
    document.body,
  );
};
