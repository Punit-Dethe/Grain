/* eslint-disable i18next/no-literal-string -- Reminders copy is untranslated until the translation pass. */
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
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

function emptyDraft(today: string): Draft {
  return {
    id: null,
    title: "",
    date: today,
    time: "",
    links: [""],
    details: "",
  };
}

function draftFrom(reminder: Reminder): Draft {
  return {
    id: reminder.id,
    title: reminder.title,
    date: reminder.date,
    time: reminder.time ?? "",
    links: reminder.links.length ? reminder.links.map((l) => l.url) : [""],
    details: reminder.details ?? "",
  };
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
  onCancel?: () => void;
}) {
  const [draft, setDraft] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [expanded, setExpanded] = useState(
    initial.id !== null || initial.details !== "",
  );
  const titleRef = useRef<HTMLInputElement>(null);
  const editing = initial.id !== null;

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
    if (editing) {
      onDone();
    } else {
      // Keep the day and time: several reminders for one day are common.
      setDraft((current) => ({
        ...current,
        title: "",
        links: [""],
        details: "",
      }));
      titleRef.current?.focus();
      onDone();
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && onCancel) {
      event.preventDefault();
      onCancel();
    }
  };

  return (
    <form
      className={`rem-composer${editing ? " is-editing" : ""}`}
      onSubmit={submit}
      onKeyDown={onKeyDown}
    >
      <div className="rem-composer-title">
        <Glyph name={editing ? "pencil" : "plus"} />
        <input
          ref={titleRef}
          value={draft.title}
          maxLength={200}
          onChange={(e) => set("title", e.target.value)}
          onFocus={() => setExpanded(true)}
          placeholder="Remind me about…"
          aria-label="What to be reminded about"
        />
      </div>
      {expanded && (
        <div className="rem-composer-body">
          <div className="rem-fields">
            <label className="rem-field">
              <span>Day</span>
              <input
                type="date"
                value={draft.date}
                required
                onChange={(e) => set("date", e.target.value)}
              />
            </label>
            <label className="rem-field">
              <span>Time</span>
              <input
                type="time"
                value={draft.time}
                onChange={(e) => set("time", e.target.value)}
              />
              {draft.time ? (
                <button
                  type="button"
                  className="rem-field-clear"
                  onClick={() => set("time", "")}
                  aria-label="Make it all-day"
                  title="Make it all-day"
                >
                  ×
                </button>
              ) : (
                <em>All day</em>
              )}
            </label>
          </div>
          <div className="rem-link-fields">
            {draft.links.map((link, index) => (
              <label className="rem-field rem-field-link" key={index}>
                <Glyph name="link" />
                <input
                  value={link}
                  onChange={(e) => setLink(index, e.target.value)}
                  placeholder="Link to open (e.g. https://meet.google.com/…)"
                  aria-label={`Link ${index + 1}`}
                  inputMode="url"
                />
                {draft.links.length > 1 && (
                  <button
                    type="button"
                    className="rem-field-clear"
                    aria-label="Remove link"
                    onClick={() =>
                      set(
                        "links",
                        draft.links.filter((_, i) => i !== index),
                      )
                    }
                  >
                    ×
                  </button>
                )}
              </label>
            ))}
          </div>
          <textarea
            className="rem-details"
            value={draft.details}
            rows={draft.details ? 2 : 1}
            maxLength={2000}
            onChange={(e) => set("details", e.target.value)}
            placeholder="Note (optional)"
            aria-label="Note"
          />
          <div className="rem-composer-actions">
            {draft.links.length < 5 && (
              <button
                type="button"
                className="rem-ghost"
                onClick={() => set("links", [...draft.links, ""])}
              >
                <Glyph name="plus" />
                Add link
              </button>
            )}
            <span className="rem-spacer" />
            {onCancel && (
              <button type="button" className="button" onClick={onCancel}>
                Cancel
              </button>
            )}
            <button
              type="submit"
              className="button primary"
              disabled={!draft.title.trim() || saving}
            >
              {editing ? "Save" : "Add reminder"}
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

  return (
    <section className="page active reminders-page" data-page-panel="reminders">
      <div className="page-wrap reminders-wrap">
        <header className="page-header">
          <div>
            <h1>Reminders</h1>
            <p className="page-subtitle">
              Grain rings at the time you set, with a button for any link. Add
              one here, or ask the Agent: “remind me Tuesday at 6 pm about the
              review, here’s the Meet link”.
            </p>
          </div>
        </header>

        <Composer key={today} initial={emptyDraft(today)} onDone={() => {}} />

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
