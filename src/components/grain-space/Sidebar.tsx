import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CalendarDays, ChevronRight, Hash, SquarePen } from "lucide-react";
import type { Note, NoteCard } from "@/bindings";

/**
 * [GRAIN] Workspace sidebar — premium three-section layout. A profile header,
 * a Create-note button, then BIG section headings (Reminders / Pinned /
 * Notes / Collections) each with their own colored icon tile on the left and
 * a collapse chevron on the right. Sections read as distinct areas, not a
 * folder tree. While a search runs the sections give way to a flat result
 * list in relevance order.
 */

const dayFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
});

/** How many loose notes / folder members to show before "See all". */
const PREVIEW = 8;

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

/** Signed compact label for a reminder's fire time ("2h", "3d ago", "12 Aug"). */
function fireLabel(fireAt: number): string {
  const diff = fireAt - Date.now();
  const past = diff < 0;
  const mins = Math.round(Math.abs(diff) / 60000);
  const suffix = (s: string) => (past ? `${s} ago` : s);
  if (mins < 1) return "now";
  if (mins < 60) return suffix(`${mins}m`);
  const hours = Math.round(mins / 60);
  if (hours < 24) return suffix(`${hours}h`);
  const days = Math.round(hours / 24);
  if (days < 7) return suffix(`${days}d`);
  return dayFormat.format(new Date(fireAt));
}

/** A node in the folder tree built from card `folder` paths. */
type FolderNode = {
  name: string;
  path: string;
  notes: NoteCard[];
  children: Map<string, FolderNode>;
};

function buildTree(cards: NoteCard[]): FolderNode {
  const root: FolderNode = {
    name: "",
    path: "",
    notes: [],
    children: new Map(),
  };
  for (const card of cards) {
    if (!card.folder) continue;
    let node = root;
    let acc = "";
    for (const seg of card.folder.split("/")) {
      if (!seg) continue;
      acc = acc ? `${acc}/${seg}` : seg;
      let child = node.children.get(seg);
      if (!child) {
        child = { name: seg, path: acc, notes: [], children: new Map() };
        node.children.set(seg, child);
      }
      node = child;
    }
    node.notes.push(card);
  }
  return root;
}

/** Total notes under a folder subtree (for the count badge). */
function subtreeCount(node: FolderNode): number {
  let n = node.notes.length;
  for (const child of node.children.values()) n += subtreeCount(child);
  return n;
}

/** Two reminders at rest; expands to a scrollable ~6-row list. */
const DOCK_REST = 2;

