/* eslint-disable i18next/no-literal-string -- Reminders copy is untranslated until the translation pass. */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { commands, events, type Reminder } from "@/bindings";
import {
  dayLabel,
  isMissed,
  isoDay,
  linkLabel,
  whenLabel,
} from "../reminderFormat";

/** Errors here have nowhere better to go than the log: the card stays put. */
async function run(
  action: () => Promise<{ status: "ok" } | { status: "error"; error: string }>,
) {
  try {
    const result = await action();
    if (result.status === "error") console.warn(result.error);
  } catch (error) {
    console.warn(error);
  }
}

function BellMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z" />
      <path d="M10 20.5a2 2 0 0 0 4 0" />
    </svg>
  );
}

function useTheme() {
  useEffect(() => {
    const apply = (resolved: string) => {
      document.documentElement.dataset.theme = resolved;
    };
    void commands.getTheme().then((state) => apply(state.resolved));
    let stop: (() => void) | undefined;
    let disposed = false;
    void events.themeChanged
      .listen(({ payload }) => apply(payload.resolved))
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);
}

function useRinging() {
  const [ringing, setRinging] = useState<Reminder[]>([]);
  const reload = useCallback(async () => {
    const result = await commands.remindersList();
    if (result.status === "ok") {
      setRinging(
        result.data
          .filter((r) => r.status === "ringing")
          .sort(
            (a, b) =>
              a.date.localeCompare(b.date) ||
              (a.time ?? "").localeCompare(b.time ?? ""),
          ),
      );
    }
  }, []);
  useEffect(() => {
    void reload();
    let stop: (() => void) | undefined;
    let disposed = false;
    void events.remindersChanged
      .listen(() => void reload())
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [reload]);
  return ringing;
}

function Card({ reminder, now }: { reminder: Reminder; now: Date }) {
  const today = isoDay(now);
  const missed = isMissed(reminder, now);
  const day =
    reminder.date === today ? "" : `${dayLabel(reminder.date, today)} · `;
  return (
    <article className="rp-card" data-missed={missed || undefined}>
      <div className="rp-head">
        <span className="rp-mark">
          <BellMark />
        </span>
        <span className="rp-when">
          {missed ? "Missed · " : ""}
          {day}
          {whenLabel(reminder)}
        </span>
      </div>
      <h1 className="rp-title">{reminder.title}</h1>
      {reminder.details && <p className="rp-details">{reminder.details}</p>}
      {reminder.links.length > 0 && (
        <div className="rp-links">
          {reminder.links.map((link, index) => (
            <button
              type="button"
              key={link.url}
              className="rp-open"
              title={link.url}
              onClick={() =>
                void run(() => commands.remindersOpenLink(reminder.id, index))
              }
            >
              Open <span>{linkLabel(link.url)}</span>
            </button>
          ))}
        </div>
      )}
      <div className="rp-actions">
        <button
          type="button"
          className="rp-quiet"
          onClick={() =>
            void run(() => commands.remindersSnooze(reminder.id, 10))
          }
        >
          Snooze 10 min
        </button>
        <button
          type="button"
          className="rp-done"
          onClick={() =>
            void run(() => commands.remindersSetDone(reminder.id, true))
          }
        >
          Done
        </button>
      </div>
    </article>
  );
}

export function ReminderPopup() {
  useTheme();
  const ringing = useRinging();
  const rootRef = useRef<HTMLDivElement>(null);
  const [now] = useState(() => new Date());

  // The window is as tall as its cards; the backend clamps and keeps the top.
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    let last = 0;
    const observer = new ResizeObserver(() => {
      const height = Math.ceil(root.getBoundingClientRect().height);
      if (height && height !== last) {
        last = height;
        void commands.remindersPopupResize(height);
      }
    });
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  return (
    <div className="rp-root" ref={rootRef}>
      {ringing.map((reminder) => (
        <Card key={reminder.id} reminder={reminder} now={now} />
      ))}
      {ringing.length > 1 && (
        <button
          type="button"
          className="rp-all"
          onClick={() => void run(() => commands.remindersShowInApp())}
        >
          Open Reminders
        </button>
      )}
    </div>
  );
}
