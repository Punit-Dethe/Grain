import { useEffect, useState } from "react";
import { commands, type RecordingStatus } from "@/bindings";

/** How often a live view refreshes the recording's clock and levels. */
const LIVE_POLL_MS = 120;

/**
 * Refresh the recording clock, levels and capture error only while recording
 * and only in the component that displays them. Reading the status changes
 * nothing in the backend, so several views can poll at once.
 */
export function useLiveRecording(
  initial: RecordingStatus | null,
): RecordingStatus | null {
  const [live, setLive] = useState(initial);
  const sessionId = initial?.session_id ?? null;
  useEffect(() => {
    if (!sessionId) {
      setLive(null);
      return;
    }
    let active = true;
    let timer = 0;
    const tick = async () => {
      try {
        const next = await commands.meetStatus();
        if (active) setLive(next.recording);
      } finally {
        if (active) timer = window.setTimeout(tick, LIVE_POLL_MS);
      }
    };
    void tick();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [sessionId]);
  return live;
}

/** Peak amplitude to a 0..1 meter fill: -50 dB is empty, -6 dB is full. */
export function meterFill(peak: number): number {
  const db = 20 * Math.log10(Math.max(peak, 1e-5));
  return Math.min(1, Math.max(0, (db + 50) / 44));
}

/** A source's meter fill, rising at once and falling away smoothly. Each
 * new reading (a new `source` object) steps it, even at an unchanged level. */
export function useMeterFill(source: {
  level: number;
  muted: boolean;
}): number {
  const [shown, setShown] = useState(0);
  useEffect(() => {
    setShown((previous) =>
      source.muted ? 0 : Math.max(meterFill(source.level), previous * 0.7),
    );
  }, [source]);
  return shown;
}
