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
import {
  isMeeting,
  isoDay,
  linkInfo,
  monthGrid,
  parseIsoDay,
  relativeDay,
  remindersByDay,
  shiftMonth,
  weekStart,
  whenLabel,
} from "./reminderFormat";

/** Items a day cell lists before "+N more". */
const CELL_ITEMS = 2;
/** The day sheet's width, in CSS px (`.rem-cal-sheet` in reminders.css). */
const SHEET_WIDTH = 320;

function Glyph({ name }: { name: string }) {
  return (
    <svg className="icon sm" aria-hidden="true">
      <use href={`#i-${name}`} />
    </svg>
  );
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

/** One day's sheet: what is on it, with its links, and a way to add more. */
function DaySheet({
  day,
  today,
  items,
  style,
  run,
  onAdd,
  onClose,
}: {
  day: string;
  today: string;
  items: Reminder[];
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

      {items.length ? (
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
      ) : (
        <p className="rem-cal-sheet-empty">Nothing on this day.</p>
      )}

      <button type="button" className="rem-cal-sheet-add" onClick={onAdd}>
        <Glyph name="plus" />
        Add a reminder on this day
      </button>
    </div>
  );
}

/** A month at a glance: what is on each day; click a day for its sheet. */
export function ReminderCalendar({ reminders, today, run, onAdd }: Props) {
  const [month, setMonth] = useState(() => shiftMonth(today, 0));
  const [open, setOpen] = useState<{
    day: string;
    style: CSSProperties;
  } | null>(null);
  // The one day cell in the tab order (arrow keys move it).
  const [cursor, setCursor] = useState(today);
  const gridRef = useRef<HTMLDivElement>(null);

  const firstDay = useMemo(() => weekStart(), []);
  const days = useMemo(() => monthGrid(month, firstDay), [month, firstDay]);
  const byDay = useMemo(() => remindersByDay(reminders), [reminders]);
  const monthKey = month.slice(0, 7);
  const weekdays = useMemo(
    () =>
      days
        .slice(0, 7)
        .map((day) =>
          parseIsoDay(day).toLocaleDateString(undefined, { weekday: "short" }),
        ),
    [days],
  );
  const title = parseIsoDay(month).toLocaleDateString(undefined, {
    month: "long",
    year: "numeric",
  });
  const tabDay = days.includes(cursor)
    ? cursor
    : days.includes(today)
      ? today
      : `${monthKey}-01`;

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

  const goToMonth = (next: string) => {
    setOpen(null);
    setMonth(next);
  };

  /** Open `day`'s sheet beside its cell, over the calendar. */
  const openDay = (day: string, cell: HTMLElement) => {
    if (open?.day === day) {
      setOpen(null);
      return;
    }
    const grid = gridRef.current;
    if (!grid) return;
    const index = days.indexOf(day);
    const column = index % 7;
    const row = Math.floor(index / 7);
    const rows = days.length / 7;
    const style: CSSProperties = {};
    // Opens toward the room: right of a left-half cell, left of a right one;
    // below the upper rows, above the lower ones.
    if (column < 4) style.left = Math.max(0, cell.offsetLeft - 1);
    else
      style.right = Math.max(
        0,
        grid.clientWidth - cell.offsetLeft - cell.offsetWidth - 1,
      );
    if (row < rows - 2) style.top = cell.offsetTop + cell.offsetHeight + 6;
    else style.bottom = grid.clientHeight - cell.offsetTop + 6;
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
    if (!days.includes(next)) setMonth(shiftMonth(next, 0));
    focusDay(next);
  };

  return (
    <section className="rem-cal" aria-label="Calendar">
      <header className="rem-cal-head">
        <h2>{title}</h2>
        <span className="rem-spacer" />
        {month !== shiftMonth(today, 0) && (
          <button
            type="button"
            className="rem-ghost"
            onClick={() => {
              goToMonth(shiftMonth(today, 0));
              setCursor(today);
            }}
          >
            Today
          </button>
        )}
        <button
          type="button"
          className="rem-icon-button"
          aria-label="Previous month"
          onClick={() => goToMonth(shiftMonth(month, -1))}
        >
          <Glyph name="chev-left" />
        </button>
        <button
          type="button"
          className="rem-icon-button"
          aria-label="Next month"
          onClick={() => goToMonth(shiftMonth(month, 1))}
        >
          <Glyph name="chev-right" />
        </button>
      </header>

      <div className="rem-cal-weekdays" aria-hidden="true">
        {weekdays.map((weekday) => (
          <span key={weekday}>{weekday}</span>
        ))}
      </div>

      <div className="rem-cal-grid" ref={gridRef} onKeyDown={onGridKey}>
        {days.map((day) => {
          const items = byDay.get(day) ?? [];
          const outside = !day.startsWith(monthKey);
          const shown = items.slice(0, CELL_ITEMS);
          const more = items.length - shown.length;
          const label = parseIsoDay(day).toLocaleDateString(undefined, {
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
              aria-pressed={open?.day === day}
              aria-label={`${label}${items.length ? `, ${items.length} reminder${items.length === 1 ? "" : "s"}` : ""}`}
              tabIndex={day === tabDay ? 0 : -1}
              onClick={(event) => openDay(day, event.currentTarget)}
            >
              <span className="rem-cal-date">{parseIsoDay(day).getDate()}</span>
              {shown.map((reminder) => (
                <span
                  key={reminder.id}
                  className="rem-cal-item"
                  data-status={reminder.status}
                  data-meeting={isMeeting(reminder) ? "" : undefined}
                >
                  <span className="rem-cal-dot" />
                  <span className="rem-cal-item-title">{reminder.title}</span>
                </span>
              ))}
              {more > 0 && <span className="rem-cal-more">+{more} more</span>}
            </button>
          );
        })}

        {open && (
          <DaySheet
            day={open.day}
            today={today}
            items={byDay.get(open.day) ?? []}
            style={{ ...open.style, width: `min(${SHEET_WIDTH}px, 100%)` }}
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
