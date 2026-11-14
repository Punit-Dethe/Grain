/* eslint-disable i18next/no-literal-string -- Reminders copy is untranslated until the translation pass. */
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type FormEvent,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { toast } from "sonner";
import { commands, events, type Reminder } from "@/bindings";
import {
  groupReminders,
  isoDay,
  linkLabel,
  whenLabel,
  type ReminderGroup,
} from "./reminderFormat";
import { ReminderCalendar } from "./ReminderCalendar";
import "./reminders.css";

function Glyph({ name }: { name: string }) {
  return (
    <svg className="icon sm" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

/** Run a command; surface its error as a toast. Returns whether it worked. */
async function attempt<T>(
  run: () => Promise<
    { status: "ok"; data: T } | { status: "error"; error: string }
  >,
): Promise<T | undefined> {
  try {
    const result = await run();
    if (result.status === "ok") return result.data;
    toast.error(result.error);
  } catch (error) {
    toast.error(String(error));
  }
  return undefined;
}

/** The list, kept fresh by `reminders-changed`, and a clock for "Today". */
function useReminders() {
  const [reminders, setReminders] = useState<Reminder[] | null>(null);
  const [now, setNow] = useState(() => new Date());

  const reload = useCallback(async () => {
    const list = await attempt(() => commands.remindersList());
    if (list) setReminders(list);
  }, []);

  useEffect(() => {
    void reload();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void events.remindersChanged
      .listen(() => void reload())
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });
    // Day boundaries move while the tab is open; a minute is plenty.
    const tick = window.setInterval(() => setNow(new Date()), 60_000);
    return () => {
      disposed = true;
      unlisten?.();
      window.clearInterval(tick);
    };
  }, [reload]);

  return { reminders, now };
}

interface Draft {
  id: string | null;
  title: string;
  date: string;
  time: string;
  links: string[];
  details: string;
}

/** Links a reminder can carry (matches the backend's limit). */
const MAX_LINKS = 5;

function emptyDraft(day: string): Draft {
  return { id: null, title: "", date: day, time: "", links: [], details: "" };
}

function draftFrom(reminder: Reminder): Draft {
  return {
    id: reminder.id,
    title: reminder.title,
    date: reminder.date,
    time: reminder.time ?? "",
    links: reminder.links.map((l) => l.url),
    details: reminder.details ?? "",
  };
}

/** The next whole hour from now, as `HH:MM` (a time to start from). */
function nextHour(): string {
  const hour = (new Date().getHours() + 1) % 24;
  return `${String(hour).padStart(2, "0")}:00`;
}

