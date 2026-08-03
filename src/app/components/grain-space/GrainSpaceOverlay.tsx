import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { MessageSquare, Settings, Sparkles } from "lucide-react";
import { commands, type Note, type NoteCard } from "@/bindings";
import { useTheme } from "@/contexts/ThemeContext";
import { Sidebar } from "./Sidebar";
import { EditorPane } from "./EditorPane";
import { CalendarView } from "./CalendarView";
import { ChatRail } from "./ChatRail";
import { createBlankDraft, resolveNotesLanding } from "./notesLanding";
import "./grain-space.css";

/** Backend events (see src-tauri/src/grain_space). */
const NOTES_CHANGED_EVENT = "grain-space://notes-changed";
const CORPUS_CHANGED_EVENT = "grain-space://corpus-changed";
const FOCUS_NOTE_EVENT = "grain-space://focus-note";

/**
 * [GRAIN] Search is STAGED, and the two delays are the whole design.
 *
 * The lexical pass is a SQLite FTS query — milliseconds — so it fires almost
 * immediately and the list moves while you type. The hybrid pass may have to spawn
 * the embedding model (seconds, the first time after the tab mounts), so it waits
 * for a real pause and then supersedes the lexical results in place.
 *
 * Firing only the hybrid call would freeze the box on the first query of a session.
 * Firing only the lexical one would throw away the meaning leg. Staging gives the
 * instant feel of the first and the ranking of the second.
 */
const LEXICAL_DEBOUNCE_MS = 140;
const HYBRID_DEBOUNCE_MS = 380;

/**
 * [GRAIN] The Grain Note workspace (NOTES-TAB-PLAN.md): a Mem/Obsidian-style
 * two-pane surface — a folder/notes rail and a markdown editor sheet — rendered
 * as the **Notes tab of the main window**. The rail lists light `NoteCard`s (no
 * bodies); the full note loads on select. On the vault backend the whole vault
 * appears — the store's folders ARE the collections — and foreign files open
 * read-only.
 *
 * It used to be its own frameless window with a sleep/revive handshake to
 * reclaim idle RAM. It does not need one: `App.tsx` keys the section container
 * on the active tab, so leaving Notes unmounts this whole tree — including the
 * lazily-loaded editor chunk's instances — which is the same DOM purge the sleep
 * handshake performed, for free. What that buys is also what makes the unmount
 * flush below load-bearing.
 */
