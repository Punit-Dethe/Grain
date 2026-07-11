import { useTranslation } from "react-i18next";
import {
  AlarmClock,
  ChevronRight,
  FileText,
  Hash,
  PenLine,
  Pin,
} from "lucide-react";
import type { Note, NoteCard } from "@/bindings";

/**
 * [GRAIN] Workspace sidebar (TAURI-OVERLAY-PLAN.md Phase B): Create note, then
 * Reminders / Pinned / Notes / Collections. Collections are the note folders
 * of the active store (Obsidian folders on the vault backend), expandable in
 * place. While a search is running the sections give way to a flat result
 * list in relevance order.
 */

const dayFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
});

/** Compact relative age for row trailers. */
function age(ms: number): string {
  const mins = Math.floor((Date.now() - ms) / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  return dayFormat.format(new Date(ms));
}

type Props = {
  cards: NoteCard[];
  searching: boolean;
  results: Note[];
  selectedId: string | null;
  expanded: ReadonlySet<string>;
  onToggleCollection: (name: string) => void;
  onSelectCard: (card: NoteCard) => void;
  onSelectResult: (note: Note) => void;
  onCreate: () => void;
};

export function Sidebar({
  cards,
  searching,
  results,
  selectedId,
  expanded,
  onToggleCollection,
  onSelectCard,
  onSelectResult,
  onCreate,
}: Props) {
  const { t } = useTranslation();

  const reminders = cards.filter(
    (c) =>
      c.reminder_state.status === "armed" ||
      c.reminder_state.status === "fired",
  );
  const pinned = cards.filter((c) => c.is_pinned);
  const loose = cards.filter((c) => !c.collection && !c.is_pinned);
  const collections = new Map<string, NoteCard[]>();
  for (const card of cards) {
    if (!card.collection) continue;
    const list = collections.get(card.collection) ?? [];
    list.push(card);
    collections.set(card.collection, list);
  }
  const collectionNames = [...collections.keys()].sort((a, b) =>
    a.localeCompare(b),
  );

  const cardRow = (card: NoteCard, child = false) => (
    <button
      key={card.id}
      type="button"
      className={`gs-row${selectedId === card.id ? " gs-row--on" : ""}${child ? " gs-row--child" : ""}`}
      onClick={() => onSelectCard(card)}
    >
      <span className="gs-row-icon">
        {card.is_pinned ? (
          <Pin width={12} height={12} />
        ) : (
          <FileText width={12} height={12} />
        )}
      </span>
      <span className="gs-row-title">
        {card.title.trim() || t("grainSpaceOverlay.untitled")}
      </span>
      <span className="gs-row-age">{age(card.timestamp)}</span>
    </button>
  );

  return (
    <aside className="gs-side">
      <div className="gs-side-head" data-tauri-drag-region>
        <span className="gs-brand" data-tauri-drag-region>
          {t("grainSpaceOverlay.brand")}
        </span>
      </div>
      <button type="button" className="gs-create" onClick={onCreate}>
        <PenLine width={13} height={13} />
        <span>{t("grainSpaceOverlay.createNote")}</span>
      </button>

      <nav className="gs-nav">
        {searching ? (
          <>
            <div className="gs-section">{t("grainSpaceOverlay.results")}</div>
            {results.length === 0 && (
              <div className="gs-nav-empty">
                {t("grainSpaceOverlay.noMatches")}
              </div>
            )}
            {results.map((note) => (
              <button
                key={note.id}
                type="button"
                className={`gs-row${selectedId === note.id ? " gs-row--on" : ""}`}
                onClick={() => onSelectResult(note)}
              >
                <span className="gs-row-icon">
                  <FileText width={12} height={12} />
                </span>
                <span className="gs-row-title">
                  {note.title.trim() ||
                    note.body.split("\n")[0]?.trim() ||
                    t("grainSpaceOverlay.untitled")}
                </span>
                <span className="gs-row-age">{age(note.timestamp)}</span>
              </button>
            ))}
          </>
        ) : (
          <>
            {reminders.length > 0 && (
              <>
                <div className="gs-section">
                  <AlarmClock width={10} height={10} />
                  {t("grainSpaceOverlay.reminders")}
                </div>
                {reminders.map((c) => cardRow(c))}
              </>
            )}

            {pinned.length > 0 && (
              <>
                <div className="gs-section">
                  <Pin width={10} height={10} />
                  {t("grainSpaceOverlay.pinned")}
                </div>
                {pinned.map((c) => cardRow(c))}
              </>
            )}

            <div className="gs-section">{t("grainSpaceOverlay.notes")}</div>
            {loose.length === 0 && (
              <div className="gs-nav-empty">
                {t("grainSpaceOverlay.emptyList")}
              </div>
            )}
            {loose.map((c) => cardRow(c))}

            {collectionNames.length > 0 && (
              <>
                <div className="gs-section">
                  {t("grainSpaceOverlay.collections")}
                </div>
                {collectionNames.map((name) => {
                  const members = collections.get(name) ?? [];
                  const open = expanded.has(name);
                  return (
                    <div key={name}>
                      <button
                        type="button"
                        className="gs-row"
                        onClick={() => onToggleCollection(name)}
                      >
                        <span
                          className={`gs-row-chev${open ? " gs-row-chev--open" : ""}`}
                        >
                          <ChevronRight width={12} height={12} />
                        </span>
                        <span className="gs-row-icon">
                          <Hash width={12} height={12} />
                        </span>
                        <span className="gs-row-title">{name}</span>
                        <span className="gs-row-count">{members.length}</span>
                      </button>
                      {open && members.map((c) => cardRow(c, true))}
                    </div>
                  );
                })}
              </>
            )}
          </>
        )}
      </nav>
    </aside>
  );
}
