import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { MessageSquare, Plus, Search, X } from "lucide-react";
import { commands, type Note, type NoteCard } from "@/bindings";
import { useTheme } from "@/contexts/ThemeContext";
import { Sidebar } from "./Sidebar";
import { EditorPane } from "./EditorPane";
import { CalendarView } from "./CalendarView";
import { ChatRail } from "./ChatRail";
import "./grain-space.css";

/** Backend events (see src-tauri/src/grain_space). */
const NOTES_CHANGED_EVENT = "grain-space://notes-changed";
const CORPUS_CHANGED_EVENT = "grain-space://corpus-changed";
const FOCUS_NOTE_EVENT = "grain-space://focus-note";
const MODEL_PROGRESS_EVENT = "grain-space://embed-model-progress";
const MODEL_COMPLETE_EVENT = "grain-space://embed-model-complete";
const MODEL_ERROR_EVENT = "grain-space://embed-model-error";

type SearchMode = "exact" | "semantic";

type ModelBanner =
  | { kind: "consent" }
  | { kind: "downloading"; percentage: number }
  | { kind: "error"; message: string };

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
export function GrainSpaceOverlay() {
  const { t } = useTranslation();
  // A workspace follows Grain's theme — not the OS, and not a toggle of its own.
  // It shares the window with the rest of the app now; two appearances in one
  // window is not a preference, it is a bug.
  const { isSettingsDark } = useTheme();

  const [cards, setCards] = useState<NoteCard[]>([]);
  const [results, setResults] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<SearchMode>("exact");
  const [semanticAvailable, setSemanticAvailable] = useState(false);
  const [isObsidian, setIsObsidian] = useState(false);
  const [selected, setSelected] = useState<Note | null>(null);
  const [selectedReadonly, setSelectedReadonly] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [calendarOpen, setCalendarOpen] = useState(false);
  const [banner, setBanner] = useState<ModelBanner | null>(null);
  /** A corpus read is in flight (first mount, or a vault/backend/folder switch).
   * Reconciling a fresh vault is a stat-scan of every file in it, so this is the
   * difference between "changing vault takes a moment" and "the app froze". */
  const [loading, setLoading] = useState(true);

  const queryRef = useRef("");
  const modeRef = useRef<SearchMode>("exact");
  const selectedRef = useRef<Note | null>(null);
  const readonlyRef = useRef(false);
  const dirtyRef = useRef(false);
  const savingRef = useRef(false);
  const saveTimer = useRef<number | undefined>(undefined);
  const searchTimer = useRef<number | undefined>(undefined);
  const mountedRef = useRef(false);
  queryRef.current = query;
  modeRef.current = mode;
  selectedRef.current = selected;
  readonlyRef.current = selectedReadonly;

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

  /** A fresh, not-yet-persisted note (backend mints the real id on first save). */
  const blankDraft = (): Note => ({
    id: "",
    title: "",
    tldr: "",
    body: "",
    timestamp: Date.now(),
    todo_tags: [],
    reminder_state: { status: "none", fire_at: null },
    is_pinned: false,
  });

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
    adopt(blankDraft(), false);
  }, [adopt, flushSave]);

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
      let result;
      if (modeRef.current === "semantic") {
        result = await commands.grainSpaceSemanticSearch(q);
        if (result.status === "error") {
          if (result.error === "model-not-downloaded") {
            setBanner((b) =>
              b?.kind === "downloading" ? b : { kind: "consent" },
            );
          } else {
            console.error("Grain Space: semantic search failed:", result.error);
          }
          result = await commands.grainSpaceSearchNotes(q);
        }
      } else {
        result = await commands.grainSpaceSearchNotes(q);
      }
      if (result.status !== "ok") {
        console.error("Grain Space: search failed:", result.error);
        return;
      }
      // Scope results to the Grain folder (matching the browse): `cardById`
      // holds exactly the Grain-folder notes, so any hit outside it — the
      // user's own vault — is dropped. Backend recall stays whole-vault.
      setResults(result.data.filter((n) => cardByIdRef.current.has(n.id)));
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
    [],
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
    adopt(null, false);
    try {
      const settings = await commands.getAppSettings();
      if (settings.status === "ok") {
        setSemanticAvailable(settings.data.grain_space_semantic ?? false);
        setIsObsidian(settings.data.grain_space_backend === "obsidian");
      }
      const focus = await commands.grainSpaceTakeFocusNote();
      const list = await commands.grainSpaceListCards();
      if (list.status !== "ok") {
        console.error("Grain Space: list failed:", list.error);
        return;
      }
      setCards(list.data);
      const target = focus
        ? (list.data.find((c) => c.id === focus) ?? list.data[0])
        : list.data[0];
      if (!target) {
        // No notes at all ⇒ open straight into a new blank note.
        adopt(blankDraft(), false);
        return;
      }
      const note = await commands.grainSpaceGetNote(target.id);
      if (note.status === "ok") adopt(note.data, false);
    } finally {
      setLoading(false);
    }
  }, [adopt]);

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
      listen<{ percentage: number }>(MODEL_PROGRESS_EVENT, (event) => {
        setBanner({
          kind: "downloading",
          percentage: event.payload.percentage,
        });
      }),
      listen(MODEL_COMPLETE_EVENT, () => {
        setBanner(null);
        void refresh();
      }),
      listen<string>(MODEL_ERROR_EVENT, (event) => {
        setBanner({ kind: "error", message: event.payload });
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

  // Debounced search-as-you-type (semantic waits a bit longer per keystroke).
  useEffect(() => {
    window.clearTimeout(searchTimer.current);
    searchTimer.current = window.setTimeout(
      () => void refresh(),
      mode === "semantic" ? 350 : 160,
    );
    return () => window.clearTimeout(searchTimer.current);
  }, [query, mode, refresh]);

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
    adopt(null, false);
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

  const startModelDownload = () => {
    setBanner({ kind: "downloading", percentage: 0 });
    commands.grainSpaceDownloadEmbedModel().then((result) => {
      if (result.status === "error") {
        // The error event usually beat us here; keep whichever message exists.
        setBanner((b) =>
          b?.kind === "error" ? b : { kind: "error", message: result.error },
        );
      }
    });
  };

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
    <div className="gs-root">
      <div className="gs-frame" data-theme={isSettingsDark ? "dark" : "light"}>
        <Sidebar
          cards={cards}
          reminders={reminders}
          searching={searching}
          results={results}
          selectedId={selected?.id ?? null}
          calendarOpen={calendarOpen}
          onOpenCalendar={() => setCalendarOpen(true)}
          onSelectCard={(card) => void selectCard(card)}
          onSelectResult={(note) => void selectResult(note)}
          onCreate={() => void newNote()}
        />

        <div className="gs-main">
          {/* No drag region, no window controls, no appearance toggle: this is a
              tab in Grain's window, which owns its own title bar and theme.
              (Phase B moves what is left of this strip into the rail.) */}
          <div className="gs-top">
            <div className="gs-search">
              <Search width={13} height={13} />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("grainSpaceOverlay.searchPlaceholder")}
                spellCheck={false}
              />
              {query && (
                <button
                  type="button"
                  className="gs-iconbtn"
                  title="Clear search"
                  onClick={() => setQuery("")}
                >
                  <X width={12} height={12} />
                </button>
              )}
            </div>
            {semanticAvailable && (
              <div className="gs-mode">
                <button
                  type="button"
                  className={mode === "exact" ? "gs-mode--on" : ""}
                  onClick={() => setMode("exact")}
                >
                  {t("grainSpaceOverlay.exact")}
                </button>
                <button
                  type="button"
                  className={mode === "semantic" ? "gs-mode--on" : ""}
                  onClick={() => setMode("semantic")}
                >
                  {t("grainSpaceOverlay.semantic")}
                </button>
              </div>
            )}
            <div className="gs-top-actions">
              <button
                type="button"
                className="gs-iconbtn"
                title="New note (Ctrl+N)"
                onClick={() => void newNote()}
              >
                <Plus width={16} height={16} />
              </button>
              <button
                type="button"
                className={`gs-iconbtn${chatOpen ? " gs-iconbtn--active" : ""}`}
                title="Toggle chat"
                onClick={() => setChatOpen((v) => !v)}
              >
                <MessageSquare width={15} height={15} />
              </button>
            </div>
          </div>

          {banner && (
            <div className="gs-banner">
              {banner.kind === "consent" && (
                <>
                  <span className="gs-banner-text">
                    {t("grainSpaceOverlay.consent")}
                  </span>
                  <button
                    type="button"
                    className="gs-btn"
                    onClick={startModelDownload}
                  >
                    {t("grainSpaceOverlay.download")}
                  </button>
                  <button
                    type="button"
                    className="gs-btn gs-btn--quiet"
                    onClick={() => {
                      setBanner(null);
                      setMode("exact");
                    }}
                  >
                    {t("grainSpaceOverlay.notNow")}
                  </button>
                </>
              )}
              {banner.kind === "downloading" && (
                <>
                  <span className="gs-banner-text">
                    {t("grainSpaceOverlay.downloading")}
                  </span>
                  <div className="gs-progress">
                    <span
                      style={{ width: `${banner.percentage.toFixed(1)}%` }}
                    />
                  </div>
                  <button
                    type="button"
                    className="gs-btn gs-btn--quiet"
                    onClick={() => {
                      void commands.grainSpaceCancelEmbedModelDownload();
                      setBanner(null);
                    }}
                  >
                    {t("grainSpaceOverlay.cancel")}
                  </button>
                </>
              )}
              {banner.kind === "error" && (
                <>
                  <span className="gs-banner-text gs-banner-error">
                    {t("grainSpaceOverlay.downloadFailed", {
                      message: banner.message,
                    })}
                  </span>
                  <button
                    type="button"
                    className="gs-btn"
                    onClick={startModelDownload}
                  >
                    {t("grainSpaceOverlay.retry")}
                  </button>
                  <button
                    type="button"
                    className="gs-btn gs-btn--quiet"
                    onClick={() => setBanner(null)}
                  >
                    {t("grainSpaceOverlay.dismiss")}
                  </button>
                </>
              )}
            </div>
          )}

          <div className="gs-stage">
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
            />
          </div>
        </div>
      </div>
    </div>
  );
}
