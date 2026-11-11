import type { MeetingSummary } from "@/bindings";
import type { Turn, TurnWord } from "./transcriptTurns";

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

export interface RecordingFilters {
  query: string;
  oldestFirst: boolean;
}

/** Filter meetings received newest first and group them by local calendar day. */
export function groupByDay(
  meetings: MeetingSummary[],
  now: Date,
  filters?: RecordingFilters,
): DayGroup[] {
  const today = localDay(now.getTime()).getTime();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const query = filters?.query.trim().toLowerCase();
  const groups: DayGroup[] = [];
  for (const meeting of meetings) {
    if (
      query &&
      !meeting.title.toLowerCase().includes(query) &&
      !meeting.preview?.toLowerCase().includes(query)
    ) {
      continue;
    }
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
  if (filters?.oldestFirst) {
    groups.reverse();
    for (const group of groups) group.meetings.reverse();
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

/** The word being heard at `ms`: the last word that has started (-1 before the first). */
export function wordAt(words: TurnWord[], ms: number): number {
  let low = 0;
  let high = words.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (words[mid].startMs <= ms) low = mid + 1;
    else high = mid;
  }
  return low - 1;
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

/** Each speaker's share (0..1) of the attributed speaking time. */
export function speakerShares(turns: Turn[]): Map<string, number> {
  const spoken = new Map<string, number>();
  let total = 0;
  for (const turn of turns) {
    if (!turn.speakerId) continue;
    const span = Math.max(0, turn.endMs - turn.startMs);
    spoken.set(turn.speakerId, (spoken.get(turn.speakerId) ?? 0) + span);
    total += span;
  }
  const shares = new Map<string, number>();
  for (const [id, span] of spoken) shares.set(id, total > 0 ? span / total : 0);
  return shares;
}
