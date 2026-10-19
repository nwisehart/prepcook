//! Plan decisions: approve, request changes, re-plan.
//!
//! A plan (spec or RCA) is decided on from four surfaces — the Lavish review
//! gate, a comment on the tracker ticket, a comment on the card, and
//! `POST /api/approvals/{issue_id}`. Each surface checks its author its own
//! way, builds a [`DecisionRequest`] and hands it to [`apply_decision`], so a
//! decision has the same effect wherever it was made. See `docs/approvals.md`.
//!
//! This is not the tool-approval service in the parent module, which answers
//! an agent's request to run a tool.

use std::sync::Arc;

pub use api_types::{
    ApprovalRecord, Decision, DecisionSource, parse_card_decision, parse_tracker_decision,
    tracker_label,
};
use api_types::{IssueExtMetadata, SpecRef};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

/// One decision about one card.
#[derive(Debug, Clone)]
pub struct DecisionRequest {
    pub issue_id: Uuid,
    pub decision: Decision,
    /// Reasons, or a note on an approval. May be empty.
    pub comment: String,
    /// Who decided, as the surface names them.
    pub by: String,
    pub source: DecisionSource,
}

/// Where a card sits, as far as decisions are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStage {
    Planning,
    Executing,
    Reviewing,
    /// The Shipped column (Push MR or Done on an older board).
    Shipped,
    /// Any other column: To Do, Clean up, an older board's Close, or one
    /// Prepcook does not drive.
    Other,
}

impl CardStage {
    fn label(self) -> &'static str {
        match self {
            Self::Planning => "Plan",
            Self::Executing => "Execute",
            Self::Reviewing => "Review",
            Self::Shipped => "Shipped",
            Self::Other => "a column Prepcook does not drive",
        }
    }
}

/// Whether a decision means anything for a card in this stage. Approval and
/// changes belong to a plan under review; a re-plan is for a plan that has
/// already been acted on, so it works from every stage the work moves
/// through.
pub fn takes_decision(decision: Decision, stage: CardStage) -> bool {
    match decision {
        Decision::Approve | Decision::RequestChanges => stage == CardStage::Planning,
        Decision::Replan => stage != CardStage::Other,
    }
}

/// The card a decision is about, as the host reads it.
#[derive(Debug, Clone)]
pub struct DecisionCard {
    pub issue_id: Uuid,
    pub project_id: Uuid,
    /// Short reference for logs and messages (the tracker key when there is
    /// one).
    pub reference: String,
    pub stage: CardStage,
    /// The raw extension metadata, so keys other tools own survive a write.
    pub metadata: Value,
}

/// What a decision did.
#[derive(Debug, Clone)]
pub struct DecisionOutcome {
    pub record: ApprovalRecord,
    /// The card moved (to Executing on approve, to Plan on re-plan).
    pub moved: bool,
    /// The agent was sent a follow-up (request changes).
    pub delivered: bool,
    /// One sentence for the person who decided, in their terms; the tracker
    /// adapter posts it on the ticket.
    pub reply: String,
}

#[derive(Debug, Error)]
pub enum DecisionError {
    #[error("no card {0}")]
    UnknownCard(Uuid),
    #[error("{} does not apply to a card in {}", .decision.label(), .stage.label())]
    WrongStage {
        decision: Decision,
        stage: CardStage,
    },
    /// The card's class has no such decision: an investigation is never
    /// approved, because its report finishes it.
    #[error("{0}")]
    NotForClass(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Failed(String),
}

/// What [`apply_decision`] needs from the system it runs in. The
/// orchestrator implements it; tests use an in-memory card.
#[async_trait]
pub trait DecisionHost: Send + Sync {
    /// `Ok(None)` for a card that does not exist.
    async fn load_card(&self, issue_id: Uuid) -> Result<Option<DecisionCard>, DecisionError>;

    /// Replaces the card's extension metadata, through the path that runs
    /// approval side effects (spec snapshot, RCA filing).
    async fn write_metadata(
        &self,
        card: &DecisionCard,
        metadata: Value,
    ) -> Result<(), DecisionError>;

