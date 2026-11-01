import type {
  Attribution,
  Speaker,
  TranscriptRevision,
  Word,
} from "@/bindings";

/** A readable run of consecutive words from one track and one attribution. */
export interface Turn {
  key: string;
  track: number;
  /** The speaker id (`t{track}-s{slot}`), or null when not attributed. */
  speakerId: string | null;
  /** True when several speakers overlap the turn's words. */
  ambiguous: boolean;
  startMs: number;
  endMs: number;
  text: string;
}

/** A pause this long starts a new turn even with the same speaker. */
const TURN_PAUSE_MS = 1_500;

function speakerKey(
  track: number,
  attribution: Attribution,
): {
  id: string | null;
  ambiguous: boolean;
} {
  if (typeof attribution === "object" && "Speaker" in attribution) {
    return { id: `t${track}-s${attribution.Speaker}`, ambiguous: false };
  }
  if (typeof attribution === "object" && "Ambiguous" in attribution) {
    return { id: null, ambiguous: true };
  }
  return { id: null, ambiguous: false };
}

/**
 * Group attributed words into turns, ordered on the meeting clock. Words from
 * different tracks interleave by time; overlapping speech stays as separate
 * turns rather than being merged into one fictional sentence.
 */
export function buildTurns(revision: TranscriptRevision): Turn[] {
  const words: { track: number; word: Word }[] = revision.tracks.flatMap(
    (track) => track.words.map((word) => ({ track: track.track, word })),
  );
  words.sort((a, b) => a.word.start_ms - b.word.start_ms || a.track - b.track);
  const turns: Turn[] = [];
  const open = new Map<number, Turn>();
  for (const { track, word } of words) {
    const speaker = speakerKey(track, word.speaker);
    const current = open.get(track);
    const continues =
      current &&
      current.speakerId === speaker.id &&
      current.ambiguous === speaker.ambiguous &&
      word.start_ms - current.endMs < TURN_PAUSE_MS;
    if (current && continues) {
      current.text = `${current.text} ${word.text}`;
      current.endMs = Math.max(current.endMs, word.end_ms);
      continue;
    }
    const turn: Turn = {
      key: `${track}-${word.start_ms}-${turns.length}`,
      track,
      speakerId: speaker.id,
      ambiguous: speaker.ambiguous,
      startMs: word.start_ms,
      endMs: word.end_ms,
      text: word.text,
    };
    turns.push(turn);
    open.set(track, turn);
  }
  return turns;
}

/** `m:ss` under an hour, `h:mm:ss` from one hour. */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1_000));
  const hours = Math.floor(total / 3_600);
  const minutes = Math.floor((total % 3_600) / 60);
  const seconds = total % 60;
  const ss = String(seconds).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${ss}`;
  }
  return `${minutes}:${ss}`;
}

export function speakerLabels(speakers: Speaker[]): Map<string, string> {
  return new Map(speakers.map((speaker) => [speaker.id, speaker.label]));
}

/** Plain-text export: one `[time] Speaker: text` line per turn. */
export function transcriptText(
  turns: Turn[],
  labels: Map<string, string>,
  unknownSpeaker: string,
): string {
  return turns
    .map((turn) => {
      const who = turn.speakerId
        ? (labels.get(turn.speakerId) ?? unknownSpeaker)
        : unknownSpeaker;
      return `[${formatClock(turn.startMs)}] ${who}: ${turn.text}`;
    })
    .join("\n");
}

export function filterTurns(turns: Turn[], query: string): Turn[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return turns;
  return turns.filter((turn) => turn.text.toLowerCase().includes(needle));
}
