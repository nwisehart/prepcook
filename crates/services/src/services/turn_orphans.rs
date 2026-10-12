//! Children a turn left running when it ended.
//!
//! Claude Code delivers a background task's completion only inside a live
//! turn. A turn that backgrounds a harness, says "I'll report when it
//! completes" and ends has therefore lost the result: nothing will read it,
//! and the card goes quiet until a person notices (BUG-10333: two hours and
//! fifty minutes). The process layer knows the moment this happens — the
//! agent's process group still has members after the agent exited — and
//! records it here; the orchestrator's next tick says so on the card and,
//! under Full autonomy, starts the turn that reads what the process left.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use uuid::Uuid;

/// A turn that ended with processes still alive in its process group.
#[derive(Debug, Clone)]
pub struct OrphanReport {
    pub execution_process_id: Uuid,
    pub ended_at: DateTime<Utc>,
    /// `<pid> <command>` for every survivor, command clipped.
    pub survivors: Vec<String>,
}

/// Reports by workspace, each waiting for the orchestrator to take it. One per
/// workspace: a newer turn's report replaces an older one nobody read, since
/// the newer turn is the one whose result is now at risk. In memory on
/// purpose — a report is about a turn that just ended, and a restart that
/// loses it loses a notice, not work.
#[derive(Clone, Default)]
pub struct TurnOrphans {
    reports: Arc<DashMap<Uuid, OrphanReport>>,
}

impl TurnOrphans {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `execution_process_id`'s turn just ended with these
    /// survivors. Stamped here so the process layer needs no clock of its own.
    pub fn record(&self, workspace_id: Uuid, execution_process_id: Uuid, survivors: Vec<String>) {
        self.reports.insert(
            workspace_id,
            OrphanReport {
                execution_process_id,
                ended_at: Utc::now(),
                survivors,
            },
        );
    }

    pub fn take(&self, workspace_id: Uuid) -> Option<OrphanReport> {
        self.reports.remove(&workspace_id).map(|(_, report)| report)
    }

    pub fn is_empty(&self) -> bool {
        self.reports.is_empty()
    }
}

/// Processes still alive in `pgid`'s process group, as `<pid> <command>`.
/// `ps -A -o pid=,pgid=,args=` reads the same on Linux and macOS; an
/// unavailable `ps` reports nothing rather than failing the turn's exit path.
pub async fn survivors_of_group(pgid: u32) -> Vec<String> {
    let output = tokio::process::Command::new("ps")
        .args(["-A", "-o", "pid=,pgid=,args="])
        .output()
        .await;
    match output {
        Ok(output) => parse_survivors(&String::from_utf8_lossy(&output.stdout), pgid),
        Err(error) => {
            tracing::debug!("turn orphans: ps failed, assuming none: {error}");
            Vec::new()
        }
    }
}

fn parse_survivors(ps: &str, pgid: u32) -> Vec<String> {
    const COMMAND_CHARS: usize = 160;
    ps.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?;
            let group: u32 = parts.next()?.parse().ok()?;
            if group != pgid || pid == pgid.to_string() {
                return None; // another group, or the leader itself (already reaped)
            }
            let command = parts.collect::<Vec<_>>().join(" ");
            if command.contains("<defunct>") {
                return None; // exited, awaiting its parent's wait — not running
            }
            let command: String = command.chars().take(COMMAND_CHARS).collect();
            Some(format!("{pid} {command}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "\
  4101  4101 /usr/bin/claude -p --output-format stream-json
  4188  4101 /bin/bash -c timeout 600 node .repro/BUG-1/repro.mjs
  4190  4101 node .repro/BUG-1/repro.mjs
  4201  4101 [node] <defunct>
  5000  5000 sshd: ec2-user
  5017  5017 ps -A -o pid=,pgid=,args=
";

    #[test]
    fn only_the_groups_live_members_are_listed() {
        let survivors = parse_survivors(PS, 4101);
        assert_eq!(
            survivors,
            vec![
                "4188 /bin/bash -c timeout 600 node .repro/BUG-1/repro.mjs",
                "4190 node .repro/BUG-1/repro.mjs",
            ]
        );
        assert!(parse_survivors(PS, 9999).is_empty());
    }

    #[test]
    fn a_report_is_taken_once_and_a_newer_one_replaces_an_unread_one() {
        let orphans = TurnOrphans::new();
        let workspace = Uuid::new_v4();
        assert!(orphans.is_empty());
        orphans.record(workspace, Uuid::new_v4(), vec!["1 sleep 300".to_string()]);
        orphans.record(workspace, Uuid::new_v4(), vec!["2 sleep 300".to_string()]);
        assert_eq!(
            orphans.take(workspace).map(|r| r.survivors),
            Some(vec!["2 sleep 300".to_string()])
        );
        assert!(orphans.take(workspace).is_none());
        assert!(orphans.is_empty());
    }
}
