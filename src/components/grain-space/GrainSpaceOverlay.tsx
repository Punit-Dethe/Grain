import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { AlarmClock, Pin, Plus, Search, Trash2, X } from "lucide-react";
import { commands, type Note, type ReminderState } from "@/bindings";
import "./grain-space.css";

/** Backend events (see src-tauri/src/grain_space). */
const NOTES_CHANGED_EVENT = "grain-space://notes-changed";
const FOCUS_NOTE_EVENT = "grain-space://focus-note";
const MODEL_PROGRESS_EVENT = "grain-space://embed-model-progress";
const MODEL_COMPLETE_EVENT = "grain-space://embed-model-complete";
const MODEL_ERROR_EVENT = "grain-space://embed-model-error";

type SearchMode = "exact" | "semantic";

type ModelBanner =
  | { kind: "consent" }
  | { kind: "downloading"; percentage: number }
  | { kind: "error"; message: string };

const reminderOf = (note: Note): ReminderState =>
  note.reminder_state ?? { status: "none", fire_at: null };

const timeFormat = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});
const dateFormat = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});
const fireFormat = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
});

/** Local-day bucket label: Today / Yesterday / formatted date. */
function dayLabel(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const startOf = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round(
    (startOf(today) - startOf(d)) / (24 * 60 * 60 * 1000),
  );
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return dateFormat.format(d);
}

function listTitle(note: Note): string {
  if (note.title.trim()) return note.title;
  const firstLine = note.body.split("\n")[0]?.trim() ?? "";
  return firstLine.length > 48 ? `${firstLine.slice(0, 45)}…` : firstLine;
}

/**
 * [GRAIN] The Grain Space overlay: a Raycast-Notes-style two-pane browser.
 * Search on top (exact FTS, plus opt-in semantic), date-grouped list on the
 * left, and the note itself — a full editor, not a metadata panel — on the
 * right, with a compact pin/reminder/delete action row at the bottom-right.
 * Strictly search + manual editing (no voice append, no ask-AI: directive 12).
 * The window is created on summon and destroyed on close/Esc.
 */