export function GrainSpaceOverlay({
  onOpenSettings,
  variant = "default",
}: {
  /** Show the notebook's settings. Owned by `NotesTab`, which swaps the whole tab
   *  to a settings page — the settings are built on Grain's app tokens and the
   *  workspace on `.gs-frame`'s own, so nesting one in the other would mix two
   *  visual languages and collide on `data-theme`. */
  onOpenSettings: () => void;
  variant?: "default" | "next";
}) {
  const { t } = useTranslation();
  // A workspace follows Grain's theme — not the OS, and not a toggle of its own.
  // It shares the window with the rest of the app now; two appearances in one
  // window is not a preference, it is a bug.
  const { isSettingsDark } = useTheme();

  const [cards, setCards] = useState<NoteCard[]>([]);
  /** Every Grain subfolder, empty ones included — the rail's tree. Separate from
   * the cards because a folder with nothing in it exists on disk and nowhere in
   * the listing, and it still has to be there to drag a note onto. */
  const [folders, setFolders] = useState<string[]>([]);
  const [results, setResults] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [isObsidian, setIsObsidian] = useState(false);
  const [selected, setSelected] = useState<Note | null>(null);
  const [selectedReadonly, setSelectedReadonly] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [calendarOpen, setCalendarOpen] = useState(false);
  /** A corpus read is in flight (first mount, or a vault/backend/folder switch).
   * Reconciling a fresh vault is a stat-scan of every file in it, so this is the
   * difference between "changing vault takes a moment" and "the app froze". */
  const [loading, setLoading] = useState(true);

  const queryRef = useRef("");
  const selectedRef = useRef<Note | null>(null);
  const readonlyRef = useRef(false);
  const dirtyRef = useRef(false);
  const savingRef = useRef(false);
  const saveTimer = useRef<number | undefined>(undefined);
  const lexicalTimer = useRef<number | undefined>(undefined);
  const hybridTimer = useRef<number | undefined>(undefined);
  /** Bumped per search. A staged search has two responses in flight and the slow
   * one can land after the user has typed further, so every write to `results` is
   * checked against this — without it, a late hybrid reply for "wif" overwrites
   * the correct results for "wifi password". */
  const searchGen = useRef(0);
  const mountedRef = useRef(false);
  const nextChatButtonRef = useRef<HTMLButtonElement>(null);
  queryRef.current = query;
  selectedRef.current = selected;
  readonlyRef.current = selectedReadonly;

  const closeNextChat = useCallback(() => {
    setChatOpen(false);
    nextChatButtonRef.current?.focus();
  }, []);

  /** Card lookup for collection chips + readonly checks on search hits. */
  const cardById = useMemo(() => {
    const map = new Map<string, NoteCard>();
    for (const card of cards) map.set(card.id, card);
    return map;
  }, [cards]);
  const cardByIdRef = useRef(cardById);
  cardByIdRef.current = cardById;

  /** Bumped on every editor switch — keys the CodeMirror document so a draft
   * adopting its backend-minted id mid-typing never resets the caret. */
  const [editSession, setEditSession] = useState(0);

  /** Persist the selected note. Drafts are created first so ids stay backend-minted. */
  const saveSelected = useCallback(async () => {
    const note = selectedRef.current;
    if (!note || !dirtyRef.current || savingRef.current) return;
    if (readonlyRef.current) return; // foreign vault file: never write
    if (!note.id && !note.title.trim() && !note.body.trim()) return; // empty draft: never persist
    savingRef.current = true;
    dirtyRef.current = false;
    try {
      if (!note.id) {
        const created = await commands.grainSpaceCreateNote(note.body);
        if (created.status !== "ok") throw new Error(created.error);
        const merged = { ...created.data, title: note.title };
        if (note.title.trim()) {
          const saved = await commands.grainSpaceSaveNote(merged);
          if (saved.status !== "ok") throw new Error(saved.error);
        }
        const current = selectedRef.current;
        if (current === note) {
          setSelected(merged);
          selectedRef.current = merged;
        } else if (current && !current.id) {
          // Keystrokes landed while the create was in flight: adopt the minted
          // id into the newer draft so the follow-up save can't duplicate it.
          const adopted = {
            ...current,
            id: merged.id,
            timestamp: merged.timestamp,
          };
          setSelected(adopted);
          selectedRef.current = adopted;
          dirtyRef.current = true;
        }
      } else {
        const result = await commands.grainSpaceSaveNote(note);
        if (result.status !== "ok") throw new Error(result.error);
      }
    } catch (e) {
      console.error("Grain Space: save failed:", e);
      dirtyRef.current = true; // retry on the next edit/flush
    } finally {
      savingRef.current = false;
      // A debounce tick that fired mid-save bailed on savingRef — catch up.
      if (dirtyRef.current) {
        window.clearTimeout(saveTimer.current);
        saveTimer.current = window.setTimeout(() => void saveSelected(), 300);
      }
    }
  }, []);

  /** Debounced save-on-change (600 ms), flushed on blur/close/switch. */
  const touchSelected = useCallback(
    (updated: Note) => {
      if (readonlyRef.current) return;
      setSelected(updated);
      selectedRef.current = updated;
      dirtyRef.current = true;
      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => void saveSelected(), 600);
    },
    [saveSelected],
  );

  const flushSave = useCallback(async () => {
    window.clearTimeout(saveTimer.current);
    await saveSelected();
  }, [saveSelected]);

  // [GRAIN] Flush on unmount. This is the one thing a tab does not get for free
  // that the old sleeping window did: the sleep handshake flushed before the
  // unmount, but switching tabs just tears the tree down, and the save is
  // debounced 600 ms. Without this, the last thing you typed before clicking
  // History is gone.
  useEffect(() => {
    return () => {
      void flushSave();
    };
  }, [flushSave]);

  // [GRAIN] Tell the backend we are on screen. This is what bounds the embedding
  // model's lifetime now that there is no window whose visibility could be
  // checked: leaving the tab reclaims it (unless the Agent panel still wants it).
  useEffect(() => {
    void commands.grainSpaceWorkspaceMounted(true);
    return () => {
      void commands.grainSpaceWorkspaceMounted(false);
    };
  }, []);

  /** Switch the editor to another note (flushing pending edits first). */
  const adopt = useCallback((note: Note | null, readonly: boolean) => {
    setSelected(note);
    selectedRef.current = note;
    setSelectedReadonly(readonly);
    readonlyRef.current = readonly;
    dirtyRef.current = false;
    setEditSession((s) => s + 1);
    setCalendarOpen(false); // opening a note leaves the calendar view
  }, []);

  const selectCard = useCallback(
    async (card: NoteCard) => {
      await flushSave();
      const result = await commands.grainSpaceGetNote(card.id);
      if (result.status !== "ok") {
        console.error("Grain Space: open note failed:", result.error);
        return;
      }
      // Everything in the note UI lives inside the Grain folder → editable.
      adopt(result.data, false);
    },
    [adopt, flushSave],
  );

  const selectResult = useCallback(
    async (note: Note) => {
      await flushSave();
      adopt(note, false);
    },
    [adopt, flushSave],
  );

  /** Open a note by id — the chat rail's source chips land here. A hit outside
   * the Grain folder (whole-vault recall on the Obsidian backend) opens
   * read-only, matching how foreign vault files behave elsewhere. */
  const openNoteById = useCallback(
    async (id: string) => {
      await flushSave();
      const result = await commands.grainSpaceGetNote(id);
      if (result.status !== "ok") {
        console.error("Grain Space: open note failed:", result.error);
        return;
      }
      adopt(result.data, !cardByIdRef.current.has(id));
    },
    [adopt, flushSave],
  );

  const newNote = useCallback(async () => {
    await flushSave();
    adopt(createBlankDraft(), false);
  }, [adopt, flushSave]);

  /** Re-read the folder tree. Cheap (a bounded directory walk), and separate from
   *  the card listing because folders change without notes changing. */
  const refreshFolders = useCallback(async () => {
    const result = await commands.grainSpaceListAllFolders();
    if (result.status === "ok") setFolders(result.data);
    else console.error("Grain Space: folder listing failed:", result.error);
  }, []);

  const createFolder = useCallback(async (name: string) => {
    const result = await commands.grainSpaceCreateFolder(name);
    if (result.status !== "ok") {
      console.error("Grain Space: create folder failed:", result.error);
      return;
    }
    // Adopt the path the backend actually created — sanitizing can change it,
    // and a tree built from what we ASKED for would disagree with the disk.
    setFolders((current) =>
      current.includes(result.data)
        ? current
        : [...current, result.data].sort(),
    );
  }, []);

  const moveNote = useCallback(
    async (id: string, folder: string | null) => {
      // Flush first: the note being filed may be the open one with unsaved
      // edits, and the move renames its file underneath us.
      await flushSave();
      const result = await commands.grainSpaceMoveNote(id, folder);
      if (result.status !== "ok") {
        console.error("Grain Space: move failed:", result.error);
        return;
      }
      // The move emits notes-changed, which re-lists the cards; the folder tree
      // needs its own nudge because filing the last note out of a folder does
      // not delete the folder, and filing into a new one is already known.
      await refreshFolders();
    },
    [flushSave, refreshFolders],
  );

  const deleteFolder = useCallback(
    async (folder: string) => {
      // The delete renames note files back to the root; flush an open, edited
      // note first so its in-flight save does not race the move.
      await flushSave();
      const result = await commands.grainSpaceDeleteFolder(folder);
      if (result.status !== "ok") {
        console.error("Grain Space: delete folder failed:", result.error);
        return;
      }
      // notes-changed re-lists the cards (the notes are now loose); the tree
      // needs its own refresh because the folder itself is gone from disk.
      await refreshFolders();
    },
    [flushSave, refreshFolders],
  );

  /**
   * Accept a search response, unless a newer search has started since it was
   * issued. Results are scoped to the Grain folder, matching the browse list:
   * `cardById` holds exactly those notes, so a hit outside it — the user's own
   * vault, on the Obsidian backend — is dropped. Backend recall stays whole-vault.
   */
  const acceptResults = useCallback((gen: number, hits: Note[]) => {
    if (searchGen.current !== gen) return;
    setResults(hits.filter((n) => cardByIdRef.current.has(n.id)));
  }, []);

  /** Run the current browse/search and (optionally) refresh the open note. */
  const refresh = useCallback(
    async (refreshSelected = false) => {
      const q = queryRef.current.trim();
      if (!q) {
        const list = await commands.grainSpaceListCards();
        if (list.status !== "ok") {
          console.error("Grain Space: list failed:", list.error);
          return;
        }
        setCards(list.data);
        setResults([]);
      } else {
        // Re-running a live search (notes changed underneath it). One call, the
        // full one — there is nothing to stage when the user is not typing.
        const gen = ++searchGen.current;
        const result = await commands.grainSpaceSearch(q);
        if (result.status !== "ok") {
          console.error("Grain Space: search failed:", result.error);
          return;
        }
        acceptResults(gen, result.data);
      }

      // Quiet content refresh for the open note (e.g. quick-add elsewhere
      // touched it) — only when there are no pending local edits.
      if (refreshSelected) {
        const current = selectedRef.current;
        if (current?.id && !dirtyRef.current) {
          const fresh = await commands.grainSpaceGetNote(current.id);
          if (fresh.status === "ok" && selectedRef.current?.id === current.id) {
            setSelected(fresh.data);
            selectedRef.current = fresh.data;
          }
        }
      }
    },
    [acceptResults],
  );

  /**
   * Read the corpus from scratch: backend settings, then the listing, then open
   * whichever note we should land on.
   *
   * Used on mount AND on `corpus-changed` — a vault switch, a backend switch, a
   * different notes folder. Nothing is carried across: the previous vault's cards,
   * search and open note all belong to a notebook that is no longer the one on
   * screen. `loading` is what keeps the swap from looking frozen, because the
   * first listing against a fresh vault reconciles it (a stat-scan of every file)
   * before it can answer.
   */
  const reload = useCallback(async () => {
    setLoading(true);
    dirtyRef.current = false;
    window.clearTimeout(saveTimer.current);
    setQuery("");
    queryRef.current = "";
    setResults([]);
    setCards([]);
    setFolders([]);
    adopt(null, false);
    try {
      await refreshFolders();
      const settings = await commands.getAppSettings();
      if (settings.status === "ok") {
        // Only the backend matters to the UI now. Whether the meaning leg is
        // available is the search command's business, not a mode this has to
        // know about in order to draw a switch.
        setIsObsidian(settings.data.grain_space_backend === "obsidian");
      }
      const focus = await commands.grainSpaceTakeFocusNote();
      const list = await commands.grainSpaceListCards();
      const firstCardId = list.status === "ok" ? list.data[0]?.id : undefined;
      const focusIsListed =
        list.status === "ok" &&
        focus != null &&
        list.data.some((card) => card.id === focus);
      if (list.status === "ok") {
        setCards(list.data);
      } else {
        console.error("Grain Space: list failed:", list.error);
      }
      const landing = resolveNotesLanding(
        variant,
        focus,
        firstCardId,
        list.status === "ok",
        focusIsListed,
      );
      if (landing.kind === "draft") {
        // UI 2.0 always arrives ready to write; an untouched draft stays local.
        adopt(createBlankDraft(), false);
        return;
      }
      if (landing.kind === "none") return;

      const note = await commands.grainSpaceGetNote(landing.id);
      if (note.status === "ok") {
        adopt(note.data, false);
      } else {
        console.error("Grain Space: initial note open failed:", note.error);
        if (variant === "next") adopt(createBlankDraft(), false);
      }
    } finally {
      setLoading(false);
    }
  }, [adopt, refreshFolders, variant]);

  // Mount: settings + focus-note handoff + first listing + event wiring.
  useEffect(() => {
    const unlistens = [
      listen(NOTES_CHANGED_EVENT, () => void refresh(true)),
      // The corpus was replaced underneath us. Do NOT flush first: the pending
      // edit belongs to a note in the vault we are leaving, and the backend has
      // already switched, so a save now would write it into the new one.
      listen(CORPUS_CHANGED_EVENT, () => void reload()),
      listen<string>(FOCUS_NOTE_EVENT, async (event) => {
        await flushSave();
        const result = await commands.grainSpaceGetNote(event.payload);
        if (result.status === "ok") {
          adopt(result.data, false);
        }
      }),
    ];

    if (!mountedRef.current) {
      mountedRef.current = true;
      void reload();
    }

    return () => {
      unlistens.forEach((p) => void p.then((fn) => fn()));
    };
  }, [adopt, flushSave, refresh, reload]);

  // Staged search-as-you-type. There is no mode to pick: the lexical pass lands
  // fast so the list moves while you type, and the hybrid pass (which may have to
  // spawn the embedding model) supersedes it on a real pause. Both writes are
  // gated on `searchGen`, so a slow reply for an older query can never win.
  useEffect(() => {
    const q = query.trim();
    window.clearTimeout(lexicalTimer.current);
    window.clearTimeout(hybridTimer.current);
    if (!q) {
      // Cancels any search still in flight AND makes its late reply a no-op.
      searchGen.current += 1;
      setResults([]);
      return;
    }
    const gen = ++searchGen.current;
    lexicalTimer.current = window.setTimeout(() => {
      void commands.grainSpaceSearchNotes(q).then((r) => {
        if (r.status === "ok") acceptResults(gen, r.data);
        else console.error("Grain Space: lexical search failed:", r.error);
      });
    }, LEXICAL_DEBOUNCE_MS);
    hybridTimer.current = window.setTimeout(() => {
      void commands.grainSpaceSearch(q).then((r) => {
        if (r.status === "ok") acceptResults(gen, r.data);
        else console.error("Grain Space: search failed:", r.error);
      });
    }, HYBRID_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(lexicalTimer.current);
      window.clearTimeout(hybridTimer.current);
    };
  }, [query, acceptResults]);

  // Esc clears the search. It no longer closes anything — this is a tab, and a
  // stray Escape must never take the user out of the app. Ctrl+N: new note.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (!queryRef.current) return;
        e.preventDefault();
        setQuery("");
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        void newNote();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [newNote]);

  // Safety net: flush pending edits if the window is truly torn down.
  useEffect(() => {
    const flush = () => void flushSave();
    window.addEventListener("beforeunload", flush);
    return () => window.removeEventListener("beforeunload", flush);
  }, [flushSave]);

  // Live two-way sync: there is no resident file watcher (zero idle RAM), so
  // re-reconcile the vault whenever the window regains focus — returning from
  // Obsidian surfaces new/edited/moved notes and folders immediately. Cheap: a
  // stat-scan the reconcile already runs on every listing, and mid-edit local
  // changes are protected (refresh only re-adopts the open note when not dirty).
  useEffect(() => {
    const onFocus = () => void refresh(true);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  const deleteSelected = async () => {
    const note = selectedRef.current;
    if (!note || readonlyRef.current) return;
    dirtyRef.current = false;
    window.clearTimeout(saveTimer.current);
    if (note.id) {
      const result = await commands.grainSpaceDeleteNote(note.id);
      if (result.status !== "ok") {
        console.error("Grain Space: delete failed:", result.error);
        return;
      }
    }
    adopt(variant === "next" ? createBlankDraft() : null, false);
    void refresh();
  };

  const togglePin = async () => {
    const note = selectedRef.current;
    if (!note?.id || readonlyRef.current) return;
    const result = await commands.grainSpaceSetPinned(note.id, !note.is_pinned);
    if (result.status === "ok") {
      setSelected(result.data);
      selectedRef.current = result.data;
      void refresh();
    }
  };

  const armReminder = async () => {
    const note = selectedRef.current;
    const fireAt = note?.reminder_state?.fire_at ?? null;
    if (!note?.id || fireAt == null) return;
    const result = await commands.grainSpaceArmReminder(note.id, fireAt);
    if (result.status === "ok") {
      setSelected(result.data);
      selectedRef.current = result.data;
    }
  };

  const dismissReminder = async () => {
    const note = selectedRef.current;
    if (!note?.id) return;
    const result = await commands.grainSpaceDismissReminder(note.id);
    if (result.status === "ok") {
      setSelected(result.data);
      selectedRef.current = result.data;
    }
  };

  const openExternal = () => {
    const note = selectedRef.current;
    if (!note?.id) return;
    void commands.grainSpaceOpenInObsidian(note.id);
  };

  // [GRAIN] No model banner here any more. It existed to catch the one case where
  // picking "Semantic" hit a model that had never been downloaded — consent,
  // progress and errors, in a bar over the notes. There is no mode to pick now:
  // search uses the meaning leg when it is available and lexical ranking when it
  // is not, silently. Downloading the model is a decision made once, in Settings,
  // which already owns that whole flow.

  const searching = query.trim().length > 0;
  const selectedFolder =
    (selected && cardById.get(selected.id)?.folder) ?? null;

  /** Notes carrying a live (armed/fired) reminder, ordered upcoming-first then
   * most-recently-past — the source for the sidebar dock and calendar view. */
  const reminders = useMemo(() => {
    const now = Date.now();
    return cards
      .filter(
        (c) =>
          (c.reminder_state.status === "armed" ||
            c.reminder_state.status === "fired") &&
          c.reminder_state.fire_at != null,
      )
      .sort((a, b) => {
        const fa = a.reminder_state.fire_at as number;
        const fb = b.reminder_state.fire_at as number;
        const aFut = fa >= now;
        const bFut = fb >= now;
        if (aFut !== bFut) return aFut ? -1 : 1;
        return aFut ? fa - fb : fb - fa;
      });
  }, [cards]);

  return (
    <div className={`gs-root${variant === "next" ? " gs-next" : ""}`}>
      <div className="gs-frame" data-theme={isSettingsDark ? "dark" : "light"}>
        <Sidebar
          variant={variant}
          cards={cards}
          folders={folders}
          reminders={reminders}
          results={results}
          selectedId={selected?.id ?? null}
          calendarOpen={calendarOpen}
          query={query}
          onQueryChange={setQuery}
          onOpenCalendar={() => setCalendarOpen(true)}
          onOpenAll={() => setCalendarOpen(false)}
          onOpenSettings={onOpenSettings}
          onSelectCard={(card) => void selectCard(card)}
          onSelectResult={(note) => void selectResult(note)}
          onCreate={() => void newNote()}
          onCreateFolder={(name) => void createFolder(name)}
          onMoveNote={(id, folder) => void moveNote(id, folder)}
          onDeleteFolder={(folder) => void deleteFolder(folder)}
        />

        <div className="gs-main">
          {/* [GRAIN] Nothing above the note. The strip that used to sit here held
              search, the search-mode switch and the window controls; search moved
              into the rail (where its results render), the mode switch is gone
              entirely, and the window controls went with the window. The note now
              owns the whole right-hand side, edge to edge.

              The chat toggle floats INSIDE the note's top-right corner rather than
              in a bar of its own — a full-width strip to carry one button is a lot
              of vertical space for one button. */}
          <div className="gs-stage">
            {variant === "next" ? (
              <div
                className={`gs-next-editor-controls${chatOpen ? " is-chat-open" : ""}`}
              >
                <button
                  ref={nextChatButtonRef}
                  type="button"
                  className={`gs-next-chat-control${chatOpen ? " is-open" : ""}`}
                  aria-label={t("grainSpaceOverlay.chat")}
                  aria-pressed={chatOpen}
                  onClick={() => setChatOpen((value) => !value)}
                >
                  <Sparkles width={14} height={14} />
                  <span>{t("grainSpaceOverlay.chat")}</span>
                </button>
                <button
                  type="button"
                  className="gs-next-editor-settings"
                  title={t("grainSpaceOverlay.settings")}
                  aria-label={t("grainSpaceOverlay.settings")}
                  onClick={onOpenSettings}
                >
                  <Settings width={15} height={15} />
                </button>
              </div>
            ) : (
              <button
                type="button"
                className={`gs-chat-toggle${chatOpen ? " gs-chat-toggle--on" : ""}`}
                title={t("grainSpaceOverlay.chat")}
                aria-label={t("grainSpaceOverlay.chat")}
                aria-pressed={chatOpen}
                onClick={() => setChatOpen((v) => !v)}
              >
                <MessageSquare width={15} height={15} />
              </button>
            )}
            {loading ? (
              <section className="gs-sheet">
                <div className="gs-sheet-empty">
                  {t("grainSpaceOverlay.reading")}
                </div>
              </section>
            ) : calendarOpen ? (
              <CalendarView
                reminders={reminders}
                onSelectCard={(card) => void selectCard(card)}
              />
            ) : selected ? (
              <EditorPane
                variant={variant}
                note={selected}
                docKey={editSession}
                readonly={selectedReadonly}
                isObsidian={isObsidian}
                folder={selectedFolder}
                onEdit={touchSelected}
                onFlush={() => void flushSave()}
                onTogglePin={() => void togglePin()}
                onDelete={() => void deleteSelected()}
                onArmReminder={() => void armReminder()}
                onDismissReminder={() => void dismissReminder()}
                onOpenExternal={openExternal}
              />
            ) : (
              <section className="gs-sheet">
                <div className="gs-sheet-empty">
                  {t("grainSpaceOverlay.noSelection")}
                </div>
              </section>
            )}
            <ChatRail
              open={chatOpen}
              onOpenNote={(id) => void openNoteById(id)}
              onClose={variant === "next" ? closeNextChat : undefined}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
