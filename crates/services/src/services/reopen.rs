//! Filing a follow-up ticket when shipped work did not hold.
//!
//! A report that a merged fix did not work is new work, not a continuation.
//! Re-arming the original card hands the second attempt the first attempt's
//! branch, merge request, review rounds and claim state, and leaves the team
//! with one ticket whose meaning changed halfway through — which is precisely
//! what makes these hard for a TAM to follow. A fresh linked ticket keeps each
//! attempt separately trackable and readable from either end in Jira.
//!
//! Nothing here creates a board card. The follow-up is assigned to the agent's
//! Jira account, and the watcher's assigned sweep imports anything assigned and
//! not done straight into Plan — so filing *is* starting the work.

use api_types::{IssueExtMetadata, ReopenState, UpdateIssueRequest};
use chrono::Utc;
use tracing::{debug, info, warn};

use crate::services::{
    remote_client::RemoteClient,
    tracker::{NewIssue, Tracker, TrackerComment},
};

/// First line a reporter writes to say a shipped fix did not work.
///
/// Matched on the first line only, like the `REVIEW:` verdicts: a marker
/// mentioned mid-sentence ("we should have said NOT FIXED") is discussion about
/// the convention, not an invocation of it.
pub const REOPEN_MARKER: &str = "NOT FIXED";

/// Jira status a ticket waits in while its follow-up runs.
///
/// In the `new` category, which the reopen sweep also matches — parking a
/// ticket here is only safe because [`existing_followup_action`] says nothing
/// when an open follow-up already owns the problem.
pub const BLOCKED_STATUS: &str = "Blocked";

/// Jira label every follow-up carries.
///
/// It is what tells the rest of the system that a ticket is a second attempt,
/// because nothing else about the Jira issue says so: the summary prefix is
/// prose a human can edit away, and the blocks link is a relationship rather
/// than a property of the ticket. Assigned pickup reads it to dispatch the
/// follow-up ahead of the queue (see `jira_watcher::priority_for`).
pub const FOLLOWUP_LABEL: &str = "prepcook-followup";

/// Why a follow-up is being filed. Only used for wording — both paths file the
/// same thing.
#[derive(Debug, Clone)]
pub enum ReopenTrigger {
    /// The reporter moved the Jira ticket back out of the done category.
    StatusReopened,
    /// A comment whose first line carries [`REOPEN_MARKER`].
    Marker,
}

impl ReopenTrigger {
    fn describe(&self) -> &'static str {
        match self {
            ReopenTrigger::StatusReopened => "the ticket was moved back out of Done in Jira",
            ReopenTrigger::Marker => "a reporter replied with the reopen marker",
        }
    }
}

/// What filing did, so callers can log it and advance their own watermarks.
#[derive(Debug)]
pub enum Outcome {
    /// A new ticket was filed; the original now depends on it.
    Filed(String),
    /// A follow-up from an earlier report is still open, so this report was
    /// added to it rather than opening a third ticket for one problem.
    Appended(String),
    /// An open follow-up already owns the problem and the trigger carried no
    /// report to add. Nothing is written anywhere, deliberately.
    ///
    /// The status trigger re-fires on every sweep for as long as the original
    /// sits outside the done category — which is indefinitely, since a reopened
    /// ticket stays where the reporter put it. Each fire used to append the same
    /// header over an empty body: eight identical comments landed on one P0
    /// follow-up in under three hours on 2026-09-01.
    AlreadyOpen(String),
    /// No tracker is configured. A follow-up is a tracker ticket, so nothing
    /// was filed or written; the card is the only record of the work.
    NoTracker,
}

/// Priority for a follow-up, given the original's.
///
/// A P0 caps to P1. The P0 remains the single record of that incident and the
/// follow-up is linked as its blocker, so filing the follow-up at P0 as well
/// would count one incident twice in the blocker rate. Every other priority
/// carries across unchanged — a ticket coming back is not by itself grounds to
/// raise it, and a P3 that reopens is still a P3.
///
/// The cap costs nothing in dispatch order: assigned pickup ranks a labelled
/// follow-up Urgent on the board whatever its Jira priority says.
fn followup_priority(original: Option<&str>) -> &str {
    match original {
        Some("P0") => "P1",
        Some(other) => other,
        // Priority is unset on some imported issues; P2 is the project default.
        None => "P2",
    }
}

