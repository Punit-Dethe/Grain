import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CalendarDays,
  ChevronRight,
  FolderPlus,
  Hash,
  Search,
  Settings,
  SquarePen,
  X,
} from "lucide-react";
import type { Note, NoteCard } from "@/bindings";
import {
  clampRecentNotesVisibleCount,
  RECENT_NOTES_PAGE_SIZE,
  revealMoreRecentNotes,
} from "./recentNotesPagination";

/**
 * [GRAIN] The Notes rail (NOTES-TAB-PLAN.md Phase B).
 *
 * Search at the top, then FOLDERS, then the loose notes, then reminders, then
 * Calendar and Settings pinned to the bottom. That order is the one users asked
 * for and it is also the honest one: folders are where you put things, loose
 * notes are what you have not put anywhere yet, so a new folder appears above
 * and a new note appears below.
 *
 * The search box lives here rather than in a strip over the editor because this
 * is where its results render. Moving it down also let the editor have that
 * strip's height back, which matters now the window can be any size.
 */

const dayFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
});

/** How many loose notes / folder members to show before "See all". */
const PREVIEW = 8;

const NEXT_NOTES_COPY = {
  title: "Notes",
  subtitle: "Local knowledge space",
  recent: "Recent",
  collections: "Collections",
  viewMore: "View more",
} as const;

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

/** A node in the folder tree. */
type FolderNode = {
  name: string;
  path: string;
  notes: NoteCard[];
  children: Map<string, FolderNode>;
};

function emptyNode(name: string, path: string): FolderNode {
  return { name, path, notes: [], children: new Map() };
}

/** Walk to (creating) the node at `path`, returning it. */
function nodeAt(root: FolderNode, path: string): FolderNode {
  let node = root;
  let acc = "";
  for (const seg of path.split("/")) {
    if (!seg) continue;
    acc = acc ? `${acc}/${seg}` : seg;
    let child = node.children.get(seg);
    if (!child) {
      child = emptyNode(seg, acc);
      node.children.set(seg, child);
    }
    node = child;
  }
  return node;
}

/**
 * The folder tree, from BOTH the cards' folder paths and the backend's folder
 * listing.
 *
 * Both are needed. Cards alone miss a folder you just created (it holds no notes
 * yet, so nothing would be there to drag onto). The listing alone misses nothing
 * in principle, but the cards are what arrive first and what a move updates, so
 * building from both means the tree is never briefly wrong.
 */
