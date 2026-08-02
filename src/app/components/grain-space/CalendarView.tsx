import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlarmClock, ChevronLeft, ChevronRight } from "lucide-react";
import type { NoteCard } from "@/bindings";

/**
 * [GRAIN] Calendar view — a month grid spanning the sheet, with the notes that
 * carry an armed/fired reminder surfaced as events below. Derives entirely from
 * the sidebar cards (reminder_state.fire_at); its only state is the visible
 * month + an optional day filter, so it costs nothing while unmounted.
 */

const monthFmt = new Intl.DateTimeFormat(undefined, {
  month: "long",
  year: "numeric",
});
const clockFmt = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});
const dayFmt = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
});
const DOW = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** Local Y-M-D key so reminders group by calendar day, not UTC. */
const dayKey = (ms: number) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
};

type Props = {
  reminders: NoteCard[];
  onSelectCard: (card: NoteCard) => void;
};

export function CalendarView({ reminders, onSelectCard }: Props) {
  const { t } = useTranslation();
  const today = new Date();
  const [view, setView] = useState({
    year: today.getFullYear(),
    month: today.getMonth(),
  });
  const [selected, setSelected] = useState<string | null>(null);

  const byDay = useMemo(() => {
    const m = new Map<string, number>();
    for (const r of reminders) {
      const at = r.reminder_state.fire_at;
      if (at == null) continue;
      const k = dayKey(at);
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return m;
  }, [reminders]);

  const first = new Date(view.year, view.month, 1);
  const daysInMonth = new Date(view.year, view.month + 1, 0).getDate();
  const startDow = first.getDay();
  const cells: (number | null)[] = [];
  for (let i = 0; i < startDow; i++) cells.push(null);
  for (let d = 1; d <= daysInMonth; d++) cells.push(d);
  while (cells.length % 7 !== 0) cells.push(null);

  const keyFor = (day: number) => `${view.year}-${view.month}-${day}`;
  const isToday = (day: number) =>
    view.year === today.getFullYear() &&
    view.month === today.getMonth() &&
    day === today.getDate();

  const step = (delta: number) =>
    setView((v) => {
      const d = new Date(v.year, v.month + delta, 1);
      return { year: d.getFullYear(), month: d.getMonth() };
    });
  const goToday = () => {
    setView({ year: today.getFullYear(), month: today.getMonth() });
    setSelected(null);
  };

  const list = useMemo(
    () =>
      selected
        ? reminders.filter(
            (r) =>
              r.reminder_state.fire_at != null &&
              dayKey(r.reminder_state.fire_at) === selected,
          )
        : reminders,
    [reminders, selected],
  );

  const listLabel = selected
    ? (() => {
        const [y, m, d] = selected.split("-").map(Number);
        return dayFmt.format(new Date(y, m, d));
      })()
    : t("grainSpaceOverlay.upcoming");

  const now = Date.now();

  return (
    <section className="gs-cal-view">
      <div className="gs-cal-view-head">
        <span className="gs-cal-view-title">{monthFmt.format(first)}</span>
        <div className="gs-cal-nav">
          <button type="button" className="gs-cal-today-btn" onClick={goToday}>
            {t("grainSpaceOverlay.today")}
          </button>
          <button
            type="button"
            onClick={() => step(-1)}
            aria-label="Previous month"
          >
            <ChevronLeft width={16} height={16} />
          </button>
          <button type="button" onClick={() => step(1)} aria-label="Next month">
            <ChevronRight width={16} height={16} />
          </button>
        </div>
      </div>

      <div className="gs-cal-month">
        <div className="gs-cal-dow-row">
          {DOW.map((d) => (
            <div key={d} className="gs-cal-dow-cell">
              {d}
            </div>
          ))}
        </div>
        <div className="gs-cal-days">
          {cells.map((day, i) => {
            if (day === null)
              return <div key={i} className="gs-cal-cell gs-cal-cell--empty" />;
            const k = keyFor(day);
            const count = byDay.get(k) ?? 0;
            const cls = [
              "gs-cal-cell",
              isToday(day) ? "gs-cal-cell--today" : "",
              selected === k ? "gs-cal-cell--sel" : "",
            ]
              .filter(Boolean)
              .join(" ");
            return (
              <button
                key={i}
                type="button"
                className={cls}
                onClick={() => setSelected((s) => (s === k ? null : k))}
              >
                <span className="gs-cal-num">{day}</span>
                {count > 0 && (
                  <span className="gs-cal-dots">
                    {Array.from({ length: Math.min(count, 3) }).map((_, j) => (
                      <span key={j} />
                    ))}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </div>

      <div className="gs-cal-list">
        <div className="gs-cal-list-head">{listLabel}</div>
        {list.length === 0 ? (
          <div className="gs-cal-empty">
            {t("grainSpaceOverlay.noReminders")}
          </div>
        ) : (
          list.map((r) => {
            const at = r.reminder_state.fire_at;
            if (at == null) return null;
            const past = at < now;
            return (
              <button
                key={r.id}
                type="button"
                className={`gs-cal-ev${past ? " gs-cal-ev--past" : ""}`}
                onClick={() => onSelectCard(r)}
              >
                <span className="gs-cal-ev-time">
                  <span className="gs-cal-ev-day">
                    {dayFmt.format(new Date(at))}
                  </span>
                  <span className="gs-cal-ev-clock">
                    {clockFmt.format(new Date(at))}
                  </span>
                </span>
                <span className="gs-cal-ev-body">
                  <span className="gs-cal-ev-title">
                    {r.title.trim() || t("grainSpaceOverlay.untitled")}
                  </span>
                  {r.tldr.trim() && (
                    <span className="gs-cal-ev-sub">{r.tldr}</span>
                  )}
                </span>
                <AlarmClock className="gs-cal-ev-icon" width={15} height={15} />
              </button>
            );
          })
        )}
      </div>
    </section>
  );
}
