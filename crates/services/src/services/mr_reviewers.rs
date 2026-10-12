//! Blame-based reviewer selection for orchestrator-created MRs.
//!
//! The reviewer signal is the *pre-change* ownership of the lines an MR
//! touches: for every hunk we blame the old-side line range at the merge
//! base and count lines per author. The top humans (bot/agent emails
//! excluded) become reviewer candidates. Candidates whose email appears in
//! the configured email→login map are assigned on the MR; the rest are named
//! in an MR note so a human can assign them (neither GitHub nor GitLab
//! resolves a user by email for a token without admin scope).
//!
//! Pure parsing/selection lives here (unit-tested); git and git-host calls
//! stay in the orchestrator, which owns the worktree and provider context.

use std::collections::BTreeMap;

/// One old-side line range an MR modifies: `(file at the merge base, start
/// line, line count)`. Pure additions (count 0) carry no blame and are
/// skipped.
pub type OldRange = (String, u32, u32);

/// Parses `git diff --unified=0 <base>...HEAD` output into old-side ranges.
pub fn changed_old_ranges(diff_text: &str) -> Vec<OldRange> {
    let mut ranges = Vec::new();
    let mut current_file: Option<String> = None;
    for line in diff_text.lines() {
        if let Some(path) = line.strip_prefix("--- a/") {
            current_file = Some(path.to_string());
        } else if line.starts_with("--- /dev/null") {
            // New file: nothing to blame on the old side.
            current_file = None;
        } else if let Some(rest) = line.strip_prefix("@@ -") {
            let Some(file) = current_file.clone() else {
                continue;
            };
            // "@@ -start[,count] +.. @@"
            let old_part = rest.split(' ').next().unwrap_or("");
            let (start_str, count_str) = match old_part.split_once(',') {
                Some((start, count)) => (start, count),
                None => (old_part, "1"),
            };
            let (Ok(start), Ok(count)) = (start_str.parse::<u32>(), count_str.parse::<u32>())
            else {
                continue;
            };
            if count == 0 || start == 0 {
                continue; // pure addition
            }
            ranges.push((file, start, count));
        }
    }
    ranges
}

/// Accumulates `git blame --line-porcelain` output into per-author line
/// counts, keyed by lowercased email. Returns `email -> (display name,
/// lines)`.
pub fn accumulate_blame(porcelain: &str, counts: &mut BTreeMap<String, (String, usize)>) {
    let mut current_author: Option<String> = None;
    for line in porcelain.lines() {
        if let Some(name) = line.strip_prefix("author ") {
            current_author = Some(name.to_string());
        } else if let Some(mail) = line.strip_prefix("author-mail ") {
            let email = mail.trim().trim_matches(['<', '>']).to_ascii_lowercase();
            let name = current_author.clone().unwrap_or_else(|| email.clone());
            let entry = counts.entry(email).or_insert_with(|| (name.clone(), 0));
            entry.1 += 1;
        }
    }
}

/// A reviewer candidate: pre-change owner of a share of the touched lines.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewerCandidate {
    pub name: String,
    pub email: String,
    pub lines: usize,
    /// Share of all counted (non-excluded) lines, 0.0–1.0.
    pub share: f64,
}

impl ReviewerCandidate {
    /// The share as a whole percentage, for saying so to a human.
    pub fn percent(&self) -> u32 {
        (self.share * 100.0).round() as u32
    }
}

/// One reviewer to assign, and everyone else blame pointed at.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewerChoice {
    /// The host login to assign, with the candidate it came from.
    pub assigned: Option<(String, ReviewerCandidate)>,
    /// The rest, in rank order, each with its mapped username if it has one.
    pub others: Vec<(Option<String>, ReviewerCandidate)>,
}