    /// Best-effort audit comment on the card.
    async fn post_card_comment(&self, card: &DecisionCard, text: &str);

    /// Sends a follow-up to the card's live implementation session. `Ok(false)`
    /// when there is no live session to send it to.
    async fn send_to_agent(&self, card: &DecisionCard, prompt: &str)
    -> Result<bool, DecisionError>;

    async fn move_card(&self, card: &DecisionCard, to: CardStage) -> Result<(), DecisionError>;

    /// Clears what a re-plan settles (the `needs-replan` tag).
    async fn after_replan(&self, card: &DecisionCard);

    /// The card a local workspace was launched for.
    async fn issue_for_workspace(&self, local_workspace_id: Uuid) -> Option<Uuid>;
}

/// Applies a decision: records it on the card, then does what it means.
///
/// - **Approve** sets the spec approval and advances the card to Executing,
///   in either autonomy mode: a person deciding is explicit intent. An
///   approval from Lavish does not move the card here — the relay hands it
///   to the planning agent, which records the spec approval, and the
///   orchestrator's gate moves the card from there.
/// - **Request changes** clears any approval and sends the reasons to the
///   planning agent.
/// - **Re-plan** clears the card's claim and moves it back to Plan in either
///   mode; the planning agent starts again on the next tick. Reasons given on
///   the card or through the API are kept for that agent's prompt (a tracker
///   comment's reasons reach it through the tracker comment forwarder).
pub async fn apply_decision(
    host: &dyn DecisionHost,
    request: DecisionRequest,
) -> Result<DecisionOutcome, DecisionError> {
    let card = host
        .load_card(request.issue_id)
        .await?
        .ok_or(DecisionError::UnknownCard(request.issue_id))?;
    if !takes_decision(request.decision, card.stage) {
        return Err(DecisionError::WrongStage {
            decision: request.decision,
            stage: card.stage,
        });
    }
    // An investigation's report moves it on by itself; approving one would
    // move it to Executing as though it were a bug. A re-plan still applies.
    if request.decision != Decision::Replan
        && IssueExtMetadata::from_value(&card.metadata).work_class
            == Some(api_types::WorkClass::Investigation)
    {
        return Err(DecisionError::NotForClass(format!(
            "an investigation card is not approved: its triage report finishes it. To fix \
             what it found, convert it to a bug ({})",
            card.reference
        )));
    }

    let record = ApprovalRecord {
        decision: request.decision,
        by: request.by.trim().to_string(),
        source: request.source,
        comment: request.comment.trim().to_string(),
        at: Utc::now(),
    };
    host.write_metadata(&card, recorded(&card.metadata, &record))
        .await?;

    let by = if record.by.is_empty() {
        "someone".to_string()
    } else {
        record.by.clone()
    };
    let via = record.source.label();
    let mut outcome = DecisionOutcome {
        record: record.clone(),
        moved: false,
        delivered: false,
        reply: String::new(),
    };

    match record.decision {
        Decision::Approve => {
            // A person approving is explicit intent, so the card moves
            // whatever the autonomy mode. Lavish is the exception: its
            // approval reaches the planning agent as a follow-up, the agent
            // records the spec approval, and the orchestrator's own gate moves
            // the card from there.
            if record.source == DecisionSource::Lavish {
                outcome.reply = "Approval recorded. The planning agent is recording it and the \
                                 ticket moves on from there."
                    .to_string();
                host.post_card_comment(
                    &card,
                    &format!("gate: plan approved by {by} via {via} — recorded"),
                )
                .await;
            } else {
                match host.move_card(&card, CardStage::Executing).await {
                    Ok(()) => {
                        outcome.moved = true;
                        outcome.reply =
                            "Approval recorded. The ticket has moved to Executing.".to_string();
                        host.post_card_comment(
                            &card,
                            &format!(
                                "gate: plan approved by {by} via {via} — advanced to Executing"
                            ),
                        )
                        .await;
                    }
                    Err(error) => {
                        outcome.reply = "Approval recorded, but the ticket stays in Plan: the \
                                         analysis has not yet met the bar the gate requires."
                            .to_string();
                        host.post_card_comment(
                            &card,
                            &format!(
                                "gate: plan approved by {by} via {via}, but the advance was \
                                 rejected: {error}"
                            ),
                        )
                        .await;
                    }
                }
            }
        }
        Decision::RequestChanges => {
            let reasons = if record.comment.is_empty() {
                "(no reasons given — ask on the card if they are not clear)".to_string()
            } else {
                record.comment.clone()
            };
            let prompt = format!(
                "{by} asked for changes to your plan on {}, via {via}:\n\n{reasons}\n\n\
                 Revise the spec/RCA accordingly and update your recorded verdict \
                 (open_questions / confidence and summary) via the Prepcook MCP `update_issue` \
                 planning fields. Then end your turn.",
                card.reference
            );
            outcome.delivered = host.send_to_agent(&card, &prompt).await?;
            if outcome.delivered {
                outcome.reply = "Changes sent to the agent. Any approval already recorded has \
                                 been cleared."
                    .to_string();
                host.post_card_comment(
                    &card,
                    &format!(
                        "gate: changes requested by {by} via {via} — sent to the planning agent"
                    ),
                )
                .await;
            } else {
                outcome.reply = "Changes noted, but there is no live planning session to deliver \
                                 them to. Use REPLAN to start planning again with this feedback."
                    .to_string();
                host.post_card_comment(
                    &card,
                    &format!(
                        "gate: changes requested by {by} via {via}, but no live planning \
                         session was found to deliver them to"
                    ),
                )
                .await;
            }
        }
        Decision::Replan => match host.move_card(&card, CardStage::Planning).await {
            Ok(()) => {
                outcome.moved = true;
                host.after_replan(&card).await;
                outcome.reply = "Planning has been restarted with this feedback.".to_string();
                host.post_card_comment(
                    &card,
                    &format!("gate: re-plan requested by {by} via {via} — moved back to Plan"),
                )
                .await;
            }
            Err(error) => {
                outcome.reply = "Could not move this ticket back to planning. Someone will need \
                                 to move it on the board."
                    .to_string();
                host.post_card_comment(
                    &card,
                    &format!(
                        "gate: re-plan requested by {by} via {via}, but moving the card back to \
                         Plan failed: {error}"
                    ),
                )
                .await;
            }
        },
    }
    Ok(outcome)
}

/// The card's metadata with `record` applied: appended to `approvals`, and
/// the spec approval and claim marker updated as the decision requires.
/// Keys this module does not own are left as they were.
pub fn recorded(metadata: &Value, record: &ApprovalRecord) -> Value {
    let mut blob = match metadata {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    let typed = IssueExtMetadata::from_value(metadata);

    let mut approvals = typed.approvals.clone();
    approvals.push(record.clone());
    blob.insert(
        "approvals".to_string(),
        serde_json::to_value(&approvals).unwrap_or(Value::Array(Vec::new())),
    );

    match record.decision {
        Decision::Approve => {
            let spec = SpecRef {
                path: typed.spec.as_ref().and_then(|spec| spec.path.clone()),
                approved_at: Some(record.at),
                approved_by: (!record.by.is_empty()).then(|| record.by.clone()),
                approved_via: Some(record.source.approval_source()),
            };
            if let Ok(spec) = serde_json::to_value(&spec) {
                blob.insert("spec".to_string(), spec);
            }
        }
        // Changes asked for after a sign-off mean the sign-off no longer
        // describes what is there.
        Decision::RequestChanges => {
            if let Some(Value::Object(spec)) = blob.get_mut("spec") {
                spec.remove("approved_at");
                spec.remove("approved_by");
                spec.remove("approved_via");
            }
        }
        // The claim marker makes the scan skip an already-claimed card;
        // clearing it is what lets the planning agent start again once the
        // card is back in Plan. Everything else a fresh plan needs is cleared
        // by the claim path when it picks the card up.
        Decision::Replan => {
            blob.remove("playbook_started_at");
            if matches!(record.source, DecisionSource::Card | DecisionSource::Api)
                && !record.comment.is_empty()
            {
                let feedback = Value::String(format!(
                    "{} asked for a re-plan: {}",
                    if record.by.is_empty() {
                        "A reviewer"
                    } else {
                        record.by.as_str()
                    },
                    record.comment
                ));
                match blob.get_mut("autonomy") {
                    Some(Value::Object(autonomy)) => {
                        autonomy.insert("replan_feedback".to_string(), feedback);
                    }
                    _ => {
                        let mut autonomy = serde_json::Map::new();
                        autonomy.insert("replan_feedback".to_string(), feedback);
                        blob.insert("autonomy".to_string(), Value::Object(autonomy));
                    }
                }
            }
        }
    }
    Value::Object(blob)
}

/// The decisions recorded on a card, oldest first.
pub fn records_of(metadata: &Value) -> Vec<ApprovalRecord> {
    IssueExtMetadata::from_value(metadata).approvals
}

/// A handle on [`apply_decision`] for code that is not itself the host: the
/// approvals route and the Lavish relay.
#[derive(Clone)]
pub struct Decisions {
    host: Arc<dyn DecisionHost>,
}

impl Decisions {
    pub fn new(host: Arc<dyn DecisionHost>) -> Self {
        Self { host }
    }

    pub async fn apply_decision(
        &self,
        request: DecisionRequest,
    ) -> Result<DecisionOutcome, DecisionError> {
        apply_decision(self.host.as_ref(), request).await
    }

    /// `Ok(None)` for a card that does not exist.
    pub async fn records(
        &self,
        issue_id: Uuid,
    ) -> Result<Option<Vec<ApprovalRecord>>, DecisionError> {
        Ok(self
            .host
            .load_card(issue_id)
            .await?
            .map(|card| records_of(&card.metadata)))
    }

    pub async fn issue_for_workspace(&self, local_workspace_id: Uuid) -> Option<Uuid> {
        self.host.issue_for_workspace(local_workspace_id).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn jira_review_verbs_are_read_the_way_people_type_them() {
        use Decision::*;
        let parse = parse_tracker_decision;

        assert_eq!(parse("RCA: APPROVED"), Some((Approve, String::new())));
        // Case and surrounding whitespace are what a comment box produces.
        assert_eq!(parse("\n  rca: approved\n"), Some((Approve, String::new())));
        assert_eq!(
            parse("RCA: CHANGES — the token expiry is not proven\nsee the logs"),
            Some((
                RequestChanges,
                "the token expiry is not proven\nsee the logs".to_string()
            ))
        );
        assert_eq!(
            parse("REPLAN: the approach is wrong"),
            Some((Replan, "the approach is wrong".to_string()))
        );
        assert_eq!(
            parse("REPLAN\n\nstart from the queue, not the renderer"),
            Some((Replan, "start from the queue, not the renderer".to_string()))
        );

        // The one that would restart somebody's work by accident.
        assert_eq!(parse("Replanning the sprint after this lands"), None);
        assert_eq!(parse("I think we should replan"), None);
        // Prose about an RCA is not a decision about one.
        assert_eq!(parse("The RCA looks right to me"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn card_decisions_are_read_from_the_first_line() {
        use Decision::*;
        let parse = parse_card_decision;

        assert_eq!(parse("APPROVE"), Some((Approve, String::new())));
        assert_eq!(parse("  approve \n"), Some((Approve, String::new())));
        assert_eq!(
            parse("Approve: ship it"),
            Some((Approve, "ship it".to_string()))
        );
        assert_eq!(parse("PLAN: APPROVE"), Some((Approve, String::new())));
        assert_eq!(
            parse("REQUEST CHANGES: cover the slides creator\nand the docs one"),
            Some((
                RequestChanges,
                "cover the slides creator\nand the docs one".to_string()
            ))
        );
        assert_eq!(
            parse("request changes:\n\nthe cause is upstream"),
            Some((RequestChanges, "the cause is upstream".to_string()))
        );
        assert_eq!(
            parse("REPLAN: start from the queue"),
            Some((Replan, "start from the queue".to_string()))
        );
        assert_eq!(parse("Replan"), Some((Replan, String::new())));
    }

    #[test]
    fn prose_that_opens_with_a_verb_is_not_a_card_decision() {
        for body in [
            "Approve once CI is green?",
            "APPROVED",
            "Request changes to the header copy",
            "Replanning the sprint after this lands",
            "REPLANNED: yesterday",
            // The agent's and the orchestrator's own lines.
            "REVIEW: APPROVE",
            "gate: plan approved by Ada via the API — recorded",
            "PLAN: the second option reads better",
            "",
        ] {
            assert_eq!(parse_card_decision(body), None, "{body:?}");
        }
    }

    #[test]
    fn a_decision_applies_only_where_it_means_something() {
        use CardStage::*;
        for stage in [Planning, Executing, Reviewing, Shipped] {
            assert!(takes_decision(Decision::Replan, stage));
        }
        assert!(!takes_decision(Decision::Replan, Other));
        assert!(takes_decision(Decision::Approve, Planning));
        assert!(takes_decision(Decision::RequestChanges, Planning));
        for stage in [Executing, Reviewing, Shipped, Other] {
            assert!(!takes_decision(Decision::Approve, stage));
            assert!(!takes_decision(Decision::RequestChanges, stage));
        }
    }

    /// An in-memory card that records every side effect.
    struct FakeHost {
        card: Mutex<Option<DecisionCard>>,
        comments: Mutex<Vec<String>>,
        moves: Mutex<Vec<CardStage>>,
        prompts: Mutex<Vec<String>>,
        live_session: bool,
    }

    impl FakeHost {
        fn with(stage: CardStage, metadata: Value) -> Self {
            Self {
                card: Mutex::new(Some(DecisionCard {
                    issue_id: Uuid::nil(),
                    project_id: Uuid::nil(),
                    reference: "BUG-1".to_string(),
                    stage,
                    metadata,
                })),
                comments: Mutex::default(),
                moves: Mutex::default(),
                prompts: Mutex::default(),
                live_session: true,
            }
        }

        fn metadata(&self) -> IssueExtMetadata {
            IssueExtMetadata::from_value(&self.card.lock().unwrap().as_ref().unwrap().metadata)
        }

        fn raw(&self) -> Value {
            self.card.lock().unwrap().as_ref().unwrap().metadata.clone()
        }
    }

    #[async_trait]
    impl DecisionHost for FakeHost {
        async fn load_card(&self, issue_id: Uuid) -> Result<Option<DecisionCard>, DecisionError> {
            Ok(self
                .card
                .lock()
                .unwrap()
                .clone()
                .filter(|card| card.issue_id == issue_id))
        }

        async fn write_metadata(
            &self,
            _card: &DecisionCard,
            metadata: Value,
        ) -> Result<(), DecisionError> {
            self.card.lock().unwrap().as_mut().unwrap().metadata = metadata;
            Ok(())
        }

        async fn post_card_comment(&self, _card: &DecisionCard, text: &str) {
            self.comments.lock().unwrap().push(text.to_string());
        }

        async fn send_to_agent(
            &self,
            _card: &DecisionCard,
            prompt: &str,
        ) -> Result<bool, DecisionError> {
            if self.live_session {
                self.prompts.lock().unwrap().push(prompt.to_string());
            }
            Ok(self.live_session)
        }

        async fn move_card(
            &self,
            _card: &DecisionCard,
            to: CardStage,
        ) -> Result<(), DecisionError> {
            self.moves.lock().unwrap().push(to);
            Ok(())
        }

        async fn after_replan(&self, _card: &DecisionCard) {}

        async fn issue_for_workspace(&self, _local_workspace_id: Uuid) -> Option<Uuid> {
            None
        }
    }

    fn request(decision: Decision, source: DecisionSource) -> DecisionRequest {
        DecisionRequest {
            issue_id: Uuid::nil(),
            decision,
            comment: " looks right ".to_string(),
            by: "Ada".to_string(),
            source,
        }
    }

    /// An investigation finishes at its report: approving it, or asking for
    /// changes to a plan it does not have, is refused and changes nothing.
    /// A re-plan still runs planning again.
    #[tokio::test]
    async fn an_investigation_is_never_approved() {
        let metadata = serde_json::json!({ "work_class": "investigation" });
        for decision in [Decision::Approve, Decision::RequestChanges] {
            let host = FakeHost::with(CardStage::Planning, metadata.clone());
            let error = apply_decision(&host, request(decision, DecisionSource::Card))
                .await
                .unwrap_err();
            assert!(matches!(error, DecisionError::NotForClass(_)), "{error:?}");
            assert!(host.moves.lock().unwrap().is_empty());
            assert_eq!(host.raw(), metadata);
        }
        let host = FakeHost::with(CardStage::Planning, metadata);
        let outcome = apply_decision(&host, request(Decision::Replan, DecisionSource::Card))
            .await
            .unwrap();
        assert!(outcome.moved);
    }

    #[tokio::test]
    async fn an_approval_is_recorded_with_its_source() {
        let host = FakeHost::with(
            CardStage::Planning,
            serde_json::json!({ "spec": { "path": "rca.html" }, "foreign": 1 }),
        );
        let outcome = apply_decision(&host, request(Decision::Approve, DecisionSource::Api))
            .await
            .unwrap();

        assert!(outcome.moved);
        assert_eq!(outcome.record.by, "Ada");
        assert_eq!(outcome.record.comment, "looks right");
        assert_eq!(*host.moves.lock().unwrap(), vec![CardStage::Executing]);

        let metadata = host.metadata();
        assert_eq!(metadata.approvals, vec![outcome.record.clone()]);
        let spec = metadata.spec.unwrap();
        assert_eq!(spec.path.as_deref(), Some("rca.html"));
        assert_eq!(spec.approved_at, Some(outcome.record.at));
        assert_eq!(spec.approved_by.as_deref(), Some("Ada"));
        assert_eq!(spec.approved_via, Some(api_types::ApprovalSource::Api));
        // A key another tool owns survives the write.
        assert_eq!(host.raw()["foreign"], 1);
        assert!(host.comments.lock().unwrap()[0].contains("advanced to Executing"));
    }

    /// A card made on the board, with no tracker anywhere: decisions need
    /// nothing but the card, so a comment on it or a call to the API moves it
    /// exactly as it would an imported one. `DecisionHost` has no tracker in
    /// it, which is what keeps this true.
    #[tokio::test]
    async fn a_board_only_card_is_decided_from_the_card_or_the_api() {
        let (decision, comment) = parse_card_decision("APPROVE").unwrap();
        for source in [DecisionSource::Card, DecisionSource::Api] {
            let host = FakeHost::with(
                CardStage::Planning,
                serde_json::json!({
                    "work_class": "bug",
                    "planning": { "kind": "rca", "confidence": "high" },
                }),
            );
            host.card.lock().unwrap().as_mut().unwrap().reference = "DEF-1".to_string();
            let outcome = apply_decision(
                &host,
                DecisionRequest {
                    issue_id: Uuid::nil(),
                    decision,
                    comment: comment.clone(),
                    by: "Ada".to_string(),
                    source,
                },
            )
            .await
            .unwrap();
            assert!(outcome.moved, "{source:?}");
            assert_eq!(*host.moves.lock().unwrap(), vec![CardStage::Executing]);
            let metadata = host.metadata();
            assert!(metadata.jira.is_none());
            assert_eq!(metadata.approvals.len(), 1);
            assert!(metadata.spec.unwrap().approved_at.is_some());
            assert!(!host.comments.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn a_human_approval_advances_the_card() {
        for source in [
            DecisionSource::Card,
            DecisionSource::Tracker,
            DecisionSource::Api,
        ] {
            let host = FakeHost::with(CardStage::Planning, Value::Null);
            let outcome = apply_decision(&host, request(Decision::Approve, source))
                .await
                .unwrap();
            assert!(outcome.moved, "{source:?}");
            assert_eq!(*host.moves.lock().unwrap(), vec![CardStage::Executing]);
            assert!(host.metadata().spec.unwrap().approved_at.is_some());
            assert_eq!(host.metadata().approvals.len(), 1);
        }
    }

    #[tokio::test]
    async fn a_lavish_approval_leaves_the_move_to_the_agent() {
        let host = FakeHost::with(CardStage::Planning, Value::Null);
        let outcome = apply_decision(&host, request(Decision::Approve, DecisionSource::Lavish))
            .await
            .unwrap();
        assert!(!outcome.moved);
        assert_eq!(
            host.metadata().spec.unwrap().approved_via,
            Some(api_types::ApprovalSource::Lavish)
        );
    }

    #[tokio::test]
    async fn changes_clear_the_approval_and_reach_the_agent() {
        let host = FakeHost::with(
            CardStage::Planning,
            serde_json::json!({
                "spec": { "path": "rca.html", "approved_at": "2026-01-01T00:00:00Z" },
                "approvals": [{
                    "decision": "approve", "by": "Lin", "source": "tracker",
                    "comment": "", "at": "2026-01-01T00:00:00Z"
                }]
            }),
        );
        let outcome = apply_decision(
            &host,
            request(Decision::RequestChanges, DecisionSource::Tracker),
        )
        .await
        .unwrap();
        assert!(outcome.delivered);
        let metadata = host.metadata();
        assert_eq!(metadata.approvals.len(), 2);
        assert_eq!(metadata.approvals[1].decision, Decision::RequestChanges);
        let spec = metadata.spec.unwrap();
        assert_eq!(spec.approved_at, None);
        assert_eq!(spec.path.as_deref(), Some("rca.html"));
        assert!(host.prompts.lock().unwrap()[0].contains("looks right"));
    }

    #[tokio::test]
    async fn a_replan_clears_the_claim_and_moves_back_to_plan() {
        let host = FakeHost::with(
            CardStage::Shipped,
            serde_json::json!({ "playbook_started_at": "2026-01-01T00:00:00Z" }),
        );
        let outcome = apply_decision(&host, request(Decision::Replan, DecisionSource::Card))
            .await
            .unwrap();
        assert!(outcome.moved);
        assert_eq!(*host.moves.lock().unwrap(), vec![CardStage::Planning]);
        assert!(host.raw().get("playbook_started_at").is_none());
        // The reasons wait on the card for the next planning agent.
        assert_eq!(
            host.metadata().autonomy.unwrap().replan_feedback.as_deref(),
            Some("Ada asked for a re-plan: looks right")
        );
    }

    #[tokio::test]
    async fn a_tracker_replan_leaves_its_reasons_to_the_comment_forwarder() {
        let host = FakeHost::with(CardStage::Executing, Value::Null);
        apply_decision(&host, request(Decision::Replan, DecisionSource::Tracker))
            .await
            .unwrap();
        assert!(
            host.metadata()
                .autonomy
                .and_then(|state| state.replan_feedback)
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_decision_out_of_stage_is_refused_and_not_recorded() {
        let host = FakeHost::with(CardStage::Executing, Value::Null);
        let error = apply_decision(&host, request(Decision::Approve, DecisionSource::Api))
            .await
            .unwrap_err();
        assert!(matches!(error, DecisionError::WrongStage { .. }));
        assert!(host.metadata().approvals.is_empty());

        let mut unknown = request(Decision::Approve, DecisionSource::Api);
        unknown.issue_id = Uuid::new_v4();
        assert!(matches!(
            apply_decision(&host, unknown).await,
            Err(DecisionError::UnknownCard(_))
        ));
    }
}
