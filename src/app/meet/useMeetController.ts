import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  commands,
  events,
  type MeetStatus,
  type MeetingSummary,
  type TranscriptRevision,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";

const IDLE: MeetStatus = {
  recording: null,
  processing: null,
  last_error: null,
};

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
  diarizerInstalled: boolean | null;
  diarizerProgress: number | null;
  reload: () => Promise<void>;
  startRecording: (title: string | null) => Promise<string>;
  stopRecording: () => Promise<MeetingSummary>;
  process: (sessionId: string) => Promise<void>;
  cancelProcessing: () => Promise<void>;
  deleteMeeting: (sessionId: string) => Promise<void>;
  downloadDiarizer: () => Promise<void>;
  loadTranscript: (sessionId: string) => Promise<TranscriptRevision | null>;
  loadPlayback: (sessionId: string, track: number) => Promise<string>;
}

export function useMeetController(): MeetController {
  const osType = useOsType();
  const [status, setStatus] = useState<MeetStatus>(IDLE);
  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [diarizerInstalled, setDiarizerInstalled] = useState<boolean | null>(
    null,
  );
  const [diarizerProgress, setDiarizerProgress] = useState<number | null>(null);
  const mountedRef = useRef(false);

  const reload = useCallback(async () => {
    try {
      const [list, installed, current] = await Promise.all([
        commands.meetList().then(unwrap),
        commands.meetDiarizerInstalled().then(unwrap),
        commands.meetStatus(),
      ]);
      if (!mountedRef.current) return;
      setMeetings(list);
      setDiarizerInstalled(installed);
      setStatus(current);
      setLoadError(null);
    } catch (error) {
      console.error("Failed to load meetings:", error);
      if (mountedRef.current) setLoadError(String(error));
    } finally {
      if (mountedRef.current) setLoading(false);
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
        (previous.processing && !next.processing);
      previous = next;
      if (ended) void reload();
    });
    const progressListener = events.meetDiarizerProgress.listen((event) => {
      const { downloaded, total } = event.payload;
      setDiarizerProgress(total > 0 ? downloaded / total : null);
    });
    void reload();
    return () => {
      mountedRef.current = false;
      void statusListener.then((cleanup) => cleanup());
      void progressListener.then((cleanup) => cleanup());
    };
  }, [reload]);

  // Elapsed time is computed by the backend; poll only while recording.
  useEffect(() => {
    if (!status.recording) return;
    const timer = window.setInterval(() => {
      void commands.meetStatus().then((next) => {
        if (mountedRef.current) setStatus(next);
      });
    }, 1_000);
    return () => window.clearInterval(timer);
  }, [status.recording]);

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

  const loadPlayback = useCallback(
    async (sessionId: string, track: number) => {
      const path = unwrap(await commands.meetPlaybackAudio(sessionId, track));
      if (osType === "linux") {
        const data = await readFile(path);
        return URL.createObjectURL(new Blob([data], { type: "audio/wav" }));
      }
      return convertFileSrc(path, "asset");
    },
    [osType],
  );

  return {
    status,
    meetings,
    loading,
    loadError,
    diarizerInstalled,
    diarizerProgress,
    reload,
    startRecording,
    stopRecording,
    process,
    cancelProcessing,
    deleteMeeting,
    downloadDiarizer,
    loadTranscript,
    loadPlayback,
  };
}