/// The single reviewer to assign: the highest-ranked candidate who is in the
/// reviewer map.
///
/// Two assignees is a review each of them can reasonably assume the other is
/// doing. One is an obligation. But "the top candidate" has to mean the top
/// candidate who can actually be assigned — the host cannot resolve a user by
/// email without admin scope, so an unmapped candidate is not assignable at
/// all, and stopping at the first name would hand back no reviewer whenever
/// the person who wrote most of the touched code has not been mapped yet.
/// That failure is invisible: a merge request with no reviewer looks exactly
/// like one nobody has picked up.
///
/// Map keys are matched without regard to case. `accumulate_blame` lowercases
/// the emails it reads from git; a map entry typed as `Ada@Example.com` would
/// otherwise never match, and would fail as silently as the above.
pub fn choose_reviewer(
    picks: &[ReviewerCandidate],
    map: &BTreeMap<String, String>,
) -> ReviewerChoice {
    let lookup = |email: &str| -> Option<String> {
        map.iter()
            .find(|(key, _)| key.trim().eq_ignore_ascii_case(email))
            .map(|(_, username)| username.trim().to_string())
            .filter(|username| !username.is_empty())
    };
    let mut assigned = None;
    let mut others = Vec::new();
    for pick in picks {
        match lookup(&pick.email) {
            Some(username) if assigned.is_none() => {
                assigned = Some((username, pick.clone()));
            }
            username => others.push((username, pick.clone())),
        }
    }
    ReviewerChoice { assigned, others }
}

/// True for emails that must never be picked: the agent's own bot identity
/// (`<id>-<user>@users.noreply.gitlab.com`), GitHub apps committing as
/// `<id>+<app>[bot]@users.noreply.github.com`, the shared agent Jira account,
/// and anything the config excludes. People's own GitHub noreply addresses
/// stay eligible — many developers commit with one.
pub fn is_excluded_email(email: &str, extra_excludes: &[String]) -> bool {
    let email = email.to_ascii_lowercase();
    email.ends_with("users.noreply.gitlab.com")
        || email.ends_with("[bot]@users.noreply.github.com")
        || email == "engineering@example.com"
        || email == "not.committed.yet"
        || extra_excludes
            .iter()
            .any(|excluded| excluded.trim().eq_ignore_ascii_case(&email))
}

