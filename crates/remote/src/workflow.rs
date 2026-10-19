//! Prepcook kanban workflow transition rules.
//!
//! Enforced only at the HTTP route layer (`routes/issues.rs`). Internal
//! automation calls repository functions directly and is intentionally
//! exempt from these rules.
//!
//! The frontend mirrors these rules in
//! `packages/web-core/src/shared/lib/workflowTransitions.ts`; keep the two
//! implementations in sync.

use api_types::issue_ext::{ConfidenceLevel, IssueExtMetadata, WorkClass};
use serde_json::Value;

/// Ordered Prepcook workflow stages (first alias matches `DEFAULT_STATUSES` in
/// `db/project_statuses.rs`; later aliases are earlier column names kept so
/// existing projects keep validating until they're renamed). Columns with
/// other names are not subject to transition validation — Triaged among them:
/// it sits between Plan and Execute but is a side track, entered from Plan by
/// an investigation and left for Execute or Plan when one is converted.
pub const WORKFLOW_STAGES: [&[&str]; 7] = [
    &["To Do", "Unassigned"],
    &["Plan", "Planning"],
    &["Execute", "Executing"],
    &["Review", "Reviewing"],
    &["Push MR", "Pushed MR"],
    &["Close", "Deployed", "Closed"],
    &["Clean up", "Clean-up", "Cleanup"],
];

/// Index of "Close" in [`WORKFLOW_STAGES`]; any stage may jump here
/// directly (manual close).
const CLOSED_INDEX: usize = 5;

fn stage_index(name: &str) -> Option<usize> {
    let name = name.trim();
    WORKFLOW_STAGES
        .iter()
        .position(|aliases| aliases.iter().any(|alias| alias.eq_ignore_ascii_case(name)))
}

/// Returns true when moving an issue from `from_name` to `to_name` is allowed.
///
/// Rules (stage names are matched case-insensitively):
/// - Either name outside the Prepcook stage list => allowed (non-workflow project
///   or hidden/custom columns).
/// - Same stage (no-op) or any backward move => allowed.
/// - Forward moves are only allowed to the immediately next stage.
/// - Exception: any stage may move directly to "Closed" (manual close).
/// - Consequence: "Clean-up" is only reachable from "Closed".
pub fn validate_transition(from_name: &str, to_name: &str) -> bool {
    let (Some(from), Some(to)) = (stage_index(from_name), stage_index(to_name)) else {
        return true;
    };

    if to <= from {
        return true;
    }
    if to == CLOSED_INDEX {
        return true;
    }
    to == from + 1
}

/// True when both names are workflow stages and the move is backward or
/// same-stage. The orchestrator claim marker survives such moves — parking a
/// ticket back a column must not re-trigger that column's automation; only a
/// forward entry re-arms it.
pub fn is_backward_move(from_name: &str, to_name: &str) -> bool {
    match (stage_index(from_name), stage_index(to_name)) {
        (Some(from), Some(to)) => to <= from,
        _ => false,
    }
}

fn confidence_label(level: ConfidenceLevel) -> &'static str {
    match level {
        ConfidenceLevel::Low => "low",
        ConfidenceLevel::Medium => "medium",
        ConfidenceLevel::High => "high",
    }
}

