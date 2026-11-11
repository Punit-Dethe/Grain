import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  commands,
  events,
  type AskTurn,
  type MeetSources,
  type MeetSummaryDoc,
  type MeetStatus,
  type MeetingSummary,
  type RecordingStatus,
  type SourceKind,
  type SourceSettings,
  type TranscriptRevision,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";

const IDLE: MeetStatus = {
  recording: null,
  processing: null,
  last_error: null,
  summarizing: null,
};

/** The level meters need a fast read; the clock only shows whole seconds.
 * Only polled while a recording is on screen. */
const LIVE_POLL_MS = 120;

function unwrap<T>(
  result: { status: "ok"; data: T } | { status: "error"; error: string },
): T {
  if (result.status !== "ok") throw new Error(result.error);
  return result.data;
}

export interface MeetController {
  status: MeetStatus;
  meetings: MeetingSummary[];
  loading: boolean;
  loadError: string | null;
  /** Recall's own audio sources; null until loaded. */
  sources: MeetSources | null;
  diarizerInstalled: boolean | null;
  diarizerProgress: number | null;
  reload: () => Promise<void>;
  refreshSources: () => Promise<void>;
  setSources: (settings: SourceSettings) => Promise<void>;
  startRecording: (title: string | null) => Promise<string>;
  stopRecording: () => Promise<MeetingSummary>;
  /** Mute or unmute one source of the recording in progress. */
  setSourceMuted: (kind: SourceKind, muted: boolean) => Promise<void>;
  process: (sessionId: string) => Promise<void>;
  cancelProcessing: () => Promise<void>;
  renameMeeting: (sessionId: string, title: string) => Promise<void>;
  deleteMeeting: (sessionId: string) => Promise<void>;
  downloadDiarizer: () => Promise<void>;
  loadTranscript: (sessionId: string) => Promise<TranscriptRevision | null>;
  loadPlayback: (sessionId: string) => Promise<string>;
  /** Mix a finished meeting's playback audio ahead of the first play. */
  warmPlayback: (sessionId: string) => Promise<void>;
  loadSummary: (sessionId: string) => Promise<MeetSummaryDoc | null>;
  generateSummary: (sessionId: string) => Promise<MeetSummaryDoc>;
  ask: (
    sessionId: string,
    question: string,
    history: AskTurn[],
  ) => Promise<string>;
}

/** Whether an AI provider and model are set up for Recall to use. Re-checked
 * when the window regains focus, since the provider is set elsewhere. */
export function useMeetAiAvailable(): boolean | null {
  const [available, setAvailable] = useState<boolean | null>(null);
  useEffect(() => {
    let active = true;
    const check = () =>
      void commands
        .meetAiAvailable()
        .then((value) => active && setAvailable(value))
        .catch(() => active && setAvailable(false));
    check();
    window.addEventListener("focus", check);
    return () => {
      active = false;
      window.removeEventListener("focus", check);
    };
  }, []);
  return available;
}

