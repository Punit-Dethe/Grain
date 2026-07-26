import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, NotebookPen } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";

/** The pack that carries the note window (NOTE-UI-EXTENSION-PLAN.md). */
const NOTE_UI_ID = "grain.note-ui";
/** Its contributed shortcut, namespaced the way every extension's is. */
const OPEN_SHORTCUT = `ext:${NOTE_UI_ID}:open`;

type Card = { id: string; enabled: boolean; version: string };
type StoreEntry = { id: string; version: string };

/**
 * [GRAIN] Install the note window without leaving this page.
 *
 * The store is the wrong place to send someone for this. They are already
 * looking at Grain Space, they have just been told the window is optional, and
 * the answer to "I do want one" should be a button — not a navigation, a search
 * and a second decision.
 *
 * The copy leads with the fact that it is optional, because it is: notes are
 * Markdown files, and Obsidian, an editor, Recall and the MCP bridge all read
 * them without this.
 */
export const NoteWindowRow: React.FC = () => {
  const [card, setCard] = useState<Card | null>(null);
  const [entry, setEntry] = useState<StoreEntry | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const cards = await invoke<Card[]>("extensions_overview").catch(() => []);
    setCard(cards.find((c) => c.id === NOTE_UI_ID) ?? null);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // The version to install comes from the signed index, never from a constant
  // here — a hard-coded version would go stale the first time the pack ships an
  // update, and would be a second place to remember to change. `store_entry`
  // reads the CACHED index and drops it again, so asking costs no network and
  // leaves nothing resident.
  useEffect(() => {
    if (card) return;
    let alive = true;
    void invoke<StoreEntry | null>("store_entry", { id: NOTE_UI_ID })
      .then((found) => alive && setEntry(found))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [card]);

  const install = async () => {
    if (!entry) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("store_install", { id: entry.id, version: entry.version });
      await invoke("extension_set_enabled", { id: entry.id, enabled: true });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("extension_uninstall", { id: NOTE_UI_ID, purge: false });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsGroup title="A window for your notes" info="Optional. Grain Space keeps notes as Markdown files, so you can also read them in Obsidian, an editor, or by asking Grain — none of which need this installed.">
      {card ? (
        <>
          <div className="flex items-center gap-3 px-4 py-3">
            <NotebookPen width={16} height={16} className="text-accent shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="text-sm text-ink">Note window</div>
              <div className="text-xs text-ink-faint">
                Installed · v{card.version}
              </div>
            </div>
            <button
              type="button"
              onClick={() => void remove()}
              disabled={busy}
              className="text-xs text-ink-faint hover:text-ink transition-colors cursor-pointer disabled:cursor-not-allowed"
            >
              Remove
            </button>
          </div>
          {/* Its own contributed shortcut, rendered by the same control every
              other shortcut uses — an extension's binding is not a lesser one. */}
          <div className="flex items-center gap-3 px-4 py-3 border-t border-line">
            <div className="flex-1 min-w-0 text-sm text-ink">Open it with</div>
            <ShortcutInput shortcutId={OPEN_SHORTCUT} />
          </div>
        </>
      ) : (
        <div className="flex items-center gap-3 px-4 py-3">
          <div className="flex-1 min-w-0">
            <div className="text-sm text-ink">Note window</div>
            <div className="text-xs text-ink-soft leading-relaxed mt-0.5">
              A list of your notes and an editor, in its own window.
            </div>
          </div>
          <button
            type="button"
            onClick={() => void install()}
            disabled={busy || !entry}
            className="shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold bg-ink text-paper hover:opacity-90 transition-opacity cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {busy && <Loader2 width={12} height={12} className="animate-spin" />}
            {busy ? "Installing…" : "Install"}
          </button>
        </div>
      )}

      {/* An install that cannot happen says why. Offline is the common case and
          reads as itself rather than as a generic failure. */}
      {!card && !entry && !busy && (
        <div className="px-4 pb-3 text-xs text-ink-faint">
          Couldn't reach the extension catalogue. Check your connection and
          reopen this page.
        </div>
      )}
      {error && <div className="px-4 pb-3 text-xs text-red-500">{error}</div>}
    </SettingsGroup>
  );
};