/// What an existing follow-up means for a fresh trigger.
#[derive(Debug, PartialEq, Eq)]
enum ExistingFollowup {
    /// Open, and this trigger brought no report. The problem is already owned
    /// and there is no news, so nothing is written.
    SayNothing,
    /// Open, and there is a real report to relay. It belongs on the follow-up,
    /// not on a third ticket for one problem.
    Append,
    /// Closed: that attempt shipped and also failed, so the chain continues
    /// with a new ticket.
    Chain,
}

/// Decides what to do about a follow-up that already exists.
///
/// A table rather than match guards because the third case is what went wrong:
/// an open follow-up with nothing new to say is not the same as an open
/// follow-up with a second report, and treating them alike appended one empty
/// comment per sweep for as long as the original stayed reopened.
fn existing_followup_action(followup_done: bool, has_reports: bool) -> ExistingFollowup {
    match (followup_done, has_reports) {
        (true, _) => ExistingFollowup::Chain,
        (false, true) => ExistingFollowup::Append,
        (false, false) => ExistingFollowup::SayNothing,
    }
}

/// True when a comment body invokes the reopen marker.
pub fn is_reopen_report(body: &str) -> bool {
    body.lines()
        .next()
        .map(|line| {
            line.trim()
                .trim_start_matches(['*', '_', '#', '>', ' '])
                .to_ascii_uppercase()
                .starts_with(REOPEN_MARKER)
        })
        .unwrap_or(false)
}

/// Files (or appends to) the follow-up for a ticket whose fix did not hold.
///
/// The original is left exactly where the reporter put it. A customer-facing
/// ticket that stays open while the engineering ticket it depends on is worked
/// is the ordinary shape, and moving someone's ticket back to Done under them to
/// tidy the board would be the orchestrator overruling the person who reported
/// the problem.
/// Who should be told about a follow-up: whoever filed the original, and
/// whoever reported that the fix did not hold.
///
/// The reporter comes first and duplicates are dropped, so a TAM who filed the
/// ticket and then reported the regression is added once. The agent's own
/// account is skipped — it is the assignee and already watches by default.
fn followup_watchers(
    reporter: Option<&str>,
    reports: &[TrackerComment],
    agent: Option<&str>,
) -> Vec<String> {
    let mut watchers: Vec<String> = Vec::new();
    let candidates = reporter.into_iter().chain(
        reports
            .iter()
            .filter_map(|report| report.author_account_id.as_deref()),
    );
    for account_id in candidates {
        if Some(account_id) == agent || watchers.iter().any(|seen| seen == account_id) {
            continue;
        }
        watchers.push(account_id.to_string());
    }
    watchers
}