export function useMeetController(): MeetController {
  const osType = useOsType();
  const [status, setStatus] = useState<MeetStatus>(IDLE);
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [sources, setSourcesState] = useState<MeetSources | null>(null);
  const [diarizerInstalled, setDiarizerInstalled] = useState<boolean | null>(
    null,
  );
  const [diarizerProgress, setDiarizerProgress] = useState<number | null>(null);
  const mountedRef = useRef(false);
  const reloadId = useRef(0);
  const sourcesRef = useRef<MeetSources | null>(null);
  const sourceRefreshId = useRef(0);
  const savingSources = useRef(false);
  useEffect(() => {
    sourcesRef.current = sources;
  }, [sources]);

  const reload = useCallback(async () => {
    const requestId = ++reloadId.current;
    try {
      const [list, installed, current] = await Promise.all([
        commands.meetList().then(unwrap),
        commands.meetDiarizerInstalled().then(unwrap),
        commands.meetStatus(),
      ]);
      if (!mountedRef.current || requestId !== reloadId.current) return;
      setMeetings(list);
      setDiarizerInstalled(installed);
      setStatus(current);
      setLoadError(null);
    } catch (error) {
      console.error("Failed to load meetings:", error);
      if (mountedRef.current && requestId === reloadId.current) {
        setLoadError(String(error));
      }
    } finally {
      if (mountedRef.current && requestId === reloadId.current) {
        setLoading(false);
      }
    }
  }, []);

  const refreshSources = useCallback(async () => {
    if (savingSources.current) return;
    const refreshId = ++sourceRefreshId.current;
    try {
      const next = unwrap(await commands.meetSources());
      if (mountedRef.current && refreshId === sourceRefreshId.current) {
        setSourcesState(next);
      }
    } catch (error) {
      console.error("Failed to load meeting audio sources:", error);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    let previous: MeetStatus = IDLE;
    const statusListener = events.meetStatusChanged.listen((event) => {
      const next = event.payload;
      setStatus(next);
      // A recording or an attempt just ended: its meeting changed on disk.
      const ended =
        (previous.recording && !next.recording) ||
        (previous.processing && !next.processing) ||
        // An AI title or summary just landed.
        (previous.summarizing && !next.summarizing);
      previous = next;
      if (ended) void reload();
    });
    const progressListener = events.meetDiarizerProgress.listen((event) => {
      const { downloaded, total } = event.payload;
      setDiarizerProgress(total > 0 ? downloaded / total : null);
    });
    // Devices come and go while the app is in the background.
    const onFocus = () => void refreshSources();
    window.addEventListener("focus", onFocus);
    void reload();
    void refreshSources();
    return () => {
      mountedRef.current = false;
      window.removeEventListener("focus", onFocus);
      void statusListener.then((cleanup) => cleanup());
      void progressListener.then((cleanup) => cleanup());
    };
  }, [reload, refreshSources]);

  const setSources = useCallback(async (settings: SourceSettings) => {
    // A device refresh started before this write may have read the old settings.
    savingSources.current = true;
    sourceRefreshId.current += 1;
    const previous = sourcesRef.current;
    if (previous) setSourcesState({ ...previous, settings });
    try {
      unwrap(await commands.meetSetSources(settings));
    } catch (error) {
      if (mountedRef.current) setSourcesState(previous);
      throw error;
    } finally {
      savingSources.current = false;
    }
  }, []);

  const startRecording = useCallback(
    async (title: string | null) => {
      const id = unwrap(await commands.meetStartRecording(title));
      await reload();
      return id;
    },
    [reload],
  );

  const stopRecording = useCallback(async () => {
    const summary = unwrap(await commands.meetStopRecording());
    await reload();
    return summary;
  }, [reload]);

  const process = useCallback(async (sessionId: string) => {
    unwrap(await commands.meetProcess(sessionId));
  }, []);

  const cancelProcessing = useCallback(async () => {
    unwrap(await commands.meetCancelProcessing());
  }, []);

  const renameMeeting = useCallback(
    async (sessionId: string, title: string) => {
      const savedTitle = unwrap(await commands.meetRename(sessionId, title));
      if (!mountedRef.current) return;
      setMeetings((current) =>
        current.map((meeting) =>
          meeting.id === sessionId
            ? { ...meeting, title: savedTitle }
            : meeting,
        ),
      );
      await reload();
    },
    [reload],
  );

  const deleteMeeting = useCallback(async (sessionId: string) => {
    unwrap(await commands.meetDelete(sessionId));
    if (mountedRef.current) {
      setMeetings((current) => current.filter((m) => m.id !== sessionId));
    }
  }, []);

  const downloadDiarizer = useCallback(async () => {
    setDiarizerProgress(0);
    try {
      unwrap(await commands.meetDownloadDiarizer());
      if (mountedRef.current) setDiarizerInstalled(true);
    } finally {
      if (mountedRef.current) setDiarizerProgress(null);
    }
  }, []);

  const loadTranscript = useCallback(
    async (sessionId: string) =>
      unwrap(await commands.meetTranscript(sessionId)),
    [],
  );

  const loadSummary = useCallback(
    async (sessionId: string) => unwrap(await commands.meetSummary(sessionId)),
    [],
  );

  const generateSummary = useCallback(
    async (sessionId: string) =>
      unwrap(await commands.meetGenerateSummary(sessionId)),
    [],
  );

  const ask = useCallback(
    async (sessionId: string, question: string, history: AskTurn[]) =>
      unwrap(await commands.meetAsk(sessionId, question, history)),
    [],
  );

  const loadPlayback = useCallback(
    async (sessionId: string) => {
      const path = unwrap(await commands.meetPlaybackAudio(sessionId));
      if (osType === "linux") {
        const data = await readFile(path);
        return URL.createObjectURL(new Blob([data], { type: "audio/wav" }));
      }
      return convertFileSrc(path, "asset");
    },
    [osType],
  );

  const setSourceMuted = useCallback(
    async (kind: SourceKind, muted: boolean) => {
      unwrap(await commands.meetSetSourceMuted(kind, muted));
    },
    [],
  );

  const warmPlayback = useCallback(async (sessionId: string) => {
    unwrap(await commands.meetPlaybackAudio(sessionId));
  }, []);

  return {
    status,
    meetings,
    loading,
    loadError,
    sources,
    diarizerInstalled,
    diarizerProgress,
    reload,
    refreshSources,
    setSources,
    startRecording,
    stopRecording,
    process,
    cancelProcessing,
    renameMeeting,
    deleteMeeting,
    downloadDiarizer,
    loadTranscript,
    loadPlayback,
    warmPlayback,
    setSourceMuted,
    loadSummary,
    generateSummary,
    ask,
  };
}

/**
 * Refresh the recording clock and capture error only while recording and only
 * in the component that displays them.
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
