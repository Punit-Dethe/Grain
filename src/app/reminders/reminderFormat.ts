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

/** -1, 0 or 1 for yesterday, today or tomorrow; `null` for any other day. */
export function relativeDay(iso: string, today: string): -1 | 0 | 1 | null {
  if (iso === today) return 0;
  if (iso === addDays(today, 1)) return 1;
  if (iso === addDays(today, -1)) return -1;
  return null;
}

/** "Wed 25 Nov" (+ year when not this year). */
export function dateLabel(iso: string, today: string, locale?: string): string {
  const date = parseIsoDay(iso);
  const sameYear = iso.slice(0, 4) === today.slice(0, 4);
  return date.toLocaleDateString(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

const RELATIVE_DAY = { [-1]: "Yesterday", 0: "Today", 1: "Tomorrow" } as const;

/** "Today", "Tomorrow", "Yesterday", or "Wed 25 Nov" (the Reminders tab). */
export function dayLabel(iso: string, today: string, locale?: string): string {
  const near = relativeDay(iso, today);
  return near === null ? dateLabel(iso, today, locale) : RELATIVE_DAY[near];
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

export interface LinkInfo {
  /** "Google Meet", or the host for an ordinary page. */
  name: string;
  /** The rest worth showing: "meet.google.com/abc-defg-hij", a page path. */
  detail: string;
  /** A call to join, a web page, or an app link. */
  kind: "meeting" | "web" | "app";
}

const MEETING_HOSTS: Array<[RegExp, string]> = [
  [/(^|\.)meet\.google\.com$/, "Google Meet"],
  [/(^|\.)zoom\.us$/, "Zoom"],
  [/(^|\.)teams\.(microsoft|live)\.com$/, "Microsoft Teams"],
  [/(^|\.)webex\.com$/, "Webex"],
  [/(^|\.)whereby\.com$/, "Whereby"],
  [/(^|\.)gotomeeting\.com$/, "GoTo Meeting"],
];

const MEETING_SCHEMES: Record<string, string> = {
  zoommtg: "Zoom",
  zoomus: "Zoom",
  msteams: "Microsoft Teams",
};

/** How a reminder link reads: a named meeting, a page, or an app link. */
export function linkInfo(url: string): LinkInfo {
  try {
    const parsed = new URL(url);
    const scheme = parsed.protocol.replace(/:$/, "");
    if (scheme === "http" || scheme === "https") {
      const host = parsed.hostname.replace(/^www\./, "");
      const rest = `${parsed.pathname}${parsed.search}`.replace(/^\/$/, "");
      const meeting = MEETING_HOSTS.find(([pattern]) => pattern.test(host));
      if (meeting) {
        return { name: meeting[1], detail: `${host}${rest}`, kind: "meeting" };
      }
      return { name: host, detail: decodeURI(rest), kind: "web" };
    }
    const meeting = MEETING_SCHEMES[scheme];
    if (meeting) return { name: meeting, detail: url, kind: "meeting" };
    return { name: scheme, detail: url.slice(scheme.length + 1), kind: "app" };
  } catch {
    return { name: url, detail: "", kind: "web" };
  }
}

const byDue = (a: Reminder, b: Reminder) =>
  a.date.localeCompare(b.date) || (a.time ?? "").localeCompare(b.time ?? "");

/** The first day of the week for `locale`, as a `Date.getDay()` index
 * (0 = Sunday). Falls back to Monday where the runtime cannot say. */
export function weekStart(locale?: string): number {
  try {
    const info = new Intl.Locale(
      locale ?? navigator.language,
    ) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const firstDay = (info.getWeekInfo?.() ?? info.weekInfo)?.firstDay;
    if (firstDay) return firstDay % 7;
  } catch {
    // Unknown locale: use the default below.
  }
  return 1;
}

/** `YYYY-MM-01` of the month `months` away from the month holding `iso`. */
export function shiftMonth(iso: string, months: number): string {
  const date = parseIsoDay(iso);
  return isoDay(new Date(date.getFullYear(), date.getMonth() + months, 1));
}

/** The days a month view shows: whole weeks from the one holding the 1st to
 * the one holding the last day (5 or 6 weeks, sometimes 4). */
export function monthGrid(iso: string, firstDay: number): string[] {
  const date = parseIsoDay(iso);
  const first = new Date(date.getFullYear(), date.getMonth(), 1);
  const last = new Date(date.getFullYear(), date.getMonth() + 1, 0);
  const lead = (first.getDay() - firstDay + 7) % 7;
  const trail = (firstDay + 6 - last.getDay()) % 7;
  const days: string[] = [];
  for (let i = -lead; i < last.getDate() + trail; i += 1) {
    days.push(isoDay(new Date(first.getFullYear(), first.getMonth(), 1 + i)));
  }
  return days;
}

/** Reminders by day, each day in due order (all-day first). */
export function remindersByDay(reminders: Reminder[]): Map<string, Reminder[]> {
  const days = new Map<string, Reminder[]>();
  for (const reminder of [...reminders].sort(byDue)) {
    const day = days.get(reminder.date);
    if (day) day.push(reminder);
    else days.set(reminder.date, [reminder]);
  }
  return days;
}

/** Whether any of the reminder's links is a call to join. */
export function isMeeting(reminder: Reminder): boolean {
  return reminder.links.some((link) => linkInfo(link.url).kind === "meeting");
}

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
