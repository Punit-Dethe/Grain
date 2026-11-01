import type { MeetingSummary } from "@/bindings";
import type { Turn } from "./transcriptTurns";

/** Quietest level the meter shows, in dBFS. */
const METER_FLOOR_DB = -60;

/** A peak sample level (0..1) as a meter fraction on a decibel scale. */
export function levelFraction(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(Math.min(1, peak));
  return Math.max(0, Math.min(1, 1 - db / METER_FLOOR_DB));
}

export type DayKind = "today" | "yesterday" | "date";

export interface DayGroup {
  key: string;
  kind: DayKind;
  /** Local midnight of the day. */
  day: Date;
  meetings: MeetingSummary[];
}

function localDay(ms: number): Date {
  const date = new Date(ms);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

/** Meetings (already newest first) grouped by local calendar day. */
export function groupByDay(meetings: MeetingSummary[], now: Date): DayGroup[] {
  const today = localDay(now.getTime()).getTime();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const groups: DayGroup[] = [];
  for (const meeting of meetings) {
    const day = localDay(meeting.created_unix_ms);
    const key = String(day.getTime());
    let group = groups[groups.length - 1];
    if (!group || group.key !== key) {
      const kind: DayKind =
        day.getTime() === today
          ? "today"
          : day.getTime() === yesterday.getTime()
            ? "yesterday"
            : "date";
      group = { key, kind, day, meetings: [] };
      groups.push(group);
    }
    group.meetings.push(meeting);
  }
  return groups;
}

/** Split text around case-insensitive matches of `needle`. */
export function highlightParts(
  text: string,
  needle: string,
): { text: string; match: boolean }[] {
  const query = needle.trim().toLowerCase();
  if (!query) return [{ text, match: false }];
  const lower = text.toLowerCase();
  const parts: { text: string; match: boolean }[] = [];
  let from = 0;
  for (;;) {
    const at = lower.indexOf(query, from);
    if (at < 0) break;
    if (at > from) parts.push({ text: text.slice(from, at), match: false });
    parts.push({ text: text.slice(at, at + query.length), match: true });
    from = at + query.length;
  }
  if (from < text.length) parts.push({ text: text.slice(from), match: false });
  return parts;
}

/** The turn being heard at `ms`: the latest one that has started and not ended. */
export function turnAt(turns: Turn[], ms: number): string | null {
  let found: Turn | null = null;
  for (const turn of turns) {
    if (turn.startMs > ms) break;
    if (ms <= turn.endMs) found = turn;
  }
  return found?.key ?? null;
}