pub async fn file_followup(
    client: &RemoteClient,
    tracker: &dyn Tracker,
    card: &api_types::Issue,
    jira_key: &str,
    trigger: ReopenTrigger,
    reports: &[TrackerComment],
) -> anyhow::Result<Outcome> {
    if !tracker.is_configured() {
        debug!("Reopen: no tracker is configured — no follow-up for {jira_key}");
        return Ok(Outcome::NoTracker);
    }
    let metadata = IssueExtMetadata::from_value(&card.extension_metadata);
    let report_block = format_reports(reports);

    // An open follow-up already owns this problem. A second report belongs on
    // it, not on a third ticket: one complaint should not become a pile that
    // nobody can tell apart. A *closed* follow-up means that attempt shipped and
    // also failed, so the chain continues with a new one.
    if let Some(existing) = metadata
        .reopen
        .as_ref()
        .and_then(|reopen| reopen.followup_key.clone())
    {
        match tracker.get_issue(&existing).await {
            Ok(fields) => match existing_followup_action(fields.is_done(), !reports.is_empty()) {
                ExistingFollowup::SayNothing => {
                    debug!(
                        "Reopen: {jira_key} still depends on the open follow-up {existing}, \
                             and this trigger carried no new report — saying nothing"
                    );
                    return Ok(Outcome::AlreadyOpen(existing));
                }
                ExistingFollowup::Append => {
                    let note = format!(
                        "Another report came in on {jira_key} ({}).\n\n{report_block}",
                        trigger.describe()
                    );
                    tracker.post_comment(&existing, &note).await?;
                    info!("Reopen: appended a further report to the open follow-up {existing}");
                    return Ok(Outcome::Appended(existing));
                }
                ExistingFollowup::Chain => {
                    info!("Reopen: follow-up {existing} is closed — chaining a new one");
                }
            },
            // Unreadable (deleted, permissions, Jira down): filing a second
            // ticket is recoverable, silently dropping the report is not.
            Err(error) => {
                warn!(
                    ?error,
                    "Reopen: could not read follow-up {existing} — filing a new one"
                );
            }
        }
    }

    let original = tracker.get_issue(jira_key).await?;
    let issue_type = original.issue_type.as_deref().unwrap_or("Bug");
    let priority = followup_priority(original.priority.as_deref());
    let original_url = tracker.issue_url(jira_key);

    let mut description = String::new();
    description.push_str(&format!(
        "The fix for {jira_key} shipped, and the reporter says it did not resolve the problem \
         ({}).\n\nOriginal ticket: {original_url}\n",
        trigger.describe()
    ));
    if let Some(mr_url) = metadata.mr_url.as_deref() {
        description.push_str(&format!("Merge request from that attempt: {mr_url}\n"));
    }
    if let Some(spec) = metadata.spec.as_ref().and_then(|spec| spec.path.clone()) {
        description.push_str(&format!("Spec/RCA from that attempt: {spec}\n"));
    }
    description.push_str(
        "\nStart from what the previous attempt concluded rather than from scratch: it was \
         wrong somewhere, and finding where is the job.\n",
    );
    if original.priority.as_deref() == Some("P0") {
        description.push_str(
            "\nThe original is P0 and this follow-up is P1 by design. The P0 stays the single \
             record of the incident and now depends on this ticket; filing this one at P0 too \
             would count one incident twice in the blocker rate.\n",
        );
    }
    if !report_block.is_empty() {
        description.push_str(&format!("\nWhat the reporter said:\n\n{report_block}\n"));
    }

    let summary = followup_summary(
        &original.summary,
        reported_symptom(reports).as_deref(),
        chrono::Utc::now().date_naive(),
    );
    let new_key = tracker
        .create_issue(&NewIssue {
            project_key: None,
            issue_type: issue_type.to_string(),
            summary,
            description,
            priority: priority.to_string(),
            labels: vec![FOLLOWUP_LABEL.to_string()],
        })
        .await?;

    // Assignment is what starts the work: the watcher's assigned sweep imports
    // anything assigned to the agent and not done into Plan.
    if let Err(error) = tracker.assign_to_self(&new_key).await {
        warn!(
            ?error,
            "Reopen: {new_key} filed but not assigned — it will sit in To Do"
        );
    }
    // The original depends on the follow-up.
    if let Err(error) = tracker.link_blocks(&new_key, jira_key).await {
        warn!(
            ?error,
            "Reopen: {new_key} filed but not linked to {jira_key}"
        );
    }

    // Watching is what makes the follow-up reach the people who care: the
    // announcement below lands on the original, which they may have stopped
    // reading once it closed, and nothing else subscribes them to the new key.
    let agent = tracker
        .identity()
        .await
        .ok()
        .map(|identity| identity.account_id);
    for account_id in followup_watchers(
        original.reporter_account_id.as_deref(),
        reports,
        agent.as_deref(),
    ) {
        if let Err(error) = tracker.add_watcher(&new_key, &account_id).await {
            warn!(
                ?error,
                "Reopen: {new_key} filed but {account_id} was not added as a watcher"
            );
        }
    }

    let new_url = tracker.issue_url(&new_key);
    let notice = format!(
        "Thanks — reopening this as {new_key}, which this ticket now depends on: \
         {new_url}\n\nI've picked it up and will work it the same way as \
         the original. Follow {new_key} for progress; this ticket stays linked so the history \
         reads end to end."
    );
    if let Err(error) = tracker.post_comment(jira_key, &notice).await {
        warn!(?error, "Reopen: could not announce {new_key} on {jira_key}");
    }

    // Recorded last and best-effort on the Jira side, first on the card: this
    // stamp is what stops the next sweep filing a duplicate, so it matters more
    // than any of the announcements above.
    // Parked, not closed: the original is the record of the incident and its
    // follow-up is the work. Recording where it was is what makes the park
    // reversible — the restore has to name a status, and "wherever it came
    // from" is three different answers to a reporter.
    let prior_status = match tracker.transition(jira_key, BLOCKED_STATUS).await {
        Ok(()) => original.status_name.clone(),
        Err(error) => {
            warn!(
                ?error,
                "Reopen: {jira_key} could not be parked in {BLOCKED_STATUS} — it stays where it is"
            );
            None
        }
    };

    let stamped = IssueExtMetadata {
        reopen: Some(ReopenState {
            followup_key: Some(new_key.clone()),
            filed_at: Some(Utc::now()),
            last_seen_comment_at: reports
                .iter()
                .filter_map(|comment| comment.created)
                .map(|created| created.with_timezone(&Utc))
                .max(),
            prior_status,
        }),
        ..Default::default()
    }
    .merge_into(&card.extension_metadata);
    // Deliberately not `?`: the ticket exists by now, and returning an error
    // would have the caller retry and file a second one for the same report.
    // Losing the stamp only costs the *next* report an append — it opens a
    // fresh ticket instead of joining this one — which is far cheaper than
    // duplicating on the spot.
    if let Err(error) = client
        .update_issue(
            card.id,
            &UpdateIssueRequest {
                status_id: None,
                title: None,
                description: None,
                priority: None,
                start_date: None,
                target_date: None,
                completed_at: None,
                sort_order: None,
                parent_issue_id: None,
                parent_issue_sort_order: None,
                extension_metadata: Some(stamped),
            },
        )
        .await
    {
        warn!(
            ?error,
            "Reopen: {new_key} filed but the follow-up key was not recorded on {jira_key} —              a later report on this ticket will open another rather than joining it"
        );
    }

    info!(
        "Reopen: filed {new_key} for {jira_key} ({})",
        trigger.describe()
    );
    Ok(Outcome::Filed(new_key))
}

