import { create } from "zustand";
import { commands, events, type TranscriptionJob } from "@/bindings";

// Grain's file-job snapshot command supplements Handy's event-only lock. The
// catalogue owns this subscription; closing it releases the listener and data.
const useJobs = create<{
  jobs: Record<number, TranscriptionJob>;
  ready: boolean;
}>(() => ({
  jobs: {},
  ready: false,
}));
const locked = (state: ReturnType<typeof useJobs.getState>) =>
  !state.ready ||
  Object.values(state.jobs).some(
    ({ status }) => status.state === "queued" || status.state === "running",
  );

export const useModelsLocked = () => useJobs(locked);
export const areModelsLocked = () => locked(useJobs.getState());

export function watchModelJobs() {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  let initialized = false;
  const buffered: TranscriptionJob[] = [];
  useJobs.setState({ jobs: {}, ready: false });
  const apply = (job: TranscriptionJob) => {
    useJobs.setState((state) => {
      const jobs = { ...state.jobs };
      if (job.status.state === "queued" || job.status.state === "running")
        jobs[job.entry_id] = job;
      else delete jobs[job.entry_id];
      return { jobs };
    });
  };
  void (async () => {
    const stop = await events.transcriptionJob.listen(({ payload }) => {
      if (disposed) return;
      if (initialized) apply(payload);
      else buffered.push(payload);
    });
    if (disposed) {
      stop();
      return;
    }
    unlisten = stop;
    const snapshot = await commands.getTranscriptionJobs();
    if (disposed) return;
    snapshot.forEach(apply);
    buffered.forEach(apply);
    buffered.length = 0;
    initialized = true;
    useJobs.setState({ ready: true });
  })().catch((error) => {
    if (!disposed) {
      // Backend admission remains authoritative if status cannot be queried.
      buffered.forEach(apply);
      buffered.length = 0;
      initialized = true;
      useJobs.setState({ ready: true });
      console.error("Could not read file transcription status:", error);
    }
  });
  return () => {
    disposed = true;
    unlisten?.();
    buffered.length = 0;
    useJobs.setState({ jobs: {}, ready: false });
  };
}