/// Planning → Executing gate: Prepcook workflow issues need BOTH a passing
/// planning verdict AND a human Lavish approval (`spec.approved_at`) recorded
/// in `extension_metadata` before execution may start. Specs require
/// `open_questions == 0`; RCAs require `confidence == high`.
///
/// Returns `None` when the move is allowed, or an actionable rejection
/// message. Only applies to the Planning → Executing move; generic tickets
/// (metadata with neither `work_class` nor `planning`) are exempt.
pub fn planning_gate_error(
    from_name: &str,
    to_name: &str,
    extension_metadata: &Value,
) -> Option<String> {
    let is_planning_to_executing = from_name.trim().eq_ignore_ascii_case("Planning")
        && to_name.trim().eq_ignore_ascii_case("Executing");
    if !is_planning_to_executing {
        return None;
    }

    let metadata = IssueExtMetadata::from_value(extension_metadata);
    // Generic tickets never went through the Prepcook planning flow.
    if metadata.work_class.is_none() && metadata.planning.is_none() {
        return None;
    }
    if metadata.work_class == Some(WorkClass::Investigation) {
        return Some("investigation cards do not execute; convert to a bug first".to_string());
    }

    let Some(verdict) = metadata.planning else {
        return Some(
            "no planning verdict recorded yet — the planning agent must record one before this issue can move to Executing"
                .to_string(),
        );
    };

    let verdict_error = match verdict.kind.as_deref().map(str::trim) {
        Some(kind) if kind.eq_ignore_ascii_case("spec") => match verdict.open_questions {
            Some(0) => None,
            Some(count) => Some(format!(
                "spec has {count} open question{} — resolve them and update the planning verdict before executing",
                if count == 1 { "" } else { "s" }
            )),
            None => Some(
                "spec verdict has no open-question count — record open_questions (must be 0) before executing"
                    .to_string(),
            ),
        },
        Some(kind) if kind.eq_ignore_ascii_case("rca") => match verdict.confidence {
            Some(ConfidenceLevel::High) => None,
            Some(level) => Some(format!(
                "RCA confidence is {} — raise it to high before executing",
                confidence_label(level)
            )),
            None => Some(
                "RCA confidence is not recorded — record high confidence before executing"
                    .to_string(),
            ),
        },
        Some(other) => Some(format!(
            "planning verdict has unknown kind '{other}' (expected \"spec\" or \"rca\")"
        )),
        None => Some(
            "planning verdict is missing its kind (\"spec\" or \"rca\") — record it before executing"
                .to_string(),
        ),
    };
    if verdict_error.is_some() {
        return verdict_error;
    }

    // Both conditions gate the move: a passing verdict AND an explicit human
    // approval recorded from the Lavish review gate.
    let approved = metadata
        .spec
        .as_ref()
        .and_then(|spec| spec.approved_at)
        .is_some();
    if !approved {
        return Some(
            "spec/RCA has not been approved in the Lavish review gate — open the artifact and click Approve before executing"
                .to_string(),
        );
    }
    None
}

