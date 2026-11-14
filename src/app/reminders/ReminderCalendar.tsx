/* eslint-disable i18next/no-literal-string -- Reminders copy is untranslated until the translation pass. */
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { commands, type Reminder } from "@/bindings";
import { hashForRoute } from "@/navigation";
import {
  countByDay,
  isMeeting,
  isoDay,
  linkInfo,
  monthGrid,
  parseIsoDay,
  relativeDay,
  remindersByDay,
  shiftMonth,
  shortTime,
  weekGrid,
  weekStart,
  whenLabel,
} from "./reminderFormat";

/** The day sheet's width, in CSS px. */
const SHEET_WIDTH = 320;
/** Weeks the default view shows. */
const SPAN_WEEKS = 2;
/** Entries a day tile lists before "+N more", per view. */
const TILE_ITEMS = { weeks: 3, month: 2 } as const;
const VIEW_KEY = "grain.reminders.calendarView";

type View = "weeks" | "month";

function Glyph({ name }: { name: string }) {
  return (
    <svg className="icon sm" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
}

function readView(): View {
  try {
    return localStorage.getItem(VIEW_KEY) === "month" ? "month" : "weeks";
  } catch {
    return "weeks";
  }
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** Recordings per day, read once per visit to the tab (counts only). */
function useRecordedDays(): Map<string, number> {
  const [days, setDays] = useState<Map<string, number>>(() => new Map());
  useEffect(() => {
    let dead = false;
    void commands
      .meetList()
      .then((result) => {
        if (dead || result.status !== "ok") return;
        setDays(countByDay(result.data.map((m) => m.created_unix_ms)));
      })
      .catch(console.error);
    return () => {
      dead = true;
    };
  }, []);
  return days;
}

interface Props {
  reminders: Reminder[];
  today: string;
  /** Run a reminder command, surfacing its error. */
  run: (
    call: () => Promise<
      { status: "ok"; data: unknown } | { status: "error"; error: string }
    >,
  ) => void;
  /** Start a new reminder on `day` in the composer. */
  onAdd: (day: string) => void;
}

/** One day's sheet: its recordings, its reminders, and a way to add more. */
function DaySheet({
  day,
  today,
  items,
  recorded,
  style,
  run,
  onAdd,
  onClose,
}: {
  day: string;
  today: string;
  items: Reminder[];
  recorded: number;
  style: CSSProperties;
  run: Props["run"];
  onAdd: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const near = relativeDay(day, today);
  const label = parseIsoDay(day).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("button")?.focus();
  }, [day]);

  return (
    <div
      ref={ref}
      className="rem-cal-sheet"
      role="dialog"
      aria-label={label}
      style={style}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }}
    >
      <header className="rem-cal-sheet-head">
        <h3>{label}</h3>
        {near !== null && (
          <span className="rem-tag">
            {near === 0 ? "Today" : near === 1 ? "Tomorrow" : "Yesterday"}
          </span>
        )}
        <button
          type="button"
          className="rem-icon-button rem-cal-sheet-close"
          aria-label="Close"
          onClick={onClose}
        >
          <Glyph name="close" />
        </button>
      </header>

      {recorded > 0 && (
        <a
          className="rem-cal-recorded"
          href={hashForRoute({ page: "meet", day })}
        >
          <span className="rem-cal-recorded-icon">
            <Glyph name="mic" />
          </span>
          <span className="rem-cal-recorded-text">
            {plural(recorded, "meeting", "meetings")} on this day
            <small>Open in Recall</small>
          </span>
          <Glyph name="chev-right" />
        </a>
      )}

      {items.length > 0 && (
        <ul className="rem-cal-sheet-list">
          {items.map((reminder) => {
            const done = reminder.status === "done";
            return (
              <li
                key={reminder.id}
                className="rem-cal-sheet-item"
                data-status={reminder.status}
              >
                <button
                  type="button"
                  className="rem-check"
                  aria-label={done ? "Mark not done" : "Mark done"}
                  title={done ? "Mark not done" : "Mark done"}
                  onClick={() =>
                    run(() => commands.remindersSetDone(reminder.id, !done))
                  }
                >
                  {done && <Glyph name="check" />}
                </button>
                <div className="rem-cal-sheet-text">
                  <span className="rem-cal-sheet-title">{reminder.title}</span>
                  <span className="rem-cal-sheet-when">
                    {whenLabel(reminder)}
                    {reminder.details && <> · {reminder.details}</>}
                  </span>
                  {reminder.links.length > 0 && (
                    <span className="rem-cal-sheet-links">
                      {reminder.links.map((link, index) => {
                        const info = linkInfo(link.url);
                        return (
                          <button
                            type="button"
                            key={link.url}
                            className="rem-cal-link"
                            title={link.url}
                            onClick={() =>
                              run(() =>
                                commands.remindersOpenLink(reminder.id, index),
                              )
                            }
                          >
                            <Glyph name="link" />
                            {info.kind === "meeting"
                              ? `Join ${info.name}`
                              : info.name}
                          </button>
                        );
                      })}
                    </span>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {items.length === 0 && recorded === 0 && (
        <p className="rem-cal-sheet-empty">Nothing on this day.</p>
      )}

      <button type="button" className="rem-cal-sheet-add" onClick={onAdd}>
        <Glyph name="plus" />
        Add a reminder on this day
      </button>
    </div>
  );
}

/**
 * The calendar strip: two weeks by default (what is on each day, one line),
 * or a compact month of dots. Click a day for its sheet.
 */
export function ReminderCalendar({ reminders, today, run, onAdd }: Props) {
  const [view, setView] = useState<View>(readView);
  // A day inside the span on show; the arrows move it.
  const [anchor, setAnchor] = useState(today);
  const [open, setOpen] = useState<{
    day: string;
    style: CSSProperties;
  } | null>(null);
  // The one day cell in the tab order (arrow keys move it).
  const [cursor, setCursor] = useState(today);
  const gridRef = useRef<HTMLDivElement>(null);
  const recordedDays = useRecordedDays();

  const firstDay = useMemo(() => weekStart(), []);
  const days = useMemo(
    () =>
      view === "month"
        ? monthGrid(anchor, firstDay)
        : weekGrid(anchor, firstDay, SPAN_WEEKS),
    [view, anchor, firstDay],
  );
  const byDay = useMemo(() => remindersByDay(reminders), [reminders]);
  const monthKey = anchor.slice(0, 7);
  const weekdays = useMemo(
    () =>
      days
        .slice(0, 7)
        .map((day) =>
          parseIsoDay(day).toLocaleDateString(undefined, { weekday: "short" }),
        ),
    [days],
  );
  const title = useMemo(() => {
    if (view === "month")
      return parseIsoDay(anchor).toLocaleDateString(undefined, {
        month: "long",
        year: "numeric",
      });
    const first = parseIsoDay(days[0]);
    const last = parseIsoDay(days[days.length - 1]);
    const month = (date: Date) =>
      date.toLocaleDateString(undefined, { month: "long" });
    return first.getMonth() === last.getMonth()
      ? `${month(first)} ${first.getFullYear()}`
      : `${month(first)} – ${month(last)} ${last.getFullYear()}`;
  }, [view, anchor, days]);
  const tabDay = days.includes(cursor)
    ? cursor
    : days.includes(today)
      ? today
      : days[0];

  // An open sheet closes on a click anywhere outside it.
  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Element | null;
      if (target?.closest(".rem-cal-sheet, .rem-cal-day")) return;
      setOpen(null);
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [open]);

  const focusDay = (day: string) =>
    requestAnimationFrame(() =>
      gridRef.current
        ?.querySelector<HTMLElement>(`[data-day="${day}"]`)
        ?.focus(),
    );

  const shift = (direction: -1 | 1) => {
    setOpen(null);
    if (view === "month") {
      setAnchor(shiftMonth(anchor, direction));
      return;
    }
    const date = parseIsoDay(anchor);
    date.setDate(date.getDate() + direction * 7);
    setAnchor(isoDay(date));
  };

  const switchView = (next: View) => {
    setOpen(null);
    setView(next);
    // Keep today in sight when it is in the span being left.
    if (days.includes(today)) setAnchor(today);
    try {
      localStorage.setItem(VIEW_KEY, next);
    } catch {
      // A per-viewer convenience only.
    }
  };

  /** Open `day`'s sheet under its cell (over it near the bottom of a month). */
  const openDay = (day: string, cell: HTMLElement) => {
    if (open?.day === day) {
      setOpen(null);
      return;
    }
    const grid = gridRef.current;
    if (!grid) return;
    const width = Math.min(SHEET_WIDTH, grid.clientWidth);
    const center = cell.offsetLeft + cell.offsetWidth / 2;
    const left = Math.min(
      Math.max(0, center - width / 2),
      grid.clientWidth - width,
    );
    const row = Math.floor(days.indexOf(day) / 7);
    const rows = days.length / 7;
    const style: CSSProperties = { left, width };
    if (view === "month" && row >= rows - 2)
      style.bottom = grid.clientHeight - cell.offsetTop + 6;
    else style.top = cell.offsetTop + cell.offsetHeight + 6;
    setCursor(day);
    setOpen({ day, style });
  };

  const onGridKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -7,
      ArrowDown: 7,
    }[event.key];
    // Arrow keys inside an open sheet are the sheet's own.
    if (!step || (event.target as Element).closest(".rem-cal-sheet")) return;
    event.preventDefault();
    const date = parseIsoDay(tabDay);
    date.setDate(date.getDate() + step);
    const next = isoDay(date);
    setOpen(null);
    setCursor(next);
    if (!days.includes(next)) setAnchor(next);
    focusDay(next);
  };

  return (
    <section className="rem-cal" data-view={view} aria-label="Calendar">
      <header className="rem-cal-head">
        <h2>{title}</h2>
        <span className="rem-spacer" />
        <div className="rem-cal-views" role="group" aria-label="Calendar view">
          {(["weeks", "month"] as const).map((option) => (
            <button
              type="button"
              key={option}
              aria-pressed={view === option}
              onClick={() => switchView(option)}
            >
              {option === "weeks" ? "2 weeks" : "Month"}
            </button>
          ))}
        </div>
        <div className="rem-cal-nav">
          <button
            type="button"
            aria-label={view === "month" ? "Previous month" : "Previous week"}
            onClick={() => shift(-1)}
          >
            <Glyph name="chev-left" />
          </button>
          <button
            type="button"
            className="rem-cal-nav-today"
            disabled={days.includes(today)}
            onClick={() => {
              setOpen(null);
              setAnchor(today);
              setCursor(today);
            }}
          >
            Today
          </button>
          <button
            type="button"
            aria-label={view === "month" ? "Next month" : "Next week"}
            onClick={() => shift(1)}
          >
            <Glyph name="chev-right" />
          </button>
        </div>
      </header>

      {/* Two weeks name each day in its own tile. */}
      {view === "month" && (
        <div className="rem-cal-weekdays" aria-hidden="true">
          {weekdays.map((weekday) => (
            <span key={weekday}>{weekday}</span>
          ))}
        </div>
      )}

      <div className="rem-cal-grid" ref={gridRef} onKeyDown={onGridKey}>
        {days.map((day) => {
          const items = byDay.get(day) ?? [];
          const recorded = recordedDays.get(day) ?? 0;
          const outside = view === "month" && !day.startsWith(monthKey);
          const limit = TILE_ITEMS[view];
          const date = parseIsoDay(day);
          const summary = [
            items.length ? plural(items.length, "reminder", "reminders") : "",
            recorded ? plural(recorded, "meeting", "meetings") : "",
          ]
            .filter(Boolean)
            .join(" · ");
          const label = date.toLocaleDateString(undefined, {
            weekday: "long",
            day: "numeric",
            month: "long",
          });
          return (
            <button
              type="button"
              key={day}
              data-day={day}
              className="rem-cal-day"
              data-outside={outside ? "" : undefined}
              data-today={day === today ? "" : undefined}
              data-past={day < today ? "" : undefined}
              data-first={date.getDate() === 1 ? "" : undefined}
              aria-pressed={open?.day === day}
              aria-label={summary ? `${label}, ${summary}` : label}
              title={summary || undefined}
              tabIndex={day === tabDay ? 0 : -1}
              onClick={(event) => openDay(day, event.currentTarget)}
            >
              <span className="rem-cal-day-top">
                {view === "weeks" ? (
                  <span className="rem-cal-wd">
                    {day === today
                      ? "Today"
                      : date.toLocaleDateString(undefined, {
                          weekday: "short",
                        })}
                    {date.getDate() === 1 &&
                      ` · ${date.toLocaleDateString(undefined, { month: "short" })}`}
                  </span>
                ) : (
                  <span className="rem-cal-date">{date.getDate()}</span>
                )}
                {recorded > 0 && (
                  <span className="rem-cal-recorded-mark">
                    <Glyph name="mic" />
                    {recorded > 1 && recorded}
                  </span>
                )}
              </span>
              {view === "weeks" && (
                <span className="rem-cal-date">{date.getDate()}</span>
              )}
              {items.length > 0 && (
                <span className="rem-cal-agenda">
                  {items.slice(0, limit).map((reminder) => (
                    <span
                      key={reminder.id}
                      className="rem-cal-entry"
                      data-status={reminder.status}
                      data-allday={reminder.time ? undefined : ""}
                      data-meeting={isMeeting(reminder) ? "" : undefined}
                    >
                      {reminder.time && <span className="rem-cal-dot" />}
                      {reminder.time && (
                        <span className="rem-cal-entry-time">
                          {shortTime(reminder.time)}
                        </span>
                      )}
                      <span className="rem-cal-entry-title">
                        {reminder.title}
                      </span>
                    </span>
                  ))}
                  {items.length > limit && (
                    <span className="rem-cal-more">
                      +{items.length - limit} more
                    </span>
                  )}
                </span>
              )}
            </button>
          );
        })}

        {open && (
          <DaySheet
            day={open.day}
            today={today}
            items={byDay.get(open.day) ?? []}
            recorded={recordedDays.get(open.day) ?? 0}
            style={open.style}
            run={run}
            onAdd={() => {
              setOpen(null);
              onAdd(open.day);
            }}
            onClose={() => {
              const day = open.day;
              setOpen(null);
              focusDay(day);
            }}
          />
        )}
      </div>
    </section>
  );
}
