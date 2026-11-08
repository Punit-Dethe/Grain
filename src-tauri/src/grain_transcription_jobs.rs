// Included in the shared queue module: keep Grain's lifetime policy out of Handy.
impl TranscriptionJobManager {
    fn grain_start_worker(self: &Arc<Self>) {
        let mut jobs = self.lock();
        if jobs.worker_active || jobs.is_idle() {
            return;
        }
        jobs.worker_active = true;
        let runner = Arc::clone(self);
        if let Err(error) = thread::Builder::new()
            .name("file-transcription".into())
            .spawn(move || runner.run_jobs())
        {
            jobs.worker_active = false;
            let mut ids: Vec<_> = jobs.retries.drain(..).collect();
            ids.extend(jobs.files.drain(..));
            self.sync_job_active(&jobs);
            drop(jobs);
            for id in ids {
                self.emit(
                    id,
                    TranscriptionJobStatus::Failed {
                        error: format!("Could not start transcription: {error}"),
                    },
                );
            }
        }
    }

    pub(crate) fn grain_snapshot(&self) -> Vec<TranscriptionJob> {
        let jobs = self.lock();
        let mut result: Vec<_> = jobs
            .retries
            .iter()
            .chain(jobs.files.iter())
            .map(|&entry_id| TranscriptionJob {
                entry_id,
                status: TranscriptionJobStatus::Queued,
            })
            .collect();
        if let Some(running) = &jobs.running {
            result.push(TranscriptionJob {
                entry_id: running.entry_id,
                status: TranscriptionJobStatus::Running { percent: 0 },
            });
        }
        result
    }
}

#[cfg(test)]
mod grain_queue_tests {
    use super::*;

    #[test]
    fn queued_and_running_entries_are_deduplicated_and_cancel_only_removes_target() {
        let mut jobs = Jobs::default();
        jobs.files.extend([1, 2]);
        jobs.retries.push_back(3);
        jobs.running = Some(Running {
            entry_id: 4,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        for id in 1..=4 {
            assert!(jobs.has(id));
        }
        assert!(!jobs.has(5));
        assert!(jobs.unqueue(2));
        assert!(!jobs.unqueue(4));
        assert_eq!(jobs.files, [1]);
        assert_eq!(jobs.retries, [3]);
        assert!(!jobs.is_idle());
        jobs.running = None;
        jobs.unqueue(1);
        jobs.unqueue(3);
        assert!(jobs.is_idle());
    }
}