/// Human-readable rejection message, e.g.
/// "Invalid workflow transition: To Do → Executing (must go through Planning)".
pub fn transition_error_message(from_name: &str, to_name: &str) -> String {
    let next_stage = stage_index(from_name)
        .and_then(|index| WORKFLOW_STAGES.get(index + 1))
        .and_then(|aliases| aliases.first())
        .copied();

    match next_stage {
        Some(next) => {
            format!("Invalid workflow transition: {from_name} → {to_name} (must go through {next})")
        }
        None => format!("Invalid workflow transition: {from_name} → {to_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_same_status_noop() {
        for aliases in WORKFLOW_STAGES {
            for stage in aliases {
                assert!(validate_transition(stage, stage), "{stage} → {stage}");
            }
        }
    }

    #[test]
    fn allows_forward_to_immediate_next_stage() {
        assert!(validate_transition("To Do", "Plan"));
        assert!(validate_transition("Plan", "Execute"));
        assert!(validate_transition("Execute", "Review"));
        assert!(validate_transition("Review", "Push MR"));
        assert!(validate_transition("Push MR", "Close"));
        assert!(validate_transition("Close", "Clean up"));
    }

    #[test]
    fn legacy_column_names_still_validate() {
        assert!(validate_transition("Unassigned", "Planning"));
        assert!(validate_transition("Pushed MR", "Closed"));
        assert!(validate_transition("Closed", "Clean-up"));
        // Mixed old/new naming across a rename in progress.
        assert!(validate_transition("Unassigned", "planning"));
        assert!(!validate_transition("Unassigned", "Executing"));
        assert!(validate_transition("Pushed MR", "Deployed"));
    }

    #[test]
    fn rejects_forward_skips() {
        assert!(!validate_transition("To Do", "Execute"));
        assert!(!validate_transition("To Do", "Review"));
        assert!(!validate_transition("Plan", "Review"));
        assert!(!validate_transition("Plan", "Push MR"));
        assert!(!validate_transition("Execute", "Push MR"));
    }

    #[test]
    fn allows_backward_moves() {
        assert!(validate_transition("Plan", "To Do"));
        assert!(validate_transition("Review", "Plan"));
        assert!(validate_transition("Close", "Execute"));
        assert!(validate_transition("Clean up", "To Do"));
    }

    #[test]
    fn backward_move_detection() {
        // Backward and same-stage moves keep the orchestrator claim marker.
        assert!(is_backward_move("Push MR", "Review"));
        assert!(is_backward_move("Pushed MR", "Reviewing")); // legacy names
        assert!(is_backward_move("Execute", "Execute"));
        assert!(is_backward_move("Clean up", "To Do"));
        // Forward moves re-arm automation (marker cleared).
        assert!(!is_backward_move("Review", "Push MR"));
        assert!(!is_backward_move("Plan", "Execute"));
        // Unknown/custom columns never suppress the clear.
        assert!(!is_backward_move("Backlog", "Review"));
        assert!(!is_backward_move("Review", "Icebox"));
    }

    #[test]
    fn allows_manual_close_from_any_stage() {
        for aliases in WORKFLOW_STAGES {
            let stage = aliases[0];
            assert!(validate_transition(stage, "Close"), "{stage} → Close");
        }
    }

    #[test]
    fn clean_up_only_reachable_from_closed() {
        assert!(validate_transition("Close", "Clean up"));
        for stage in ["To Do", "Plan", "Execute", "Review", "Push MR"] {
            assert!(
                !validate_transition(stage, "Clean up"),
                "{stage} → Clean up"
            );
        }
    }

    #[test]
    fn unknown_columns_bypass_validation() {
        assert!(validate_transition("Backlog", "Executing"));
        assert!(validate_transition("To Do", "Icebox"));
        assert!(validate_transition("Backlog", "Icebox"));
    }

    #[test]
    fn stage_names_are_case_insensitive_and_trimmed() {
        assert!(!validate_transition("to do", "EXECUTE"));
        assert!(validate_transition("plan", "execute"));
        assert!(validate_transition(" push mr ", "close"));
    }

    #[test]
    fn error_message_names_the_required_next_stage() {
        assert_eq!(
            transition_error_message("To Do", "Execute"),
            "Invalid workflow transition: To Do → Execute (must go through Plan)"
        );
    }

    #[test]
    fn planning_gate_ignores_other_transitions() {
        let metadata = serde_json::json!({ "work_class": "bug" });
        assert_eq!(planning_gate_error("To Do", "Plan", &metadata), None);
        assert_eq!(planning_gate_error("Execute", "Review", &metadata), None);
        assert_eq!(planning_gate_error("Execute", "Plan", &metadata), None);
    }

    #[test]
    fn planning_gate_skips_generic_tickets() {
        assert_eq!(
            planning_gate_error("Planning", "Executing", &serde_json::json!({})),
            None
        );
        assert_eq!(
            planning_gate_error(
                "Planning",
                "Executing",
                &serde_json::json!({ "mr_url": "https://example.com/mr/1" })
            ),
            None
        );
    }

    #[test]
    fn planning_gate_requires_a_verdict_for_classified_work() {
        let metadata = serde_json::json!({ "work_class": "story" });
        let message = planning_gate_error("Planning", "Executing", &metadata)
            .expect("classified work without a verdict must be rejected");
        assert!(
            message.contains("no planning verdict recorded yet"),
            "{message}"
        );
    }

    #[test]
    fn planning_gate_stage_names_are_case_insensitive() {
        let metadata = serde_json::json!({ "work_class": "bug" });
        assert!(planning_gate_error(" planning ", "EXECUTING", &metadata).is_some());
    }

    #[test]
    fn an_investigation_never_moves_to_executing() {
        let metadata = serde_json::json!({
            "work_class": "investigation",
            "planning": { "kind": "rca", "confidence": "high" },
            "spec": { "path": "docs/specs/PC-1.html", "approved_at": "2026-07-10T00:00:00Z" }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("convert to a bug first"), "{message}");
        // Triaged is a side track the transition rules leave alone.
        assert!(validate_transition("Plan", "Triaged"));
        assert!(validate_transition("Triaged", "Execute"));
        assert!(validate_transition("Triaged", "Plan"));
    }

    #[test]
    fn spec_verdict_passes_with_zero_open_questions_and_approval() {
        let metadata = serde_json::json!({
            "work_class": "story",
            "planning": { "kind": "spec", "open_questions": 0 },
            "spec": { "path": "docs/specs/BUG-1.html", "approved_at": "2026-07-10T00:00:00Z" }
        });
        assert_eq!(
            planning_gate_error("Planning", "Executing", &metadata),
            None
        );
    }

    #[test]
    fn passing_verdict_without_lavish_approval_is_rejected() {
        let metadata = serde_json::json!({
            "work_class": "story",
            "planning": { "kind": "spec", "open_questions": 0 }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("Lavish review gate"), "{message}");

        // Path recorded but never approved.
        let metadata = serde_json::json!({
            "work_class": "bug",
            "planning": { "kind": "rca", "confidence": "high" },
            "spec": { "path": "docs/specs/BUG-2.html" }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("Lavish review gate"), "{message}");
    }

    #[test]
    fn spec_verdict_rejected_with_open_questions() {
        let metadata = serde_json::json!({
            "work_class": "story",
            "planning": { "kind": "spec", "open_questions": 3 }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("spec has 3 open questions"), "{message}");

        let metadata = serde_json::json!({
            "planning": { "kind": "spec", "open_questions": 1 }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("spec has 1 open question"), "{message}");
    }

    #[test]
    fn spec_verdict_rejected_without_open_question_count() {
        let metadata = serde_json::json!({
            "planning": { "kind": "spec", "confidence": "high" }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("no open-question count"), "{message}");
    }

    #[test]
    fn rca_verdict_passes_with_high_confidence_and_approval() {
        let metadata = serde_json::json!({
            "work_class": "bug",
            "planning": { "kind": "rca", "confidence": "high" },
            "spec": { "path": "docs/specs/BUG-3.html", "approved_at": "2026-07-10T00:00:00Z" }
        });
        assert_eq!(
            planning_gate_error("Planning", "Executing", &metadata),
            None
        );
    }

    #[test]
    fn rca_verdict_rejected_below_high_confidence() {
        let metadata = serde_json::json!({
            "work_class": "bug",
            "planning": { "kind": "rca", "confidence": "medium" }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(
            message.contains("RCA confidence is medium — raise it to high"),
            "{message}"
        );

        let metadata = serde_json::json!({
            "planning": { "kind": "rca" }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(
            message.contains("RCA confidence is not recorded"),
            "{message}"
        );
    }

    #[test]
    fn verdict_with_missing_or_unknown_kind_is_rejected() {
        let metadata = serde_json::json!({
            "planning": { "open_questions": 0 }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("missing its kind"), "{message}");

        let metadata = serde_json::json!({
            "planning": { "kind": "vibes", "open_questions": 0 }
        });
        let message = planning_gate_error("Planning", "Executing", &metadata).unwrap();
        assert!(message.contains("unknown kind 'vibes'"), "{message}");
    }
}