function Composer({
  initial,
  autoFocus,
  onDone,
  onCancel,
}: {
  initial: Draft;
  autoFocus?: boolean;
  onDone: () => void;
  /** Editing: leave without saving. A new reminder collapses instead. */
  onCancel?: () => void;
}) {
  const editing = initial.id !== null;
  const [draft, setDraft] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [expanded, setExpanded] = useState(editing);
  const [noteOpen, setNoteOpen] = useState(initial.details !== "");
  const formRef = useRef<HTMLFormElement>(null);
  const titleRef = useRef<HTMLInputElement>(null);
  const linksRef = useRef<HTMLDivElement>(null);
  const noteRef = useRef<HTMLTextAreaElement>(null);
  const today = isoDay(new Date());
  const tomorrow = (() => {
    const date = new Date();
    date.setDate(date.getDate() + 1);
    return isoDay(date);
  })();

  useEffect(() => {
    if (autoFocus) titleRef.current?.focus();
  }, [autoFocus]);

  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const setLink = (index: number, value: string) =>
    setDraft((current) => ({
      ...current,
      links: current.links.map((link, i) => (i === index ? value : link)),
    }));

  const addLink = () => {
    set("links", [...draft.links, ""]);
    requestAnimationFrame(() =>
      linksRef.current
        ?.querySelector<HTMLInputElement>(".rem-link-row:last-child input")
        ?.focus(),
    );
  };

  const openNote = () => {
    setNoteOpen(true);
    requestAnimationFrame(() => noteRef.current?.focus());
  };

  /** Fold a new reminder back to its one line; what was typed is kept. */
  const collapse = () => {
    setExpanded(false);
    (document.activeElement as HTMLElement | null)?.blur();
  };

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!draft.title.trim() || saving) return;
    setSaving(true);
    const saved = await attempt(() =>
      commands.remindersSave({
        id: draft.id,
        title: draft.title,
        date: draft.date,
        time: draft.time || null,
        links: draft.links.map((l) => l.trim()).filter(Boolean),
        details: draft.details.trim() || null,
      }),
    );
    setSaving(false);
    if (!saved) return;
    if (!editing) {
      // Keep the day and time: several reminders for one day are common.
      setDraft((current) => ({
        ...current,
        title: "",
        links: [],
        details: "",
      }));
      setNoteOpen(false);
      titleRef.current?.focus();
    }
    onDone();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (onCancel) onCancel();
    else collapse();
  };

  /** Cancel a new reminder: clear it and fold it up. */
  const discard = () => {
    setDraft(emptyDraft(initial.date));
    setNoteOpen(false);
    collapse();
  };

  const untouched =
    !draft.title.trim() &&
    !draft.details.trim() &&
    draft.links.every((link) => !link.trim());

  // A press outside an untouched new reminder folds it back up; a press
  // anywhere inside the card never does.
  useEffect(() => {
    if (editing || !expanded || !untouched) return;
    const onPointer = (event: PointerEvent) => {
      if (!formRef.current?.contains(event.target as Node)) setExpanded(false);
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [editing, expanded, untouched]);

  // Tabbing out of it does too.
  const onBlur = (event: FocusEvent<HTMLFormElement>) => {
    const next = event.relatedTarget as Node | null;
    if (!editing && untouched && next && !formRef.current?.contains(next))
      setExpanded(false);
  };

  // A press on the card's empty space keeps the caret where it was.
  const onPointerDown = (event: ReactPointerEvent<HTMLFormElement>) => {
    const target = event.target as Element;
    if (!target.closest("input, textarea, button, label, a, select"))
      event.preventDefault();
  };

  const timed = draft.time !== "";

  return (
    <form
      ref={formRef}
      className="rem-composer"
      data-open={expanded ? "" : undefined}
      data-editing={editing ? "" : undefined}
      onSubmit={submit}
      onKeyDown={onKeyDown}
      onBlur={onBlur}
      onPointerDown={onPointerDown}
    >
      <div className="rem-composer-title">
        <span className="rem-composer-mark" aria-hidden="true">
          <Glyph name={editing ? "pencil" : "plus"} />
        </span>
        <input
          ref={titleRef}
          value={draft.title}
          maxLength={200}
          onChange={(e) => set("title", e.target.value)}
          onFocus={() => setExpanded(true)}
          placeholder="Remind me about…"
          aria-label="What to be reminded about"
        />
        {expanded && !editing && (
          <button
            type="button"
            className="rem-composer-fold"
            aria-label="Collapse"
            title="Collapse (Esc)"
            onClick={collapse}
          >
            <Glyph name="chev-up" />
          </button>
        )}
      </div>

      {expanded && (
        <div className="rem-composer-body">
          <div className="rem-when-row">
            <div className="rem-seg" role="group" aria-label="Day">
              <button
                type="button"
                aria-pressed={draft.date === today}
                onClick={() => set("date", today)}
              >
                Today
              </button>
              <button
                type="button"
                aria-pressed={draft.date === tomorrow}
                onClick={() => set("date", tomorrow)}
              >
                Tomorrow
              </button>
              <label
                className="rem-seg-date"
                data-on={
                  draft.date !== today && draft.date !== tomorrow
                    ? ""
                    : undefined
                }
              >
                <Glyph name="calendar" />
                <input
                  type="date"
                  value={draft.date}
                  required
                  aria-label="Day"
                  onChange={(e) =>
                    e.target.value && set("date", e.target.value)
                  }
                />
              </label>
            </div>

            {timed ? (
              <label className="rem-time" data-on="">
                <Glyph name="clock" />
                <input
                  type="time"
                  value={draft.time}
                  aria-label="Time"
                  onChange={(e) => set("time", e.target.value)}
                />
                <button
                  type="button"
                  className="rem-time-clear"
                  aria-label="Make it all-day"
                  title="All day"
                  onClick={() => set("time", "")}
                >
                  <Glyph name="close" />
                </button>
              </label>
            ) : (
              <button
                type="button"
                className="rem-time"
                onClick={() => set("time", nextHour())}
              >
                <Glyph name="clock" />
                All day
                <span className="rem-time-hint">Add time</span>
              </button>
            )}
          </div>

          {draft.links.length > 0 && (
            <div className="rem-link-rows" ref={linksRef}>
              {draft.links.map((link, index) => (
                <label className="rem-link-row" key={index}>
                  <Glyph name="link" />
                  <input
                    value={link}
                    onChange={(e) => setLink(index, e.target.value)}
                    placeholder="Paste a link: Meet, Zoom, a doc…"
                    aria-label={`Link ${index + 1}`}
                    inputMode="url"
                  />
                  <button
                    type="button"
                    className="rem-link-remove"
                    aria-label="Remove link"
                    onClick={() =>
                      set(
                        "links",
                        draft.links.filter((_, i) => i !== index),
                      )
                    }
                  >
                    <Glyph name="close" />
                  </button>
                </label>
              ))}
            </div>
          )}

          {noteOpen && (
            <textarea
              ref={noteRef}
              className="rem-note"
              value={draft.details}
              rows={2}
              maxLength={2000}
              onChange={(e) => set("details", e.target.value)}
              placeholder="Add a note"
              aria-label="Note"
            />
          )}

          <div className="rem-composer-foot">
            {draft.links.length < MAX_LINKS && (
              <button type="button" className="rem-ghost" onClick={addLink}>
                <Glyph name="link" />
                {draft.links.length ? "Another link" : "Link"}
              </button>
            )}
            {!noteOpen && (
              <button type="button" className="rem-ghost" onClick={openNote}>
                <Glyph name="note" />
                Note
              </button>
            )}
            <span className="rem-spacer" />
            <button
              type="button"
              className="rem-ghost"
              onClick={onCancel ?? discard}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="rem-save"
              disabled={!draft.title.trim() || saving}
            >
              {editing ? "Save" : "Add reminder"}
              <kbd aria-hidden="true">↵</kbd>
            </button>
          </div>
        </div>
      )}
    </form>
  );
}

function ReminderRow({
  reminder,
  onEdit,
}: {
  reminder: Reminder;
  onEdit: () => void;
}) {
  const done = reminder.status === "done";
  const ringing = reminder.status === "ringing";
  // Saved for a time already past today: listed, but it will not ring.
  const silent =
    reminder.status === "pending" &&
    reminder.ring_at_ms === null &&
    reminder.date >= isoDay(new Date());

  return (
    <li className="rem-row" data-status={reminder.status}>
      <button
        type="button"
        className="rem-check"
        aria-label={done ? "Mark not done" : "Mark done"}
        title={done ? "Mark not done" : "Mark done"}
        onClick={() =>
          void attempt(() => commands.remindersSetDone(reminder.id, !done))
        }
      >
        {done && <Glyph name="check" />}
      </button>
      <div className="rem-main">
        <div className="rem-title">{reminder.title}</div>
        <div className="rem-meta">
          <span className="rem-when">{whenLabel(reminder)}</span>
          {silent && !done && (
            <span
              className="rem-tag"
              title="Its time has passed; it won't ring"
            >
              Won't ring
            </span>
          )}
          {reminder.source === "agent" && (
            <span className="rem-tag" title="Created by the Agent">
              Agent
            </span>
          )}
          {reminder.details && (
            <span className="rem-detail-text">{reminder.details}</span>
          )}
        </div>
      </div>
      <div className="rem-row-end">
        {reminder.links.map((link, index) => (
          <button
            type="button"
            key={link.url}
            className="rem-open"
            title={link.url}
            onClick={() =>
              void attempt(() => commands.remindersOpenLink(reminder.id, index))
            }
          >
            Open <span>{linkLabel(link.url)}</span>
          </button>
        ))}
        {ringing && (
          <button
            type="button"
            className="rem-ghost"
            onClick={() =>
              void attempt(() => commands.remindersSnooze(reminder.id, 10))
            }
          >
            Snooze
          </button>
        )}
        <div className="rem-row-tools">
          <button
            type="button"
            className="rem-icon-button"
            aria-label="Edit reminder"
            title="Edit"
            onClick={onEdit}
          >
            <Glyph name="pencil" />
          </button>
          <button
            type="button"
            className="rem-icon-button danger"
            aria-label="Delete reminder"
            title="Delete"
            onClick={() =>
              void attempt(() => commands.remindersDelete(reminder.id))
            }
          >
            <Glyph name="trash" />
          </button>
        </div>
      </div>
    </li>
  );
}

function Group({
  group,
  editing,
  setEditing,
}: {
  group: ReminderGroup;
  editing: string | null;
  setEditing: (id: string | null) => void;
}) {
  const [open, setOpen] = useState(group.tone !== "done");
  return (
    <section className="rem-group" data-tone={group.tone}>
      <header className="rem-group-head">
        {group.tone === "done" ? (
          <button
            type="button"
            className="rem-group-toggle"
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            <h2>{group.label}</h2>
            <span className="rem-count">{group.items.length}</span>
          </button>
        ) : (
          <>
            <h2>{group.label}</h2>
            <span className="rem-count">{group.items.length}</span>
          </>
        )}
        {group.tone === "done" && open && (
          <button
            type="button"
            className="rem-ghost rem-clear"
            onClick={() => void attempt(() => commands.remindersClearDone())}
          >
            Clear
          </button>
        )}
      </header>
      {open && (
        <ul className="rem-list">
          {group.items.map((reminder) =>
            editing === reminder.id ? (
              <li key={reminder.id} className="rem-row-editing">
                <Composer
                  initial={draftFrom(reminder)}
                  autoFocus
                  onDone={() => setEditing(null)}
                  onCancel={() => setEditing(null)}
                />
              </li>
            ) : (
              <ReminderRow
                key={reminder.id}
                reminder={reminder}
                onEdit={() => setEditing(reminder.id)}
              />
            ),
          )}
        </ul>
      )}
    </section>
  );
}

export function RemindersPage() {
  const { reminders, now } = useReminders();
  const [editing, setEditing] = useState<string | null>(null);
  const today = isoDay(now);
  const groups = useMemo(
    () => (reminders ? groupReminders(reminders, now) : []),
    [reminders, now],
  );
  const upcoming = reminders?.filter((r) => r.status !== "done").length ?? 0;
  // A day picked on the calendar starts a fresh composer on that day.
  const [seed, setSeed] = useState<{ day: string; n: number } | null>(null);
  const composerRef = useRef<HTMLDivElement>(null);
  const addOn = useCallback((day: string) => {
    setSeed((current) => ({ day, n: (current?.n ?? 0) + 1 }));
    requestAnimationFrame(() =>
      composerRef.current?.scrollIntoView({
        block: "nearest",
        behavior: "smooth",
      }),
    );
  }, []);
  const run = useCallback(
    (
      call: () => Promise<
        { status: "ok"; data: unknown } | { status: "error"; error: string }
      >,
    ) => void attempt(call),
    [],
  );

  return (
    <section className="page active reminders-page" data-page-panel="reminders">
      <div className="page-wrap reminders-wrap">
        <header className="page-header">
          <div>
            <h1>Reminders</h1>
          </div>
        </header>

        {reminders !== null && (
          <ReminderCalendar
            reminders={reminders}
            today={today}
            run={run}
            onAdd={addOn}
          />
        )}

        <div ref={composerRef} className="rem-composer-slot">
          <Composer
            key={`${today}:${seed?.n ?? 0}`}
            initial={emptyDraft(seed?.day ?? today)}
            autoFocus={seed !== null}
            onDone={() => {}}
          />
        </div>

        {reminders === null ? null : groups.length === 0 ? (
          <div className="rem-empty">
            <div className="rem-empty-mark">
              <Glyph name="bell" />
            </div>
            <p>Nothing to remember yet.</p>
            <span>Reminders you add, or ask the Agent for, show up here.</span>
          </div>
        ) : (
          <>
            {upcoming === 0 && (
              <p className="rem-caught-up">You’re all caught up.</p>
            )}
            {groups.map((group) => (
              <Group
                key={group.key}
                group={group}
                editing={editing}
                setEditing={setEditing}
              />
            ))}
          </>
        )}
      </div>
    </section>
  );
}
