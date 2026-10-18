//! Linear as a tracker: named so a config can choose it, not implemented.
//! It is never configured, and every operation answers
//! [`TrackerError::NotSupported`], so a config that names it gets a clear
//! refusal rather than a silent fallback to another tracker.

use std::path::Path;

use async_trait::async_trait;

use super::{
    CreatedSubtask, FetchedIssue, IssueFields, NewIssue, NewSubtask, Query, RemoteLink,
    TicketStage, Tracker, TrackerAttachment, TrackerComment, TrackerError, TrackerIdentity,
    TrackerResult, TrackerUser,
};
use crate::services::{
    config::{TrackerConfig, TrackerKind},
    integrations::{Descriptor, Field, FieldKind},
};

pub struct LinearTracker;

const KIND: TrackerKind = TrackerKind::Linear;

impl LinearTracker {
    pub fn new(_config: TrackerConfig) -> Self {
        Self
    }

    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            KIND.as_str(),
            "Linear",
            "Linear is not implemented — contributions welcome. It can be named, and every \
             call to it says it is not supported.",
            vec![
                Field::new(
                    "linear_api_key",
                    "API key",
                    FieldKind::Secret,
                    true,
                    "A Linear personal API key.",
                ),
                Field::new(
                    "team_key",
                    "Team key",
                    FieldKind::Text,
                    true,
                    "The team issues come from, e.g. ENG.",
                ),
            ],
            "https://linear.app/developers",
        )
        .unavailable()
    }
}

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

    fn is_valid_key(&self, _key: &str) -> bool {
        false
    }

    async fn test(&self) -> TrackerResult<String> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn search(&self, _query: &str) -> TrackerResult<Vec<FetchedIssue>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn find(&self, _query: Query<'_>) -> TrackerResult<Vec<FetchedIssue>> {
        Err(TrackerError::NotSupported(KIND))
    }

    async fn fetch(&self, _keys: &[String]) -> TrackerResult<Vec<FetchedIssue>> {
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

    async fn enter_stage(&self, _key: &str, _stage: TicketStage) -> TrackerResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn not_supported<T: std::fmt::Debug>(result: TrackerResult<T>, what: &str) {
        assert!(
            matches!(result, Err(TrackerError::NotSupported(TrackerKind::Linear))),
            "{what} must refuse: {result:?}"
        );
    }

    /// Linear is a stub. If any of these starts answering, it is being
    /// shipped, and that needs a real implementation and fixtures first.
    #[tokio::test]
    async fn every_operation_is_refused() {
        let configured = TrackerConfig {
            kind: TrackerKind::Linear,
            linear_api_key: Some("lin_api_key".to_string()),
            team_key: Some("ENG".to_string()),
            ..Default::default()
        };
        let linear = LinearTracker::new(configured);
        assert!(!linear.is_configured());
        assert!(!linear.is_valid_key("ENG-1"));
        let key = "ENG-1";
        let dir = std::env::temp_dir();
        not_supported(linear.test().await, "test");
        not_supported(linear.search("anything").await, "search");
        not_supported(
            linear
                .find(Query::Active {
                    project: "ENG",
                    within: chrono::Duration::days(1),
                })
                .await,
            "find",
        );
        not_supported(linear.fetch(&[key.to_string()]).await, "fetch");
        not_supported(linear.get_issue(key).await, "get_issue");
        not_supported(linear.comments(key).await, "comments");
        not_supported(linear.post_comment(key, "hi").await, "post_comment");
        not_supported(linear.transition(key, "Done").await, "transition");
        not_supported(
            linear.transition_to_in_progress(key).await,
            "transition_to_in_progress",
        );
        not_supported(
            linear.enter_stage(key, TicketStage::Planning).await,
            "enter_stage",
        );
        not_supported(linear.assignee(key).await, "assignee");
        not_supported(linear.assign_to_self(key).await, "assign_to_self");
        not_supported(linear.set_summary(key, "x").await, "set_summary");
        not_supported(linear.link_blocks(key, "ENG-2").await, "link_blocks");
        not_supported(
            linear
                .set_remote_link(
                    key,
                    RemoteLink {
                        id: "x",
                        title: "x",
                        url: "https://example.com",
                    },
                )
                .await,
            "set_remote_link",
        );
        not_supported(linear.add_watcher(key, "u").await, "add_watcher");
        not_supported(
            linear
                .create_issue(&NewIssue {
                    project_key: None,
                    issue_type: "Bug".to_string(),
                    summary: "x".to_string(),
                    description: String::new(),
                    priority: "P2".to_string(),
                    labels: Vec::new(),
                })
                .await,
            "create_issue",
        );
        not_supported(linear.create_subtasks(key, &[]).await, "create_subtasks");
        not_supported(
            linear.download_attachments(key, &dir).await,
            "download_attachments",
        );
        not_supported(linear.identity().await, "identity");
        not_supported(linear.assignable_users("ENG").await, "assignable_users");
    }
}
