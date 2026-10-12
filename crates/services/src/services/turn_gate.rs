//! One live coding-agent process per workspace.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

/// Serialises turn starts per workspace.
///
/// Two requests to start a turn in one workspace — the composer and the
/// orchestrator, or one comment delivered twice — used to race the "is
/// anything running?" check and both spawn: two Claude Code processes in one
/// home, colliding on the credential refresh (`Failed to refresh OAuth token:
/// another Claude Code process is refreshing it`, BUG-10333, 12 Sep). The gate
/// makes check-and-spawn one step: the caller holds the workspace's guard from
/// the running-process query until the process row exists, and a second caller
/// waits its turn and then sees the first.
#[derive(Clone, Default)]
pub struct TurnGate {
    locks: Arc<std::sync::Mutex<HashMap<Uuid, Arc<Mutex<()>>>>>,
}

impl TurnGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Waits for the workspace's guard. Hold it across the busy check and the
    /// spawn; drop it as soon as the process row exists.
    pub async fn acquire(&self, workspace_id: Uuid) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self
                .locks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locks.entry(workspace_id).or_default().clone()
        };
        lock.lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn a_second_caller_waits_for_the_first_to_let_go() {
        let gate = TurnGate::new();
        let workspace = Uuid::new_v4();
        let first = gate.acquire(workspace).await;

        let second = tokio::time::timeout(Duration::from_millis(50), gate.acquire(workspace));
        assert!(second.await.is_err(), "the guard is held");

        drop(first);
        let second = tokio::time::timeout(Duration::from_millis(50), gate.acquire(workspace));
        assert!(second.await.is_ok(), "released guards are taken");
    }

    #[tokio::test]
    async fn workspaces_do_not_wait_on_each_other() {
        let gate = TurnGate::new();
        let _held = gate.acquire(Uuid::new_v4()).await;
        let other = tokio::time::timeout(Duration::from_millis(50), gate.acquire(Uuid::new_v4()));
        assert!(other.await.is_ok());
    }
}
