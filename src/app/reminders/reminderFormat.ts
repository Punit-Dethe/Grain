import type { Reminder } from "@/bindings";

/** All-day reminders ring at this local hour (matches the backend). */
export const ALL_DAY_RING_HOUR = 9;

export type GroupTone = "ringing" | "earlier" | "today" | "upcoming" | "done";

export interface ReminderGroup {
  key: string;
  label: string;
  tone: GroupTone;
  items: Reminder[];
}

const pad = (n: number) => String(n).padStart(2, "0");

/** Local calendar day as `YYYY-MM-DD`. */
export function isoDay(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Local midnight of a `YYYY-MM-DD` day. */
export function parseIsoDay(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

function addDays(iso: string, days: number): string {
  const date = parseIsoDay(iso);
  date.setDate(date.getDate() + days);
  return isoDay(date);
}

/** "6:00 PM" (locale-aware) for a stored `HH:MM`. */
export function timeLabel(time: string, locale?: string): string {
  const [h, m] = time.split(":").map(Number);
  return new Date(2000, 0, 1, h, m).toLocaleTimeString(locale, {
    hour: "numeric",
    minute: "2-digit",
  });
}

/** "Today", "Tomorrow", "Yesterday", or "Wed 25 Nov" (+ year when not this year). */
export function dayLabel(iso: string, today: string, locale?: string): string {
  if (iso === today) return "Today";
  if (iso === addDays(today, 1)) return "Tomorrow";
  if (iso === addDays(today, -1)) return "Yesterday";
  const date = parseIsoDay(iso);
  const sameYear = iso.slice(0, 4) === today.slice(0, 4);
  return date.toLocaleDateString(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

/** "6:00 PM", or "All day". */
export function whenLabel(reminder: Reminder, locale?: string): string {
  return reminder.time ? timeLabel(reminder.time, locale) : "All day";
}

/** The local instant a reminder is for (all-day = the ring hour). */
export function dueInstant(reminder: Reminder): Date {
  const date = parseIsoDay(reminder.date);
  if (reminder.time) {
    const [h, m] = reminder.time.split(":").map(Number);
    date.setHours(h, m, 0, 0);
  } else {
    date.setHours(ALL_DAY_RING_HOUR, 0, 0, 0);
  }
  return date;
}

/** A ringing reminder whose time passed more than two minutes ago. */
export function isMissed(reminder: Reminder, now: Date): boolean {
  return now.getTime() - dueInstant(reminder).getTime() > 2 * 60_000;
}

/** "meet.google.com" for a web link; the scheme for an app link. */
export function linkLabel(url: string): string {
  try {
    const parsed = new URL(url);
    if (parsed.protocol === "http:" || parsed.protocol === "https:") {
      return parsed.hostname.replace(/^www\./, "");
    }
    return parsed.protocol.replace(/:$/, "");
  } catch {
    return url;
  }
}

const byDue = (a: Reminder, b: Reminder) =>
  a.date.localeCompare(b.date) || (a.time ?? "").localeCompare(b.time ?? "");

/**
 * Ringing first, then past-but-open ("Earlier"), today, one group per later
 * day, and done last.
 */
export function groupReminders(
  reminders: Reminder[],
  now: Date,
  locale?: string,
): ReminderGroup[] {
  const today = isoDay(now);
  const ringing: Reminder[] = [];
  const earlier: Reminder[] = [];
  const done: Reminder[] = [];
  const days = new Map<string, Reminder[]>();
  for (const reminder of reminders) {
    if (reminder.status === "ringing") ringing.push(reminder);
    else if (reminder.status === "done") done.push(reminder);
    else if (reminder.date < today) earlier.push(reminder);
    else {
      const day = days.get(reminder.date) ?? [];
      day.push(reminder);
      days.set(reminder.date, day);
    }
  }
  const groups: ReminderGroup[] = [];
  if (ringing.length) {
    groups.push({
      key: "ringing",
      label: "Ringing now",
      tone: "ringing",
      items: ringing.sort(byDue),
    });
  }
  if (earlier.length) {
    groups.push({
      key: "earlier",
      label: "Earlier",
      tone: "earlier",
      items: earlier.sort(byDue),
    });
  }
  for (const day of [...days.keys()].sort()) {
    groups.push({
      key: day,
      label: dayLabel(day, today, locale),
      tone: day === today ? "today" : "upcoming",
      items: (days.get(day) ?? []).sort(byDue),
    });
  }
  if (done.length) {
    groups.push({
      key: "done",
      label: "Done",
      tone: "done",
      items: done.sort((a, b) => b.updated_ms - a.updated_ms),
    });
  }
  return groups;
}
