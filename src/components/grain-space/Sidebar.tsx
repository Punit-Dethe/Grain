import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlarmClock,
  ChevronRight,
  FileText,
  Library,
  Pin,
  Search,
  SquarePen,
} from "lucide-react";
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

type Props = {
  cards: NoteCard[];
  searching: boolean;
  results: Note[];
  selectedId: string | null;
  storeLabel: string;
  onSelectCard: (card: NoteCard) => void;
  onSelectResult: (note: Note) => void;
  onCreate: () => void;
};

export function Sidebar({
  cards,
  searching,
  results,
  selectedId,
  storeLabel,
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

  const reminders = cards.filter(
    (c) =>
      c.reminder_state.status === "armed" ||
      c.reminder_state.status === "fired",
  );
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
      <span className="gs-row-icon">
        {card.is_pinned ? (
          <Pin width={13} height={13} />
        ) : (
          <FileText width={13} height={13} />
        )}
      </span>
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
            <FileText width={13} height={13} />
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

  /** Big section heading: colored icon tile (left) + label + count, chevron
   * flush to the right edge. Clicking toggles collapse. */
  const sectionHead = (
    key: string,
    label: string,
    iconClass: string,
    icon: React.ReactNode,
    count?: number,
  ) => (
    <button
      type="button"
      className="gs-section"
      onClick={() => toggleSection(key)}
    >
      <span className={`gs-section-ic ${iconClass}`}>{icon}</span>
      <span className="gs-section-label">{label}</span>
      {count != null && count > 0 && (
        <span className="gs-section-count">{count}</span>
      )}
      <span
        className={`gs-section-chev${collapsed[key] ? " gs-section-chev--closed" : ""}`}
      >
        <ChevronRight width={14} height={14} />
      </span>
    </button>
  );

  return (
    <aside className="gs-side">
      <div className="gs-profile" data-tauri-drag-region>
        <span className="gs-avatar">G</span>
        <div className="gs-profile-text">
          <span className="gs-profile-name">
            {t("grainSpaceOverlay.brand")}
          </span>
          <span className="gs-profile-sub">{storeLabel}</span>
        </div>
      </div>

      <button type="button" className="gs-create" onClick={onCreate}>
        <SquarePen width={15} height={15} />
        <span>{t("grainSpaceOverlay.createNote")}</span>
      </button>

      <nav className="gs-nav">
        {searching ? (
          <>
            <div className="gs-section gs-section--static">
              <span className="gs-section-ic gs-section-ic--results">
                <Search width={13} height={13} />
              </span>
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
                <span className="gs-row-icon">
                  <FileText width={13} height={13} />
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
            {reminders.length > 0 &&
              sectionHead(
                "reminders",
                t("grainSpaceOverlay.reminders"),
                "gs-section-ic--reminders",
                <AlarmClock width={13} height={13} />,
                reminders.length,
              )}
            {!collapsed.reminders && reminders.length > 0 && (
              <>{reminders.map((c) => cardRow(c))}</>
            )}

            {sectionHead(
              "pinned",
              t("grainSpaceOverlay.pinned"),
              "gs-section-ic--pinned",
              <Pin width={13} height={13} />,
              pinned.length,
            )}
            {!collapsed.pinned &&
              (pinned.length > 0 ? (
                pinned.map((c) => cardRow(c))
              ) : (
                <div className="gs-nav-hint">
                  {t("grainSpaceOverlay.pinnedHint")}
                </div>
              ))}

            {sectionHead(
              "notes",
              t("grainSpaceOverlay.notes"),
              "gs-section-ic--notes",
              <FileText width={13} height={13} />,
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
                    <div className="gs-divider">
                      <span>{t("grainSpaceOverlay.fromVault")}</span>
                    </div>
                    {looseList("obsidian", obsidianLoose)}
                  </>
                )}
              </>
            )}

            {topFolders.length > 0 &&
              sectionHead(
                "folders",
                t("grainSpaceOverlay.collections"),
                "gs-section-ic--collections",
                <Library width={13} height={13} />,
                collectionCount,
              )}
            {!collapsed.folders &&
              topFolders.map((node) => renderFolder(node, 0))}
          </>
        )}
      </nav>
    </aside>
  );
}
