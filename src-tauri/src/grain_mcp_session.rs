//! Per-provider gates, with no retained SDK client or background task.
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::{watch, OwnedMutexGuard};

pub(super) type Lease = Arc<OwnedMutexGuard<()>>;

pub(super) struct Control {
    generation: Mutex<u64>,
    changed: watch::Sender<u64>,
    operation: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Clone)]
pub(super) struct Ticket {
    control: Arc<Control>,
    generation: u64,
}

pub(super) const CANCELLED: &str = "MCP account or access changed. Ask again.";

impl Control {
    pub(super) fn new() -> Arc<Self> {
        let (changed, _) = watch::channel(1);
        Arc::new(Self {
            generation: Mutex::new(1),
            changed,
            operation: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub(super) fn ticket(self: &Arc<Self>) -> Ticket {
        Ticket {
            control: self.clone(),
            generation: *self.generation.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }

    pub(super) fn invalidate(self: &Arc<Self>) -> Ticket {
        let mut generation = self.generation.lock().unwrap_or_else(|e| e.into_inner());
        *generation = generation.checked_add(1).expect("MCP generation exhausted");
        self.changed.send_replace(*generation);
        Ticket {
            control: self.clone(),
            generation: *generation,
        }
    }
}

impl Ticket {
    pub(super) fn invalidate_if_current(&self) {
        let mut generation = self
            .control
            .generation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *generation == self.generation {
            *generation = generation.checked_add(1).expect("MCP generation exhausted");
            self.control.changed.send_replace(*generation);
        }
    }

    /// Check and synchronous commit share a lock with invalidation. In particular,
    /// a spawn_blocking vault write cannot outlive logout and resurrect a grant.
    pub(super) fn commit<T>(&self, write: impl FnOnce() -> T) -> Result<T, String> {
        let generation = self
            .control
            .generation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *generation != self.generation {
            return Err(CANCELLED.into());
        }
        Ok(write())
    }

    pub(super) async fn run<T>(&self, future: impl Future<Output = T>) -> Result<T, String> {
        let mut changed = self.control.changed.subscribe();
        let cancelled = async {
            loop {
                if *changed.borrow_and_update() != self.generation {
                    break;
                }
                if changed.changed().await.is_err() {
                    break;
                }
            }
        };
        tokio::select! {
            biased;
            _ = cancelled => Err(CANCELLED.into()),
            result = future => Ok(result),
        }
    }

    pub(super) async fn acquire(&self) -> Result<Lease, String> {
        self.run(self.control.operation.clone().lock_owned())
            .await
            .map(Arc::new)
    }

    pub(super) fn bind_digest(&self, digest: &str) -> String {
        format!("{}:{digest}", self.generation)
    }

    pub(super) fn unbind_digest<'a>(&self, digest: &'a str) -> Result<&'a str, String> {
        let (generation, digest) = digest.split_once(':').ok_or(CANCELLED)?;
        if generation != self.generation.to_string() {
            return Err(CANCELLED.into());
        }
        self.commit(|| digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queued_old_account_cannot_acquire_new_account_gate() {
        let control = Control::new();
        let old = control.ticket();
        let guard = old.acquire().await.unwrap();
        let next = control.invalidate();
        assert!(old.acquire().await.is_err());
        drop(guard);
        assert!(next.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn cancellation_drops_operation_and_other_providers_remain_independent() {
        let first = Control::new();
        let second = Control::new();
        let ticket = first.ticket();
        let running = tokio::spawn(async move {
            let _guard = ticket.acquire().await.unwrap();
            ticket.run(std::future::pending::<()>()).await
        });
        tokio::task::yield_now().await;
        first.invalidate();
        assert!(running.await.unwrap().is_err());
        assert!(first.ticket().acquire().await.is_ok());
        assert!(second.ticket().commit(|| true).unwrap());
    }

    #[test]
    fn stale_vault_commit_and_approval_are_rejected() {
        let control = Control::new();
        let old = control.ticket();
        let approval = old.bind_digest("unchanged-tools");
        let new = control.invalidate();
        let mut writes = 0;
        assert!(old.commit(|| writes += 1).is_err());
        assert_eq!(writes, 0);
        assert!(new.unbind_digest(&approval).is_err());
        assert_eq!(
            new.unbind_digest(&new.bind_digest("unchanged-tools"))
                .unwrap(),
            "unchanged-tools"
        );
    }

    #[test]
    fn late_failure_cleanup_does_not_cancel_a_replacement() {
        let control = Control::new();
        let old = control.ticket();
        let replacement = control.invalidate();
        old.invalidate_if_current();
        assert!(replacement.commit(|| ()).is_ok());
        replacement.invalidate_if_current();
        assert!(replacement.commit(|| ()).is_err());
    }

    #[test]
    fn blocking_vault_commit_is_ordered_before_logout_then_late_writes_fail() {
        let control = Control::new();
        let old = control.ticket();
        let account = Arc::new(Mutex::new(None));
        let (entered, observing) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let writer = old.clone();
        let target = account.clone();
        let saving = std::thread::spawn(move || {
            writer
                .commit(|| {
                    entered.send(()).unwrap();
                    released.recv().unwrap();
                    *target.lock().unwrap() = Some("old-token");
                })
                .unwrap();
        });
        observing.recv().unwrap();
        let logout_control = control.clone();
        let target = account.clone();
        let logout = std::thread::spawn(move || {
            let next = logout_control.invalidate();
            next.commit(|| *target.lock().unwrap() = None).unwrap();
        });
        release.send(()).unwrap();
        saving.join().unwrap();
        logout.join().unwrap();
        assert!(old
            .commit(|| *account.lock().unwrap() = Some("late-token"))
            .is_err());
        assert_eq!(*account.lock().unwrap(), None);
    }

    #[tokio::test]
    async fn detached_blocking_task_keeps_refresh_gate_until_it_finishes() {
        let control = Control::new();
        let owner = control.ticket().acquire().await.unwrap();
        let task_owner = owner.clone();
        let (entered, observing) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let task = tokio::task::spawn_blocking(move || {
            let _owner = task_owner;
            entered.send(()).unwrap();
            released.recv().unwrap();
        });
        observing.await.unwrap();
        drop(owner);
        assert!(control.operation.try_lock().is_err());
        release.send(()).unwrap();
        task.await.unwrap();
        assert!(control.ticket().acquire().await.is_ok());
    }
}