/// The leading customer tag of a summary — `(Acme) `, `[ACME] ` — or empty.
///
/// Carried onto the follow-up because it is how these titles are scanned: the
/// customer is the first thing a TAM looks for and the last thing that should
/// go missing when the rest of the title changes.
pub(crate) fn leading_tag(summary: &str) -> &str {
    let summary = summary.trim_start();
    let close = match summary.chars().next() {
        Some('(') => ')',
        Some('[') => ']',
        _ => return "",
    };
    match summary.find(close) {
        // `+ 1` for the bracket; the space after it is included when present
        // so the caller can concatenate without thinking about it.
        Some(at) => {
            let end = at + close.len_utf8();
            let with_space = if summary[end..].starts_with(' ') {
                end + 1
            } else {
                end
            };
            &summary[..with_space]
        }
        None => "",
    }
}

/// The first sentence of `text`, never longer than `max` characters.
///
/// Char-safe: a summary clipped mid-character is a panic, and reporters write
/// in more than ASCII.
pub(crate) fn first_sentence_clipped(text: &str, max: usize) -> String {
    let text = text.trim();
    let sentence = match text.find(['.', '!', '?', '\n']) {
        Some(at) if at > 0 => &text[..at],
        _ => text,
    }
    .trim();
    if sentence.chars().count() <= max {
        return sentence.to_string();
    }
    let kept: String = sentence.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// The newest thing a reporter actually said, with the reopen marker removed.
///
/// `NOT FIXED` on its own line is the marker the triage playbook teaches; what
/// follows it is the part worth putting in a title.
fn reported_symptom(reports: &[TrackerComment]) -> Option<String> {
    let newest = reports.last()?;
    let body = newest.body_text.trim();
    let mut lines = body.lines().map(str::trim);
    let first = lines.next()?;
    let without_marker = first
        .strip_prefix("NOT FIXED")
        .or_else(|| first.strip_prefix("not fixed"))
        .map(|rest| rest.trim_start_matches([':', '-', '—', ' ']).trim());
    match without_marker {
        // Marker plus reasons on the same line.
        Some(rest) if !rest.is_empty() => Some(rest.to_string()),
        // Marker alone: the reasons are below it, if there are any.
        Some(_) => lines.find(|line| !line.is_empty()).map(str::to_string),
        // No marker at all — a status change rather than a comment.
        None if !first.is_empty() => Some(first.to_string()),
        None => None,
    }
}

/// A follow-up's title: what is still wrong, not a copy of the original's.
///
/// `Reopened: <original>` left three tickets reading the same. On BUG-10332 a
/// TAM told a customer a fix had shipped, from the wrong one of BUG-10044,
/// BUG-10332 and BUG-10333. That is the cost — not untidiness.
///
/// The date is the distinguisher of last resort, because a follow-up to a
/// follow-up must not collide either.
fn followup_summary(original: &str, symptom: Option<&str>, filed_on: chrono::NaiveDate) -> String {
    const MAX_SUMMARY: usize = 200;
    let tag = leading_tag(original);
    match symptom.map(str::trim).filter(|said| !said.is_empty()) {
        Some(said) => {
            let said = first_sentence_clipped(said, MAX_SUMMARY - tag.chars().count());
            // The reporter may have led with the tag themselves.
            if !tag.is_empty() && said.starts_with(tag.trim()) {
                said
            } else {
                format!("{tag}{said}")
            }
        }
        // Nothing was said — a status flipped back, or the comment was the
        // bare marker. Still has to be told apart from the original and from
        // any earlier follow-up.
        None => format!(
            "{} — still failing as of {}",
            original.trim().trim_end_matches('.'),
            filed_on.format("%-d %b %Y")
        ),
    }
}

fn format_reports(reports: &[TrackerComment]) -> String {
    reports
        .iter()
        .map(|comment| format!("- {}: {}", comment.author, comment.body_text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(body: &str) -> Vec<TrackerComment> {
        vec![TrackerComment {
            author: "Emelie".to_string(),
            author_account_id: None,
            mentions: Vec::new(),
            created: None,
            body_text: body.to_string(),
        }]
    }

    fn on(day: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    #[test]
    fn a_cause_makes_a_better_title_than_a_symptom() {
        // What part one produces at filing time, from the reporter's words.
        let filed = "(Acme) Org Name is still empty for accounts with no dashboard URL";
        // What the follow-up's own analysis concludes hours later.
        let client_summary = "(Acme) Org Name stays blank when an account has no \
                              dashboard URL, because the lookup gives up instead of \
                              falling back to the account record. Customers saw an \
                              empty field. The fix reads the account record when the \
                              dashboard URL is missing.";

        let tag = leading_tag(filed);
        let named = first_sentence_clipped(client_summary, 200 - tag.chars().count());
        let title = if named.starts_with(tag.trim()) {
            named
        } else {
            format!("{tag}{named}")
        };

        assert!(title.starts_with("(Acme) "));
        assert!(title.contains("because the lookup gives up"));
        // One sentence, not the whole summary.
        assert!(!title.contains("Customers saw an empty field"));
        assert!(title.chars().count() <= 200);
    }

    #[test]
    fn a_follow_up_is_named_for_what_is_still_wrong() {
        let original = "(Acme) Org Name input suddenly loads super slow";
        let reports = said("NOT FIXED\nOrg Name is still empty for accounts with no dashboard URL");

        assert_eq!(
            followup_summary(original, reported_symptom(&reports).as_deref(), on(22)),
            "(Acme) Org Name is still empty for accounts with no dashboard URL"
        );
    }

    #[test]
    fn the_marker_and_its_reasons_may_share_a_line() {
        let reports = said("NOT FIXED — the SFDC app still blocks on load");
        assert_eq!(
            reported_symptom(&reports).as_deref(),
            Some("the SFDC app still blocks on load")
        );
    }

    #[test]
    fn a_bare_marker_falls_back_to_something_distinguishable() {
        let original = "(Acme) Org Name input suddenly loads super slow";
        let bare = said("NOT FIXED");

        let first = followup_summary(original, reported_symptom(&bare).as_deref(), on(22));
        assert_eq!(
            first,
            "(Acme) Org Name input suddenly loads super slow — still failing as of 22 Sep 2026"
        );
        // A follow-up to a follow-up filed on another day must not collide
        // with this one either.
        let second = followup_summary(&first, None, on(29));
        assert_ne!(first, second);
        assert!(second.contains("29 Sep 2026"));
    }

    #[test]
    fn the_customer_tag_survives_but_is_never_doubled() {
        let original = "[ACME] exports time out";
        let with_tag = said("NOT FIXED\n[ACME] exports still time out over 50MB");
        assert_eq!(
            followup_summary(original, reported_symptom(&with_tag).as_deref(), on(22)),
            "[ACME] exports still time out over 50MB"
        );
        let without = said("NOT FIXED\nexports still time out over 50MB");
        assert_eq!(
            followup_summary(original, reported_symptom(&without).as_deref(), on(22)),
            "[ACME] exports still time out over 50MB"
        );
    }

    #[test]
    fn a_long_report_is_clipped_without_splitting_a_character() {
        let long = "é".repeat(400);
        let reports = said(&format!("NOT FIXED\n{long}"));
        let title = followup_summary("(x) y", reported_symptom(&reports).as_deref(), on(22));
        assert!(title.chars().count() <= 200, "{}", title.chars().count());
        assert!(title.ends_with('…'));
    }

    fn comment_from(account_id: Option<&str>) -> TrackerComment {
        TrackerComment {
            author: "Someone".to_string(),
            author_account_id: account_id.map(str::to_string),
            mentions: Vec::new(),
            created: None,
            body_text: "NOT FIXED".to_string(),
        }
    }

    #[test]
    fn the_reporter_is_watched_first_and_only_once() {
        let watchers = followup_watchers(
            Some("tam"),
            &[comment_from(Some("tam")), comment_from(Some("customer"))],
            Some("agent"),
        );
        assert_eq!(watchers, vec!["tam", "customer"]);
    }

    #[test]
    fn the_agent_never_watches_its_own_followup() {
        let watchers = followup_watchers(
            Some("agent"),
            &[comment_from(Some("agent")), comment_from(Some("tam"))],
            Some("agent"),
        );
        assert_eq!(watchers, vec!["tam"]);
    }

    #[test]
    fn an_author_jira_gave_no_account_id_is_skipped() {
        assert!(followup_watchers(None, &[comment_from(None)], None).is_empty());
    }

    #[test]
    fn the_marker_survives_how_people_actually_type_it() {
        for body in [
            "NOT FIXED",
            "not fixed",
            "Not Fixed — still seeing the same totals",
            "**NOT FIXED**",
            "> NOT FIXED",
            "  not fixed\n\nSame numbers as before.",
            "NOT FIXED: the export still drops the last row",
        ] {
            assert!(is_reopen_report(body), "should have matched: {body:?}");
        }
    }

    #[test]
    fn an_open_follow_up_with_no_new_report_says_nothing() {
        // The shape that put eight identical comments on BUG-10234: the status
        // trigger re-matches a reopened ticket every sweep and carries no
        // comments, because a status change is not a report.
        assert_eq!(
            existing_followup_action(false, false),
            ExistingFollowup::SayNothing
        );
        // A real second report still belongs on the open follow-up.
        assert_eq!(
            existing_followup_action(false, true),
            ExistingFollowup::Append
        );
        // A closed follow-up chains a new one either way — that attempt shipped
        // and also failed, which is new work whether or not a comment came with
        // it.
        assert_eq!(
            existing_followup_action(true, false),
            ExistingFollowup::Chain
        );
        assert_eq!(
            existing_followup_action(true, true),
            ExistingFollowup::Chain
        );
    }

    #[test]
    fn shipped_statuses_are_matched_by_name_not_category() {
        let statuses = crate::services::config::StatusMapping::default();
        for name in ["Merged", "deployed to production", "DONE", " Merged "] {
            assert!(statuses.is_shipped(name), "expected shipped: {name}");
        }
        // In Progress shares a status *category* with Merged and Deployed to
        // Production in this project, which is the whole reason this is a
        // name list rather than a category check.
        for name in ["In Progress", "To Do", "In Review"] {
            assert!(!statuses.is_shipped(name), "not shipped: {name}");
        }
    }

    #[test]
    fn a_follow_up_never_files_at_p0() {
        assert_eq!(followup_priority(Some("P0")), "P1");
        // Everything else carries across: coming back is not an escalation,
        // and it is not a demotion either.
        assert_eq!(followup_priority(Some("P1")), "P1");
        assert_eq!(followup_priority(Some("P2")), "P2");
        assert_eq!(followup_priority(Some("P3")), "P3");
        assert_eq!(followup_priority(None), "P2");
    }

    #[test]
    fn only_the_first_line_invokes_it() {
        // Discussing the convention is not using it, and a reply that buries
        // the marker under pleasantries has to fail loudly rather than
        // half-work — which is why the playbook tells reporters the rule.
        for body in [
            "Thanks! Sadly NOT FIXED, same behaviour.",
            "Confirmed working, thanks.",
            "",
            "The fix is not fixed in the sense that it regressed elsewhere",
        ] {
            assert!(!is_reopen_report(body), "should not have matched: {body:?}");
        }
    }
}