function RemindersDock({
  reminders,
  onSelectCard,
}: {
  reminders: NoteCard[];
  onSelectCard: (card: NoteCard) => void;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  if (reminders.length === 0) return null;
  const shown = expanded ? reminders : reminders.slice(0, DOCK_REST);
  const now = Date.now();
  return (
    <div className="gs-dock">
      <div className="gs-dock-head">
        <span className="gs-dock-title">
          {t("grainSpaceOverlay.reminders")}
        </span>
        {reminders.length > DOCK_REST && (
          <button
            type="button"
            className="gs-dock-toggle"
            onClick={() => setExpanded((v) => !v)}
          >
            {expanded
              ? t("grainSpaceOverlay.seeLess")
              : `+${reminders.length - DOCK_REST}`}
          </button>
        )}
      </div>
      <div className={`gs-dock-list${expanded ? " gs-dock-list--exp" : ""}`}>
        {shown.map((r) => {
          const at = r.reminder_state.fire_at ?? 0;
          return (
            <button
              key={r.id}
              type="button"
              className="gs-dock-item"
              onClick={() => onSelectCard(r)}
              title={r.title.trim() || t("grainSpaceOverlay.untitled")}
            >
              <span
                className={`gs-dock-dot${at < now ? " gs-dock-dot--past" : ""}`}
              />
              <span className="gs-dock-item-title">
                {r.title.trim() || t("grainSpaceOverlay.untitled")}
              </span>
              <span className="gs-dock-item-when">{fireLabel(at)}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

type Props = {
  cards: NoteCard[];
  reminders: NoteCard[];
  searching: boolean;
  results: Note[];
  selectedId: string | null;
  calendarOpen: boolean;
  onOpenCalendar: () => void;
  onSelectCard: (card: NoteCard) => void;
  onSelectResult: (note: Note) => void;
  onCreate: () => void;
};

export function Sidebar({
  cards,
  reminders,
  searching,
  results,
  selectedId,
  calendarOpen,
  onOpenCalendar,
  onSelectCard,
  onSelectResult,
  onCreate,
}: Props) {
  const { t } = useTranslation();

  // Section collapse + per-folder expand + "see all" are sidebar-local UI.
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [openFolders, setOpenFolders] = useState<ReadonlySet<string>>(
    new Set(),
  );
  const [seeAll, setSeeAll] = useState<Record<string, boolean>>({});

  const toggleSection = (key: string) =>
    setCollapsed((c) => ({ ...c, [key]: !c[key] }));
  const toggleFolder = (path: string) =>
    setOpenFolders((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const pinned = cards.filter((c) => c.is_pinned);
  const grainLoose = cards.filter(
    (c) => !c.folder && !c.is_pinned && !c.readonly,
  );
  const obsidianLoose = cards.filter(
    (c) => !c.folder && !c.is_pinned && c.readonly,
  );
  const tree = useMemo(() => buildTree(cards), [cards]);
  const topFolders = [...tree.children.values()].sort((a, b) =>
    a.name.localeCompare(b.name),
  );
  const collectionCount = useMemo(
    () => topFolders.reduce((n, f) => n + subtreeCount(f), 0),
    [topFolders],
  );

  const cardRow = (card: NoteCard, depth = 0) => (
    <button
      key={card.id}
      type="button"
      className={`gs-row${selectedId === card.id ? " gs-row--on" : ""}`}
      style={depth ? { paddingLeft: 12 + depth * 16 } : undefined}
      onClick={() => onSelectCard(card)}
      title={card.title.trim() || t("grainSpaceOverlay.untitled")}
    >
      <span className="gs-row-title">
        {card.title.trim() || t("grainSpaceOverlay.untitled")}
      </span>
      <span className="gs-row-age">{age(card.timestamp)}</span>
    </button>
  );

  /** A "see all"-capped list of loose note rows. */
  const looseList = (key: string, list: NoteCard[]) => {
    const show = seeAll[key] ? list : list.slice(0, PREVIEW);
    return (
      <>
        {show.map((c) => cardRow(c))}
        {list.length > PREVIEW && (
          <button
            type="button"
            className="gs-seeall"
            onClick={() => setSeeAll((s) => ({ ...s, [key]: !s[key] }))}
          >
            {seeAll[key]
              ? t("grainSpaceOverlay.seeLess")
              : t("grainSpaceOverlay.seeAll")}
          </button>
        )}
      </>
    );
  };

  /** Recursive folder node: header row + (subfolders, then note members). */
  const renderFolder = (node: FolderNode, depth: number) => {
    const open = openFolders.has(node.path);
    const subs = [...node.children.values()].sort((a, b) =>
      a.name.localeCompare(b.name),
    );
    return (
      <div key={node.path}>
        <button
          type="button"
          className="gs-row gs-row--folder"
          style={{ paddingLeft: 8 + depth * 16 }}
          onClick={() => toggleFolder(node.path)}
        >
          <span className={`gs-row-chev${open ? " gs-row-chev--open" : ""}`}>
            <ChevronRight width={13} height={13} />
          </span>
          <span className="gs-row-hash">
            <Hash width={12} height={12} />
          </span>
          <span className="gs-row-title">{node.name}</span>
          <span className="gs-row-count">{subtreeCount(node)}</span>
        </button>
        {open && (
          <>
            {subs.map((child) => renderFolder(child, depth + 1))}
            {node.notes.map((c) => cardRow(c, depth + 1))}
          </>
        )}
      </div>
    );
  };

  /** Section heading — Mem style: a small disclosure chevron, a quiet label,
   * and a subtle count. No icon tiles; hierarchy reads through restraint.
   * Clicking toggles collapse. */
  const sectionHead = (key: string, label: string, count?: number) => (
    <button
      type="button"
      className="gs-section"
      onClick={() => toggleSection(key)}
    >
      <span
        className={`gs-section-chev${collapsed[key] ? " gs-section-chev--closed" : ""}`}
      >
        <ChevronRight width={12} height={12} />
      </span>
      <span className="gs-section-label">{label}</span>
      {count != null && count > 0 && (
        <span className="gs-section-count">{count}</span>
      )}
    </button>
  );

  return (
    <aside className="gs-side">
      <div className="gs-sidebar-brand" data-tauri-drag-region>
        {t("grainSpaceOverlay.brand")}
      </div>
      <div className="gs-sidebar-foot">
        <button
          type="button"
          className={`gs-nav-tab${calendarOpen ? " gs-nav-tab--on" : ""}`}
          onClick={onOpenCalendar}
        >
          <span className="gs-nav-tab-icon">
            <CalendarDays width={13} height={13} />
          </span>
          <span className="gs-nav-tab-label">
            {t("grainSpaceOverlay.calendar")}
          </span>
        </button>
        <button type="button" className="gs-create" onClick={onCreate}>
          <SquarePen width={15} height={15} />
          <span>{t("grainSpaceOverlay.createNote")}</span>
        </button>
      </div>

      <nav className="gs-nav">
        {searching ? (
          <>
            <div className="gs-section gs-section--static">
              <span className="gs-section-label">
                {t("grainSpaceOverlay.results")}
              </span>
            </div>
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
            {pinned.length > 0 &&
              sectionHead(
                "pinned",
                t("grainSpaceOverlay.pinned"),
                pinned.length,
              )}
            {pinned.length > 0 &&
              !collapsed.pinned &&
              pinned.map((c) => cardRow(c))}

            {sectionHead(
              "notes",
              t("grainSpaceOverlay.notes"),
              grainLoose.length + obsidianLoose.length,
            )}
            {!collapsed.notes && (
              <>
                {grainLoose.length === 0 && obsidianLoose.length === 0 && (
                  <div className="gs-nav-hint">
                    {t("grainSpaceOverlay.emptyList")}
                  </div>
                )}
                {looseList("grain", grainLoose)}
                {obsidianLoose.length > 0 && (
                  <>
                    <div className="gs-divider" />
                    {looseList("obsidian", obsidianLoose)}
                  </>
                )}
              </>
            )}

            {topFolders.length > 0 &&
              sectionHead(
                "folders",
                t("grainSpaceOverlay.collections"),
                collectionCount,
              )}
            {!collapsed.folders &&
              topFolders.map((node) => renderFolder(node, 0))}
          </>
        )}
      </nav>

      <RemindersDock reminders={reminders} onSelectCard={onSelectCard} />
    </aside>
  );
}