export function GrainSpaceOverlay() {
  const { t } = useTranslation();
  const win = getCurrentWindow();

  const [notes, setNotes] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<SearchMode>("exact");
  const [semanticAvailable, setSemanticAvailable] = useState(false);
  const [selected, setSelected] = useState<Note | null>(null);
  const [banner, setBanner] = useState<ModelBanner | null>(null);

  const queryRef = useRef("");
  const modeRef = useRef<SearchMode>("exact");
  const selectedRef = useRef<Note | null>(null);
  const dirtyRef = useRef(false);
  const savingRef = useRef(false);
  const saveTimer = useRef<number | undefined>(undefined);
  const searchTimer = useRef<number | undefined>(undefined);
  const mountedRef = useRef(false);
  queryRef.current = query;
  modeRef.current = mode;
  selectedRef.current = selected;

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

  /** Switch the editor to another note (flushing pending edits first). */
  const selectNote = useCallback(
    async (note: Note | null) => {
      await flushSave();
      setSelected(note);
      selectedRef.current = note;
      dirtyRef.current = false;
    },
    [flushSave],
  );

  const newNote = useCallback(async () => {
    await selectNote(blankDraft());
  }, [selectNote]);

  /** Run the current search (or list) and reconcile the selection. */
  const refresh = useCallback(async () => {
    const q = queryRef.current.trim();
    let result;
    if (!q) {
      result = await commands.grainSpaceListNotes();
    } else if (modeRef.current === "semantic") {
      result = await commands.grainSpaceSemanticSearch(q);
      if (result.status === "error") {
        if (result.error === "model-not-downloaded") {
          setBanner((b) => (b?.kind === "downloading" ? b : { kind: "consent" }));
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
    const fresh = result.data;
    setNotes(fresh);

    const current = selectedRef.current;
    if (current) {
      // Keep the editor put; refresh its content only when there are no
      // pending local edits (e.g. quick-add elsewhere touched another note).
      const match = fresh.find((n) => n.id === current.id);
      if (match && !dirtyRef.current) {
        setSelected(match);
        selectedRef.current = match;
      }
      return;
    }
    if (fresh.length > 0) {
      setSelected(fresh[0]);
      selectedRef.current = fresh[0];
    }
  }, []);

  // Mount: blank-vs-list rule + focus-note handoff + event wiring.
  useEffect(() => {
    if (mountedRef.current) return;
    mountedRef.current = true;

    void (async () => {
      const settings = await commands.getAppSettings();
      if (settings.status === "ok") {
        setSemanticAvailable(settings.data.grain_space_semantic ?? false);
      }
      const focus = await commands.grainSpaceTakeFocusNote();
      const list = await commands.grainSpaceListNotes();
      if (list.status !== "ok") {
        console.error("Grain Space: list failed:", list.error);
        return;
      }
      setNotes(list.data);
      if (list.data.length === 0) {
        // No notes at all ⇒ open straight into a new blank note.
        const draft = blankDraft();
        setSelected(draft);
        selectedRef.current = draft;
      } else {
        const target =
          (focus && list.data.find((n) => n.id === focus)) || list.data[0];
        setSelected(target);
        selectedRef.current = target;
      }
    })();

    const unlistens = [
      listen(NOTES_CHANGED_EVENT, () => void refresh()),
      listen<string>(FOCUS_NOTE_EVENT, async (event) => {
        const result = await commands.grainSpaceGetNote(event.payload);
        if (result.status === "ok") await selectNote(result.data);
      }),
      listen<{ percentage: number }>(MODEL_PROGRESS_EVENT, (event) => {
        setBanner({ kind: "downloading", percentage: event.payload.percentage });
      }),
      listen(MODEL_COMPLETE_EVENT, () => {
        setBanner(null);
        void refresh();
      }),
      listen<string>(MODEL_ERROR_EVENT, (event) => {
        setBanner({ kind: "error", message: event.payload });
      }),
    ];
    return () => {
      unlistens.forEach((p) => void p.then((fn) => fn()));
    };
  }, [refresh, selectNote]);

  // Debounced search-as-you-type (semantic waits a bit longer per keystroke).
  useEffect(() => {
    window.clearTimeout(searchTimer.current);
    searchTimer.current = window.setTimeout(
      () => void refresh(),
      mode === "semantic" ? 350 : 160,
    );
    return () => window.clearTimeout(searchTimer.current);
  }, [query, mode, refresh]);

  // Esc: clear the search first, then close (destroy) the window. Ctrl+N: new note.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (queryRef.current) {
          setQuery("");
        } else {
          void flushSave().then(() => void win.close());
        }
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        void newNote();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [flushSave, newNote, win]);

  // Flush pending edits when the window is about to go away.
  useEffect(() => {
    const flush = () => void flushSave();
    window.addEventListener("beforeunload", flush);
    return () => window.removeEventListener("beforeunload", flush);
  }, [flushSave]);

  const deleteSelected = async () => {
    const note = selectedRef.current;
    if (!note) return;
    dirtyRef.current = false;
    window.clearTimeout(saveTimer.current);
    if (note.id) {
      const result = await commands.grainSpaceDeleteNote(note.id);
      if (result.status !== "ok") {
        console.error("Grain Space: delete failed:", result.error);
        return;
      }
    }
    const remaining = notes.filter((n) => n.id !== note.id);
    setNotes(remaining);
    const next = remaining[0] ?? null;
    setSelected(next);
    selectedRef.current = next;
  };

  const togglePin = async () => {
    const note = selectedRef.current;
    if (!note?.id) return;
    const result = await commands.grainSpaceSetPinned(note.id, !note.is_pinned);
    if (result.status === "ok") {
      setSelected(result.data);
      selectedRef.current = result.data;
      void refresh();
    }
  };

  const armReminder = async () => {
    const note = selectedRef.current;
    const fireAt = note ? reminderOf(note).fire_at : null;
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

  const toggleTodo = (index: number) => {
    const note = selectedRef.current;
    if (!note) return;
    const todos = (note.todo_tags ?? []).map((todo, i) =>
      i === index ? { ...todo, done: !todo.done } : todo,
    );
    touchSelected({ ...note, todo_tags: todos });
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

  // Grouping: pinned first, then local-day buckets (search results keep the
  // backend's relevance order instead).
  const searching = query.trim().length > 0;
  const groups: { label: string; items: Note[] }[] = [];
  if (searching) {
    if (notes.length > 0)
      groups.push({ label: t("grainSpaceOverlay.results"), items: notes });
  } else {
    const sorted = [...notes].sort(
      (a, b) =>
        Number(b.is_pinned) - Number(a.is_pinned) || b.timestamp - a.timestamp,
    );
    for (const note of sorted) {
      const label = note.is_pinned
        ? t("grainSpaceOverlay.pinned")
        : dayLabel(note.timestamp);
      const last = groups[groups.length - 1];
      if (last && last.label === label) last.items.push(note);
      else groups.push({ label, items: [note] });
    }
  }

  const reminder = selected ? reminderOf(selected) : null;
  const todos = selected?.todo_tags ?? [];

  return (
    <div className="gs-root">
      <div className="gs-card">
        <div className="gs-head" data-tauri-drag-region>
          <span className="gs-brand" data-tauri-drag-region>
            {t("grainSpaceOverlay.brand")}
          </span>
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
          <button
            type="button"
            className="gs-iconbtn"
            title="New note (Ctrl+N)"
            onClick={() => void newNote()}
          >
            <Plus width={15} height={15} />
          </button>
          <button
            type="button"
            className="gs-iconbtn"
            title="Close"
            onClick={() => void flushSave().then(() => void win.close())}
          >
            <X width={15} height={15} />
          </button>
        </div>

        {banner && (
          <div className="gs-banner">
            {banner.kind === "consent" && (
              <>
                <span className="gs-banner-text">
                  {t("grainSpaceOverlay.consent")}
                </span>
                <button type="button" className="gs-btn" onClick={startModelDownload}>
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
                  <span style={{ width: `${banner.percentage.toFixed(1)}%` }} />
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
                <button type="button" className="gs-btn" onClick={startModelDownload}>
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

        <div className="gs-body">
          <div className="gs-list">
            {groups.length === 0 ? (
              <div className="gs-list-empty">
                {searching
                  ? t("grainSpaceOverlay.noMatches")
                  : t("grainSpaceOverlay.emptyList")}
              </div>
            ) : (
              groups.map((group) => (
                <div key={group.label}>
                  <div className="gs-group">{group.label}</div>
                  {group.items.map((note) => (
                    <button
                      key={note.id}
                      type="button"
                      className={`gs-item${
                        selected?.id === note.id ? " gs-item--on" : ""
                      }`}
                      onClick={() => void selectNote(note)}
                    >
                      <div className="gs-item-title">
                        {note.is_pinned && <Pin width={11} height={11} />}
                        <span>
                          {listTitle(note) || t("grainSpaceOverlay.untitled")}
                        </span>
                      </div>
                      <div className="gs-item-sub">
                        {timeFormat.format(new Date(note.timestamp))}
                        {note.tldr.trim() ? ` · ${note.tldr}` : ""}
                      </div>
                    </button>
                  ))}
                </div>
              ))
            )}
          </div>

          {selected ? (
            <div className="gs-editor">
              <input
                className="gs-title"
                value={selected.title}
                placeholder={t("grainSpaceOverlay.titlePlaceholder")}
                spellCheck={false}
                onChange={(e) =>
                  touchSelected({ ...selected, title: e.target.value })
                }
                onBlur={() => void flushSave()}
              />
              {selected.tldr.trim() && (
                <div className="gs-tldr">{selected.tldr}</div>
              )}
              <textarea
                className="gs-bodytext"
                value={selected.body}
                placeholder={t("grainSpaceOverlay.bodyPlaceholder")}
                onChange={(e) =>
                  touchSelected({ ...selected, body: e.target.value })
                }
                onBlur={() => void flushSave()}
              />
              {todos.length > 0 && (
                <div className="gs-todos">
                  <div className="gs-todos-label">
                    {t("grainSpaceOverlay.todos")}
                  </div>
                  {todos.map((todo, i) => (
                    <label
                      key={`${i}-${todo.text}`}
                      className={`gs-todo${todo.done ? " gs-todo--done" : ""}`}
                    >
                      <input
                        type="checkbox"
                        checked={todo.done}
                        onChange={() => toggleTodo(i)}
                      />
                      <span>{todo.text}</span>
                    </label>
                  ))}
                </div>
              )}
              <div className="gs-actions">
                <div className="gs-reminder">
                  {reminder?.status === "pending" && reminder.fire_at != null && (
                    <>
                      <AlarmClock width={13} height={13} />
                      <span>{fireFormat.format(new Date(reminder.fire_at))}</span>
                      <button type="button" className="gs-btn" onClick={armReminder}>
                        {t("grainSpaceOverlay.armReminder")}
                      </button>
                    </>
                  )}
                  {(reminder?.status === "armed" ||
                    reminder?.status === "fired") && (
                    <>
                      <AlarmClock width={13} height={13} />
                      {reminder.fire_at != null && (
                        <span>
                          {fireFormat.format(new Date(reminder.fire_at))}
                        </span>
                      )}
                      <button
                        type="button"
                        className="gs-btn gs-btn--quiet"
                        onClick={dismissReminder}
                      >
                        {t("grainSpaceOverlay.dismiss")}
                      </button>
                    </>
                  )}
                </div>
                <button
                  type="button"
                  className={`gs-iconbtn${selected.is_pinned ? " gs-iconbtn--active" : ""}`}
                  title={selected.is_pinned ? "Unpin" : "Pin"}
                  onClick={togglePin}
                  disabled={!selected.id}
                >
                  <Pin width={14} height={14} />
                </button>
                <button
                  type="button"
                  className="gs-iconbtn gs-iconbtn--danger"
                  title="Delete note"
                  onClick={() => void deleteSelected()}
                >
                  <Trash2 width={14} height={14} />
                </button>
              </div>
            </div>
          ) : (
            <div className="gs-editor-empty">
              {t("grainSpaceOverlay.noSelection")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
