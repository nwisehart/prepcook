//! Linear as a tracker: named so a config can choose it, not yet
//! implemented. Every operation answers [`TrackerError::NotSupported`].

use std::path::Path;

use async_trait::async_trait;

use super::{
    CreatedSubtask, FetchedIssue, IssueFields, NewIssue, NewSubtask, RemoteLink, Tracker,
    TrackerAttachment, TrackerComment, TrackerError, TrackerIdentity, TrackerResult, TrackerUser,
};
use crate::services::config::TrackerKind;

pub struct LinearTracker;

const KIND: TrackerKind = TrackerKind::Linear;

#[async_trait]
impl Tracker for LinearTracker {
    fn kind(&self) -> TrackerKind {
        KIND
    }

    fn is_configured(&self) -> bool {
        false
    }

    fn issue_url(&self, key: &str) -> String {
        key.to_string()
    }

    async fn search(&self, _query: &str) -> TrackerResult<Vec<FetchedIssue>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn get_issue(&self, _key: &str) -> TrackerResult<IssueFields> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn comments(&self, _key: &str) -> TrackerResult<Vec<TrackerComment>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn post_comment(&self, _key: &str, _text: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn transition(&self, _key: &str, _status_name: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn transition_to_in_progress(&self, _key: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn assignee(&self, _key: &str) -> TrackerResult<Option<TrackerUser>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn assign_to_self(&self, _key: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn set_summary(&self, _key: &str, _summary: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn link_blocks(&self, _blocker: &str, _blocked: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn set_remote_link(&self, _key: &str, _link: RemoteLink<'_>) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn add_watcher(&self, _key: &str, _account_id: &str) -> TrackerResult<()> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn create_issue(&self, _issue: &NewIssue) -> TrackerResult<String> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn create_subtasks(
        &self,
        _parent: &str,
        _items: &[NewSubtask],
    ) -> TrackerResult<Vec<CreatedSubtask>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn download_attachments(
        &self,
        _key: &str,
        _dir: &Path,
    ) -> TrackerResult<Vec<TrackerAttachment>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn identity(&self) -> TrackerResult<TrackerIdentity> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn assignable_users(&self, _project: &str) -> TrackerResult<Vec<TrackerUser>> {
        Err(TrackerError::NotSupported(KIND))
    }
}
