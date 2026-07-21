import React, { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink, Package, Store } from "lucide-react";

/** Mirror of the Rust `ExtensionCard` (grain_commands.rs). Local type until the
 * next dev run regenerates bindings.ts — never hand-edit bindings. */
interface ExtensionCard {
  id: string;
  name: string;
  description: string;
  version: string;
  tier: "builtin" | "pack";
  enabled: boolean;
  /** u64 as string; "18446744073709551615" (u64::MAX) = never toggled. */
  toggle_seq: string;
  repository: string | null;
}

const NEVER_TOGGLED = "18446744073709551615";

/** Toggle-order sort (SPEC §4.4): enabled first, each group by the order the
 * user enabled them in; never-toggled sorts last, stable by name. */
function sortCards(cards: ExtensionCard[]): ExtensionCard[] {
  const seq = (c: ExtensionCard) =>
    c.toggle_seq === NEVER_TOGGLED ? Number.MAX_SAFE_INTEGER : Number(c.toggle_seq);
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
}> = ({ onJump }) => {
  const [cards, setCards] = useState<ExtensionCard[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setCards(sortCards(await invoke<ExtensionCard[]>("extensions_overview")));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

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
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="space-y-3">
      {error && (
        <div className="px-3 py-2 rounded-lg bg-red-500/10 text-red-600 text-sm">
          {error}
        </div>
      )}

      <div className="rounded-xl border border-line bg-paper-raised divide-y divide-line">
        {cards.map((card) => (
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
                onClick={() => onJump(card.id)}
                className="text-sm font-medium text-ink hover:text-accent transition-colors cursor-pointer"
              >
                {card.name}
              </button>
              <div className="text-xs text-ink-faint truncate">
                {card.description}
              </div>
            </div>
            <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-paper-sunken text-ink-faint border border-line">
              {card.tier === "builtin" ? "built-in" : "pack"} · v{card.version}
            </span>
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
            {/* Inline enable toggle (first-enable permission sheet arrives with
                packs that actually request permissions). */}
            <button
              type="button"
              role="switch"
              aria-checked={card.enabled}
              disabled={busy === card.id}
              onClick={() => void toggle(card)}
              className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer ${
                card.enabled ? "bg-accent" : "bg-paper-sunken border border-line"
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
        {cards.length === 0 && !error && (
          <div className="px-4 py-6 text-sm text-ink-faint text-center">
            Loading extensions…
          </div>
        )}
      </div>

      {/* Store entry point (SPEC §5.3) — the slide-over ships with the
          marketplace phase; the affordance exists now so the layout is final. */}
      <button
        type="button"
        disabled
        title="The extension store arrives in a later update"
        className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-xl border border-dashed border-line text-sm text-ink-faint cursor-not-allowed"
      >
        <Store width={14} height={14} />
        Browse extensions — coming soon
      </button>
    </div>
  );
};
