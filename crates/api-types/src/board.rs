//! Card states the board derives from a card's pull requests rather than from
//! its column, shared by the remote server's move rules and the local
//! orchestrator. The web app mirrors them in
//! `packages/web-core/src/features/kanban/model/cardSignals.ts`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{IssueExtMetadata, PullRequestStatus};

/// Where a card's pull requests stand, taken together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    /// At least one is still open: the work is in review.
    Open,
    /// None is open and at least one merged: the work landed.
    Merged,
    /// Every one closed without merging: nothing landed.
    Closed,
}

/// The card's [`PrState`], `None` for a card with no pull request.
///
/// A single open request holds the card however many others merged: a ticket
/// that opened several is not done while any part of it is still in review.
pub fn pr_state(statuses: &[PullRequestStatus]) -> Option<PrState> {
    if statuses.is_empty() {
        None
    } else if statuses.contains(&PullRequestStatus::Open) {
        Some(PrState::Open)
    } else if statuses.contains(&PullRequestStatus::Merged) {
        Some(PrState::Merged)
    } else {
        Some(PrState::Closed)
    }
}

/// Why the Clean up column refuses a card, `None` when it takes it.
///
/// Clean up archives the card's worktree and branch, so it takes a card whose
/// work landed (merged), a card that never had a pull request (an
/// investigation, a story closed without code), or one whose pull request
/// closed unmerged once a person has confirmed the discard. An open pull
/// request is never cleaned up from under its reviewers.
pub fn cleanup_block_reason(
    state: Option<PrState>,
    metadata: &IssueExtMetadata,
) -> Option<&'static str> {
    match state {
        None | Some(PrState::Merged) => None,
        Some(PrState::Open) => {
            Some("the pull request is still open — merge or close it before cleaning the card up")
        }
        Some(PrState::Closed) if metadata.discard_confirmed_at.is_some() => None,
        Some(PrState::Closed) => Some(
            "the pull request closed without merging — confirm discarding the branch and \
             worktree to clean the card up",
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn pr_state_reads_every_request_together() {
        use PullRequestStatus::{Closed, Merged, Open};
        assert_eq!(pr_state(&[]), None);
        assert_eq!(pr_state(&[Open]), Some(PrState::Open));
        assert_eq!(pr_state(&[Merged, Open]), Some(PrState::Open));
        assert_eq!(pr_state(&[Merged]), Some(PrState::Merged));
        assert_eq!(pr_state(&[Merged, Closed]), Some(PrState::Merged));
        assert_eq!(pr_state(&[Closed]), Some(PrState::Closed));
        assert_eq!(pr_state(&[Closed, Closed]), Some(PrState::Closed));
    }

    #[test]
    fn clean_up_takes_merged_and_pr_less_cards() {
        let plain = IssueExtMetadata::default();
        assert_eq!(cleanup_block_reason(Some(PrState::Merged), &plain), None);
        assert_eq!(cleanup_block_reason(None, &plain), None);
    }

    #[test]
    fn clean_up_refuses_an_open_pull_request_even_when_confirmed() {
        let confirmed = IssueExtMetadata {
            discard_confirmed_at: Some(Utc::now()),
            ..Default::default()
        };
        for metadata in [IssueExtMetadata::default(), confirmed] {
            let reason = cleanup_block_reason(Some(PrState::Open), &metadata).unwrap();
            assert!(reason.contains("still open"), "{reason}");
        }
    }

    #[test]
    fn a_closed_unmerged_pull_request_needs_the_discard_confirmed() {
        let reason = cleanup_block_reason(Some(PrState::Closed), &IssueExtMetadata::default())
            .expect("unconfirmed discard is refused");
        assert!(reason.contains("confirm discarding"), "{reason}");
        let confirmed = IssueExtMetadata {
            discard_confirmed_at: Some(Utc::now()),
            ..Default::default()
        };
        assert_eq!(
            cleanup_block_reason(Some(PrState::Closed), &confirmed),
            None
        );
    }
}