function buildTree(cards: NoteCard[], folders: readonly string[]): FolderNode {
  const root = emptyNode("", "");
  for (const path of folders) nodeAt(root, path);
  for (const card of cards) {
    if (!card.folder) continue;
    nodeAt(root, card.folder).notes.push(card);
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
  variant?: "default" | "next";
  cards: NoteCard[];
  /** Every Grain subfolder, including empty ones (grain_space_list_all_folders). */
  folders: readonly string[];
  reminders: NoteCard[];
  results: Note[];
  selectedId: string | null;
  calendarOpen: boolean;
  query: string;
  onQueryChange: (q: string) => void;
  onOpenCalendar: () => void;
  onOpenAll: () => void;
  onOpenSettings: () => void;
  onSelectCard: (card: NoteCard) => void;
  onSelectResult: (note: Note) => void;
  onCreate: () => void;
  onCreateFolder: (name: string) => void;
  /** File a note into a folder — `null` moves it back out to the Grain root. */
  onMoveNote: (id: string, folder: string | null) => void;
};

export function Sidebar({
  variant = "default",
  cards,
  folders,
  reminders,
  results,
  selectedId,
  calendarOpen,
  query,
  onQueryChange,
  onOpenCalendar,
  onOpenSettings,
  onSelectCard,
  onSelectResult,
  onCreate,
  onCreateFolder,
  onMoveNote,
}: Props) {
  const { t } = useTranslation();
  const searching = query.trim().length > 0;

  // Section collapse + per-folder expand + "see all" are rail-local UI.
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [openFolders, setOpenFolders] = useState<ReadonlySet<string>>(
    new Set(),
  );
  const [seeAll, setSeeAll] = useState<Record<string, boolean>>({});
  /** The new-folder field, when open. Inline rather than a dialog: naming a
   *  folder is one word, and a modal for one word is a ceremony. */
  const [newFolder, setNewFolder] = useState<string | null>(null);
  const [recentVisibleCount, setRecentVisibleCount] = useState(
    RECENT_NOTES_PAGE_SIZE,
  );

  // Drag state. The note id rides in a ref as well as the dataTransfer, because
  // `dragover` is not allowed to read the payload — and the drop target has to
  // know whether to highlight before the drop happens.
  const draggingRef = useRef<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);

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
  // Loose notes are split by a divider: Grain-authored above, notes authored in
  // Obsidian inside the Grain folder below (no `grain_id` yet — `readonly` flags
  // them, though all are editable).
  const grainLoose = cards.filter(
    (c) => !c.folder && !c.is_pinned && !c.readonly,
  );
  const obsidianLoose = cards.filter(
    (c) => !c.folder && !c.is_pinned && c.readonly,
  );
  const tree = useMemo(() => buildTree(cards, folders), [cards, folders]);
  const topFolders = [...tree.children.values()].sort((a, b) =>
    a.name.localeCompare(b.name),
  );
  const folderCount = useMemo(
    () => topFolders.reduce((n, f) => n + subtreeCount(f), 0),
    [topFolders],
  );
  const recentCards = useMemo(
    () => [...cards].sort((a, b) => b.timestamp - a.timestamp),
    [cards],
  );
  const visibleRecentCount = clampRecentNotesVisibleCount(
    recentVisibleCount,
    recentCards.length,
  );

  useEffect(() => {
    if (recentVisibleCount !== visibleRecentCount) {
      setRecentVisibleCount(visibleRecentCount || RECENT_NOTES_PAGE_SIZE);
    }
  }, [recentVisibleCount, visibleRecentCount]);

  /** Card of the note being dragged, so a no-op move can be declined. */
  const dragFolderOf = (id: string): string | null =>
    cards.find((c) => c.id === id)?.folder ?? null;

  const beginDrag = (id: string) => (e: React.DragEvent) => {
    draggingRef.current = id;
    e.dataTransfer.setData("text/plain", id);
    e.dataTransfer.effectAllowed = "move";
  };

  const endDrag = () => {
    draggingRef.current = null;
    setDropTarget(null);
  };

  /** Drop-zone handlers for a folder path (`null` = the Grain root). */
  const dropZone = (folder: string | null) => {
    const key = folder ?? " root";
    return {
      onDragOver: (e: React.DragEvent) => {
        const id = draggingRef.current;
        // Refusing the no-op keeps the highlight honest: no drop indicator on
        // the folder the note is already in.
        if (!id || dragFolderOf(id) === folder) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        setDropTarget(key);
      },
      onDragLeave: () => {
        setDropTarget((current) => (current === key ? null : current));
      },
      onDrop: (e: React.DragEvent) => {
        e.preventDefault();
        const id = draggingRef.current ?? e.dataTransfer.getData("text/plain");
        endDrag();
        if (!id || dragFolderOf(id) === folder) return;
        onMoveNote(id, folder);
      },
      "data-drop": dropTarget === key ? "on" : undefined,
    };
  };

  const cardRow = (card: NoteCard, depth = 0) => (
    <button
      key={card.id}
      type="button"
      draggable
      onDragStart={beginDrag(card.id)}
      onDragEnd={endDrag}
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
    const count = subtreeCount(node);
    return (
      <div key={node.path}>
        <button
          type="button"
          className="gs-row gs-row--folder"
          style={{ paddingLeft: 8 + depth * 16 }}
          onClick={() => toggleFolder(node.path)}
          {...dropZone(node.path)}
        >
          <span className={`gs-row-chev${open ? " gs-row-chev--open" : ""}`}>
            <ChevronRight width={13} height={13} />
          </span>
          <span className="gs-row-hash">
            <Hash width={12} height={12} />
          </span>
          <span className="gs-row-title">{node.name}</span>
          {count > 0 && <span className="gs-row-count">{count}</span>}
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

  /** Section heading — a disclosure chevron, a quiet label, a count, and an
   *  optional action. The label is its own button so the action can be one too
   *  (a button cannot contain a button). */
  const sectionHead = (
    key: string,
    label: string,
    count?: number,
    action?: { icon: React.ReactNode; title: string; onClick: () => void },
  ) => (
    <div className="gs-section-row">
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
      {action && (
        <button
          type="button"
          className="gs-section-action"
          title={action.title}
          aria-label={action.title}
          onClick={action.onClick}
        >
          {action.icon}
        </button>
      )}
    </div>
  );

  const submitNewFolder = () => {
    const name = (newFolder ?? "").trim();
    setNewFolder(null);
    if (name) onCreateFolder(name);
  };

  if (variant === "next") {
    const recent = recentCards.slice(0, visibleRecentCount);
    return (
      <aside className="gs-side gs-next-side">
        <div className="gs-next-pane-head">
          <div>
            <strong>{NEXT_NOTES_COPY.title}</strong>
            <span>{NEXT_NOTES_COPY.subtitle}</span>
          </div>
        </div>
        <div className="gs-side-head">
          <div className="gs-search">
            <Search width={13} height={13} />
            <input
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder={t("grainSpaceOverlay.searchPlaceholder")}
              spellCheck={false}
            />
            {query && (
              <button
                type="button"
                className="gs-search-clear"
                title="Clear search"
                aria-label="Clear search"
                onClick={() => onQueryChange("")}
              >
                <X width={11} height={11} />
              </button>
            )}
          </div>
        </div>

        <nav className="gs-nav" aria-label="Note views and collections">
          <div className="gs-next-primary-nav">
            <button
              type="button"
              className={`gs-next-nav-row${calendarOpen ? " is-active" : ""}`}
              onClick={onOpenCalendar}
            >
              <CalendarDays width={15} height={15} />
              <strong>{t("grainSpaceOverlay.calendar")}</strong>
            </button>
            <button
              type="button"
              className="gs-next-nav-row"
              onClick={onCreate}
            >
              <SquarePen width={15} height={15} />
              <strong>{t("grainSpaceOverlay.createNote")}</strong>
            </button>
          </div>

          {searching ? (
            <>
              <div className="gs-next-group-label">
                {t("grainSpaceOverlay.results")}
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
                  className={`gs-row gs-next-note-row${selectedId === note.id ? " gs-row--on" : ""}`}
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
              <div className="gs-next-group-row" {...dropZone(null)}>
                <span>{NEXT_NOTES_COPY.collections}</span>
                <button
                  type="button"
                  title={t("grainSpaceOverlay.newFolder")}
                  aria-label={t("grainSpaceOverlay.newFolder")}
                  onClick={() => setNewFolder("")}
                >
                  <FolderPlus width={14} height={14} />
                </button>
              </div>
              {newFolder != null && (
                <div className="gs-newfolder">
                  <Hash width={12} height={12} />
                  <input
                    autoFocus
                    value={newFolder}
                    placeholder={t("grainSpaceOverlay.newFolderPlaceholder")}
                    spellCheck={false}
                    onChange={(event) => setNewFolder(event.target.value)}
                    onBlur={submitNewFolder}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        submitNewFolder();
                      } else if (event.key === "Escape") {
                        event.preventDefault();
                        setNewFolder(null);
                      }
                    }}
                  />
                </div>
              )}
              {topFolders.map((node) => renderFolder(node, 0))}
              {topFolders.length === 0 && newFolder == null && (
                <div className="gs-nav-hint">
                  {t("grainSpaceOverlay.noFolders")}
                </div>
              )}

              <div className="gs-next-group-label">
                {NEXT_NOTES_COPY.recent}
              </div>
              {recent.length === 0 ? (
                <div className="gs-nav-hint">
                  {t("grainSpaceOverlay.emptyList")}
                </div>
              ) : (
                recent.map((card) => cardRow(card))
              )}
              {visibleRecentCount < recentCards.length && (
                <button
                  type="button"
                  className="gs-seeall"
                  onClick={() =>
                    setRecentVisibleCount((current) =>
                      revealMoreRecentNotes(current, recentCards.length),
                    )
                  }
                >
                  {NEXT_NOTES_COPY.viewMore}
                </button>
              )}
            </>
          )}
        </nav>

        <RemindersDock reminders={reminders} onSelectCard={onSelectCard} />
        <div className="gs-side-foot">
          <button
            type="button"
            className="gs-foot-btn"
            onClick={onOpenSettings}
            title={t("grainSpaceOverlay.settings")}
          >
            <Settings width={13} height={13} />
            <span>{t("grainSpaceOverlay.settings")}</span>
          </button>
        </div>
      </aside>
    );
  }

  return (
    <aside className="gs-side">
      {/* One search box and no mode switch. The old Exact / Semantic pair asked a
          question the user cannot answer, and was not even a real choice —
          "semantic" already fused the lexical leg. Search now uses whichever legs
          are available; see `grain_space_search`. */}
      <div className="gs-side-head">
        <div className="gs-search">
          <Search width={12} height={12} />
          <input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder={t("grainSpaceOverlay.searchPlaceholder")}
            spellCheck={false}
          />
          {query && (
            <button
              type="button"
              className="gs-search-clear"
              title="Clear search"
              aria-label="Clear search"
              onClick={() => onQueryChange("")}
            >
              <X width={11} height={11} />
            </button>
          )}
        </div>
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
              "folders",
              t("grainSpaceOverlay.collections"),
              folderCount,
              {
                icon: <FolderPlus width={13} height={13} />,
                title: t("grainSpaceOverlay.newFolder"),
                onClick: () => setNewFolder(""),
              },
            )}
            {!collapsed.folders && (
              <>
                {newFolder != null && (
                  <div className="gs-newfolder">
                    <Hash width={12} height={12} />
                    <input
                      autoFocus
                      value={newFolder}
                      placeholder={t("grainSpaceOverlay.newFolderPlaceholder")}
                      spellCheck={false}
                      onChange={(e) => setNewFolder(e.target.value)}
                      onBlur={submitNewFolder}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          submitNewFolder();
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          setNewFolder(null);
                        }
                      }}
                    />
                  </div>
                )}
                {topFolders.length === 0 && newFolder == null && (
                  <div className="gs-nav-hint">
                    {t("grainSpaceOverlay.noFolders")}
                  </div>
                )}
                {topFolders.map((node) => renderFolder(node, 0))}
              </>
            )}

            {sectionHead(
              "notes",
              t("grainSpaceOverlay.notes"),
              grainLoose.length + obsidianLoose.length,
              {
                icon: <SquarePen width={13} height={13} />,
                title: t("grainSpaceOverlay.createNote"),
                onClick: onCreate,
              },
            )}
            {!collapsed.notes && (
              // Also the drop zone that takes a note back OUT of a folder: the
              // loose list is literally "notes in no folder", so dragging one
              // here is the move it looks like.
              <div className="gs-drop-root" {...dropZone(null)}>
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
              </div>
            )}
          </>
        )}
      </nav>

      <RemindersDock reminders={reminders} onSelectCard={onSelectCard} />

      <div className="gs-side-foot">
        <button
          type="button"
          className={`gs-foot-btn${calendarOpen ? " gs-foot-btn--on" : ""}`}
          onClick={onOpenCalendar}
        >
          <CalendarDays width={13} height={13} />
          <span>{t("grainSpaceOverlay.calendar")}</span>
        </button>
        <button
          type="button"
          className="gs-foot-btn"
          onClick={onOpenSettings}
          title={t("grainSpaceOverlay.settings")}
        >
          <Settings width={13} height={13} />
          <span>{t("grainSpaceOverlay.settings")}</span>
        </button>
      </div>
    </aside>
  );
}