/// Top `max` candidates by blamed lines, bots/excluded filtered out.
pub fn pick_candidates(
    counts: &BTreeMap<String, (String, usize)>,
    extra_excludes: &[String],
    max: usize,
) -> Vec<ReviewerCandidate> {
    let mut humans: Vec<(&String, &(String, usize))> = counts
        .iter()
        .filter(|(email, _)| !is_excluded_email(email, extra_excludes))
        .collect();
    let total: usize = humans.iter().map(|(_, (_, lines))| lines).sum();
    if total == 0 {
        return Vec::new();
    }
    humans.sort_by(|a, b| b.1.1.cmp(&a.1.1).then_with(|| a.0.cmp(b.0)));
    humans
        .into_iter()
        .take(max)
        .map(|(email, (name, lines))| ReviewerCandidate {
            name: name.clone(),
            email: email.clone(),
            lines: *lines,
            share: *lines as f64 / total as f64,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n\
index 111..222 100644\n\
--- a/src/lib.rs\n\
+++ b/src/lib.rs\n\
@@ -10,3 +10,4 @@ fn ctx() {\n\
-old\n\
+new\n\
@@ -40 +42,0 @@\n\
-gone\n\
diff --git a/new.rs b/new.rs\n\
--- /dev/null\n\
+++ b/new.rs\n\
@@ -0,0 +1,5 @@\n\
+added\n";

    #[test]
    fn parses_old_ranges_and_skips_new_files() {
        let ranges = changed_old_ranges(DIFF);
        assert_eq!(
            ranges,
            vec![
                ("src/lib.rs".to_string(), 10, 3),
                ("src/lib.rs".to_string(), 40, 1),
            ]
        );
    }

    #[test]
    fn blame_counts_accumulate_by_email() {
        let porcelain = "abc 1 1 1\nauthor Ada\nauthor-mail <ada@example.com>\n\
                         filler\nabc 2 2 1\nauthor Ada\nauthor-mail <ADA@example.com>\n\
                         def 3 3 1\nauthor Bot\nauthor-mail <123-bot@users.noreply.gitlab.com>\n";
        let mut counts = BTreeMap::new();
        accumulate_blame(porcelain, &mut counts);
        assert_eq!(counts.get("ada@example.com").unwrap().1, 2);
        assert_eq!(counts.get("123-bot@users.noreply.gitlab.com").unwrap().1, 1);
    }

    #[test]
    fn picks_top_humans_and_computes_share() {
        let mut counts = BTreeMap::new();
        counts.insert("ada@example.com".into(), ("Ada".into(), 6));
        counts.insert("bob@example.com".into(), ("Bob".into(), 3));
        counts.insert("cam@example.com".into(), ("Cam".into(), 1));
        counts.insert(
            "42-agent@users.noreply.gitlab.com".into(),
            ("Agent".into(), 90),
        );
        counts.insert("engineering@example.com".into(), ("Agent Jira".into(), 50));
        let picks = pick_candidates(&counts, &[], 2);
        assert_eq!(picks.len(), 2);
        assert_eq!(picks[0].email, "ada@example.com");
        assert!((picks[0].share - 0.6).abs() < 1e-9);
        assert_eq!(picks[1].email, "bob@example.com");
    }

    fn candidate(email: &str, lines: usize) -> ReviewerCandidate {
        ReviewerCandidate {
            name: email.split('@').next().unwrap().to_string(),
            email: email.to_string(),
            lines,
            share: lines as f64 / 10.0,
        }
    }

    #[test]
    fn assigns_the_top_candidate_who_is_in_the_map() {
        let picks = [
            candidate("ada@example.com", 6),
            candidate("bob@example.com", 3),
            candidate("cam@example.com", 1),
        ];
        let map = BTreeMap::from([
            ("bob@example.com".to_string(), "bobby".to_string()),
            ("cam@example.com".to_string(), "cammy".to_string()),
        ]);

        let choice = choose_reviewer(&picks, &map);

        // Ada wrote the most of it and cannot be assigned, so the reviewer is
        // Bob — not nobody, which is what taking only the top name would give.
        assert_eq!(
            choice
                .assigned
                .as_ref()
                .map(|(username, _)| username.as_str()),
            Some("bobby")
        );
        // Exactly one is assigned even though Cam maps too.
        assert_eq!(choice.others.len(), 2);
        assert_eq!(choice.others[0].0, None);
        assert_eq!(choice.others[0].1.email, "ada@example.com");
        assert_eq!(choice.others[1].0.as_deref(), Some("cammy"));
    }

    #[test]
    fn nobody_in_the_map_assigns_nobody_rather_than_a_stranger() {
        let picks = [
            candidate("ada@example.com", 6),
            candidate("bob@example.com", 3),
        ];
        let choice = choose_reviewer(&picks, &BTreeMap::new());
        assert!(choice.assigned.is_none());
        // Still reported, so the MR note can say who to add to the map.
        assert_eq!(choice.others.len(), 2);
    }

    #[test]
    fn map_keys_match_regardless_of_case_and_padding() {
        let picks = [candidate("ada@example.com", 6)];
        let map = BTreeMap::from([(" Ada@Example.COM ".to_string(), " ada ".to_string())]);
        let choice = choose_reviewer(&picks, &map);
        assert_eq!(
            choice.assigned.map(|(username, _)| username),
            Some("ada".to_string())
        );
    }

    #[test]
    fn a_blank_username_is_not_an_assignment() {
        let picks = [
            candidate("ada@example.com", 6),
            candidate("bob@example.com", 3),
        ];
        let map = BTreeMap::from([
            ("ada@example.com".to_string(), "  ".to_string()),
            ("bob@example.com".to_string(), "bobby".to_string()),
        ]);
        let choice = choose_reviewer(&picks, &map);
        // A blank value would have been passed to the host's reviewer call and failed
        // the whole call, losing Bob with it.
        assert_eq!(
            choice.assigned.map(|(username, _)| username),
            Some("bobby".to_string())
        );
    }

    #[test]
    fn github_apps_are_excluded_but_github_people_are_not() {
        assert!(is_excluded_email(
            "41898282+github-actions[bot]@users.noreply.github.com",
            &[]
        ));
        assert!(!is_excluded_email("1234+ada@users.noreply.github.com", &[]));
    }

    #[test]
    fn config_excludes_apply_case_insensitively() {
        assert!(is_excluded_email(
            "Contractor@Ext.io",
            &["contractor@ext.io".to_string()]
        ));
        assert!(!is_excluded_email("dev@example.com", &[]));
    }
}
