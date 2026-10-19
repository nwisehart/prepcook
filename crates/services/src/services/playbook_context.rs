//! What a playbook's placeholders are filled with, and the filling. The
//! placeholders are listed in `docs/playbooks.md`; the texts are in
//! `config::playbooks`.
//!
//! `packages/web-core/src/shared/lib/stagePlaybooks.ts` renders the same way
//! for agents started from the board by hand; keep the two in step.

use std::collections::BTreeMap;

use db::models::repo::Repo;
use git::GitService;
use git_host::{GitHostProvider, GitHostService, ProviderKind};
use sqlx::SqlitePool;
use uuid::Uuid;

use api_types::WorkClass;

use crate::services::{
    config::{
        EvidenceSourceConfig, InAppGate, TargetOrigin,
        playbooks::{INVESTIGATION_GATE_VAR, gate_instructions, gate_instructions_for},
    },
    evidence::{self, SourceBrief},
    scripts::{load_manifest, manifest::ProductProfile},
    secrets,
};

/// The repository- and product-level values every playbook can use, resolved
/// once per launch from the board's first repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybookContext {
    /// `[profile] name`, else the repository's name.
    pub product_name: String,
    pub product_summary: String,
    pub product_areas: Vec<String>,
    pub product_error_patterns: Vec<String>,
    /// `[profile] evidence_tools`: the appendix to the sources' paragraphs.
    pub product_evidence_tools: String,
    /// The evidence sources the board's agents are told about, in order.
    pub evidence: Vec<SourceBrief>,
    /// The branch work is based on and merged into, as its remote-tracking
    /// ref when it has one (`origin/main`).
    pub target_branch: String,
    /// The author name the agent's commits carry.
    pub bot_name: String,
    /// The git host, which decides `{{host_cli}}` and `{{pr_noun}}`. `None`
    /// when the remote is not one Prepcook recognises.
    pub host: Option<ProviderKind>,
    pub in_app_gate: InAppGate,
}

/// What `{{product.name}}` says when there is not even a repository to name.
const UNNAMED_PRODUCT: &str = "this project";
/// The branch assumed when the repository names none and has no checkout.
const FALLBACK_TARGET_BRANCH: &str = "main";

impl PlaybookContext {
    pub fn new(
        repo_name: &str,
        profile: Option<&ProductProfile>,
        target_branch: String,
        bot_name: String,
        host: Option<ProviderKind>,
        in_app_gate: InAppGate,
    ) -> Self {
        let text = |value: Option<&String>| {
            value
                .map(|value| value.trim().to_string())
                .unwrap_or_default()
        };
        let list = |values: Option<&Vec<String>>| {
            values
                .into_iter()
                .flatten()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        };
        let named = text(profile.and_then(|profile| profile.name.as_ref()));
        Self {
            product_name: if named.is_empty() {
                repo_name.trim().to_string()
            } else {
                named
            },
            product_summary: text(profile.and_then(|profile| profile.summary.as_ref())),
            product_areas: list(profile.map(|profile| &profile.areas)),
            product_error_patterns: list(profile.map(|profile| &profile.error_patterns)),
            product_evidence_tools: text(
                profile.and_then(|profile| profile.evidence_tools.as_ref()),
            ),
            evidence: Vec::new(),
            target_branch,
            bot_name,
            host,
            in_app_gate,
        }
    }

    /// The context with these evidence sources described.
    pub fn with_evidence(mut self, evidence: Vec<SourceBrief>) -> Self {
        self.evidence = evidence;
        self
    }

    /// The command-line client for the git host. GitHub's `gh` is the
    /// fallback for a host Prepcook does not recognise.
    pub fn host_cli(&self) -> &'static str {
        match self.host {
            Some(ProviderKind::GitLab) => "glab",
            Some(ProviderKind::AzureDevOps) => "az repos",
            Some(ProviderKind::GitHub | ProviderKind::Unknown) | None => "gh",
        }
    }

    /// What the git host calls a change proposed for merging.
    pub fn pr_noun(&self) -> &'static str {
        match self.host {
            Some(ProviderKind::GitLab) => "merge request",
            _ => "pull request",
        }
    }

    /// Placeholder name (without braces) to value. `gate_instructions` is the
    /// raw gate text, which may itself use `{{spec_path}}`;
    /// `investigation_gate_instructions` is what replaces it for an
    /// investigation card, for pages that fill playbooks themselves.
    pub fn vars(&self) -> BTreeMap<String, String> {
        let product_name = if self.product_name.is_empty() {
            UNNAMED_PRODUCT.to_string()
        } else {
            self.product_name.clone()
        };
        BTreeMap::from([
            ("product.name".to_string(), product_name),
            ("product.summary".to_string(), self.product_summary.clone()),
            ("product.areas".to_string(), self.product_areas.join(", ")),
            (
                "product.error_patterns".to_string(),
                self.product_error_patterns.join(", "),
            ),
            (
                "product.evidence_tools".to_string(),
                evidence::render_evidence_tools(&self.evidence, &self.product_evidence_tools),
            ),
            (
                "product.evidence_sources".to_string(),
                evidence::render_evidence_sources(&self.evidence),
            ),
            ("target_branch".to_string(), self.target_branch.clone()),
            ("bot_name".to_string(), self.bot_name.clone()),
            ("host_cli".to_string(), self.host_cli().to_string()),
            ("pr_noun".to_string(), self.pr_noun().to_string()),
            (
                "gate_instructions".to_string(),
                gate_instructions(self.in_app_gate).to_string(),
            ),
            (
                INVESTIGATION_GATE_VAR.to_string(),
                gate_instructions_for(self.in_app_gate, Some(WorkClass::Investigation)).to_string(),
            ),
        ])
    }

    /// Fills a playbook: the card's values (`ticket_key`, `title`, …) and
    /// this context's.
    pub fn fill(&self, template: &str, card: BTreeMap<String, String>) -> String {
        let mut vars = self.vars();
        vars.extend(card);
        render_playbook(template, &vars)
    }

    /// [`Self::fill`] for a card of `work_class`, whose class may change what
    /// `{{gate_instructions}}` says (see `gate_instructions_for`).
    pub fn fill_for_class(
        &self,
        template: &str,
        mut card: BTreeMap<String, String>,
        work_class: Option<WorkClass>,
    ) -> String {
        card.insert(
            "gate_instructions".to_string(),
            gate_instructions_for(self.in_app_gate, work_class).to_string(),
        );
        self.fill(template, card)
    }
}

impl Default for PlaybookContext {
    fn default() -> Self {
        Self::new(
            "",
            None,
            FALLBACK_TARGET_BRANCH.to_string(),
            git::FALLBACK_AUTHOR_NAME.to_string(),
            None,
            InAppGate::default(),
        )
    }
}

/// The context for one repository: its profile from `.prepcook/scripts.toml` in
/// the checkout, its default target branch, its remote's host, the commit
/// identity configured for it, and the evidence sources `configured` for its
/// board plus the profile's own.
pub fn context_for_repo(
    git: &GitService,
    repo: &Repo,
    in_app_gate: InAppGate,
    configured: &[(TargetOrigin, EvidenceSourceConfig)],
) -> PlaybookContext {
    let branch = repo
        .default_target_branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .map(str::to_string)
        .or_else(|| git.get_current_branch(&repo.path).ok())
        .unwrap_or_else(|| FALLBACK_TARGET_BRANCH.to_string());
    let target_branch = git
        .base_branch_name(&repo.path, &branch)
        .unwrap_or_else(|_| branch.clone());
    let profile = load_manifest(&repo.path)
        .ok()
        .and_then(|manifest| manifest.profile);
    let host = git
        .resolve_remote_for_branch(&repo.path, &branch)
        .ok()
        .and_then(|remote| GitHostService::from_url(&remote.url).ok())
        .map(|host| host.provider_kind());
    let own = profile
        .as_ref()
        .map(|profile| profile.evidence.as_slice())
        .unwrap_or_default();
    let briefs = evidence_briefs(configured, own, Some(repo.path.clone()));
    PlaybookContext::new(
        &repo.name,
        profile.as_ref(),
        target_branch,
        git.commit_author_name(&repo.path),
        host,
        in_app_gate,
    )
    .with_evidence(briefs)
}

/// The usable sources among `configured` and a repository's `own`, as a
/// playbook describes them. A source missing a setting it needs is left out,
/// as it is from searches.
pub fn evidence_briefs(
    configured: &[(TargetOrigin, EvidenceSourceConfig)],
    own: &[EvidenceSourceConfig],
    root: Option<std::path::PathBuf>,
) -> Vec<SourceBrief> {
    let settings = evidence::resolve_sources(configured, own, &secrets::load());
    evidence::build(&settings, root).briefs()
}

/// The context for a board: its first repository that still exists, or the
/// defaults when it has none. `configured` is the board's evidence sources
/// (`Config::evidence_sources`).
pub async fn context_for_repos(
    pool: &SqlitePool,
    git: &GitService,
    repo_ids: &[Uuid],
    in_app_gate: InAppGate,
    configured: &[(TargetOrigin, EvidenceSourceConfig)],
) -> PlaybookContext {
    for repo_id in repo_ids {
        if let Ok(Some(repo)) = Repo::find_by_id(pool, *repo_id).await {
            return context_for_repo(git, &repo, in_app_gate, configured);
        }
    }
    PlaybookContext {
        in_app_gate,
        ..PlaybookContext::default()
    }
    .with_evidence(evidence_briefs(configured, &[], None))
}

/// Fills `template` from `vars`.
///
/// `{{#if name}}…{{/if}}` keeps its body only when `name`'s value is not
/// blank, and `{{#if name}}…{{else}}…{{/if}}` picks one of two; blocks do not
/// nest. `{{name}}` is replaced by its value; a name with no value is left as
/// written, so a later step (or a reader) can still see it. Inserted values
/// are not scanned again, except `gate_instructions`, which is filled from
/// the same values first because it refers to `{{spec_path}}`.
pub fn render_playbook(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut vars = vars.clone();
    if let Some(gate) = vars.get("gate_instructions").cloned() {
        let filled = substitute(&gate, &vars);
        vars.insert("gate_instructions".to_string(), filled);
    }
    substitute(&resolve_conditionals(template, &vars), &vars)
}

fn is_set(vars: &BTreeMap<String, String>, name: &str) -> bool {
    vars.get(name).is_some_and(|value| !value.trim().is_empty())
}

fn resolve_conditionals(template: &str, vars: &BTreeMap<String, String>) -> String {
    const OPEN: &str = "{{#if ";
    const ELSE: &str = "{{else}}";
    const CLOSE: &str = "{{/if}}";

    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find(OPEN) {
        let after_open = &rest[start + OPEN.len()..];
        let Some(name_end) = after_open.find("}}") else {
            break;
        };
        let body = &after_open[name_end + 2..];
        let Some(close) = body.find(CLOSE) else {
            break;
        };
        let name = after_open[..name_end].trim();
        let inner = &body[..close];
        let (then, otherwise) = inner.split_once(ELSE).unwrap_or((inner, ""));
        out.push_str(&rest[..start]);
        out.push_str(if is_set(vars, name) { then } else { otherwise });
        rest = &body[close + CLOSE.len()..];
    }
    out.push_str(rest);
    out
}

fn substitute(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) if is_placeholder_name(&after[..end]) => {
                let name = &after[..end];
                match vars.get(name) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str("{{");
                        out.push_str(name);
                        out.push_str("}}");
                    }
                }
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_placeholder_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::config::{
        COMMENT_REPLY_PLAYBOOK_KEY,
        playbooks::{INVESTIGATION_PLAYBOOK_KEY, PATTERN_PLAYBOOK_KEY, default_playbooks},
    };

    fn profile() -> ProductProfile {
        ProductProfile {
            name: Some("Pantry".to_string()),
            summary: Some("A shared grocery list.".to_string()),
            areas: vec!["Lists".to_string(), "Sharing".to_string()],
            error_patterns: vec!["SeedMissing".to_string()],
            evidence_tools: Some("The app logs to logs/app.log.".to_string()),
            evidence: Vec::new(),
        }
    }

    fn card(spec_path: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("ticket_key".to_string(), "KEY-7".to_string()),
            ("jira_key".to_string(), "KEY-7".to_string()),
            ("title".to_string(), "Items vanish".to_string()),
            ("description".to_string(), "They vanish.".to_string()),
            ("spec_path".to_string(), spec_path.to_string()),
            ("epic_key".to_string(), "KEY-1".to_string()),
        ])
    }

    fn full(gate: InAppGate) -> PlaybookContext {
        PlaybookContext::new(
            "pantry",
            Some(&profile()),
            "origin/main".to_string(),
            "Pantry Bot".to_string(),
            Some(ProviderKind::GitLab),
            gate,
        )
    }

    fn bare() -> PlaybookContext {
        PlaybookContext::new(
            "pantry",
            None,
            "origin/main".to_string(),
            "Prepcook".to_string(),
            None,
            InAppGate::None,
        )
    }

    #[test]
    fn a_profile_fills_the_product_placeholders() {
        let filled = full(InAppGate::Lavish).fill(
            "{{product.name}}: {{product.summary}} [{{product.areas}}] \
             [{{product.error_patterns}}] {{product.evidence_tools}} \
             {{bot_name}} {{host_cli}} {{pr_noun}} {{target_branch}} {{ticket_key}}",
            card("docs/specs/KEY-7.html"),
        );
        assert_eq!(
            filled,
            "Pantry: A shared grocery list. [Lists, Sharing] [SeedMissing] \
             The app logs to logs/app.log.\n\nIf none of these show the failure, the harness \
             is your evidence. Pantry Bot glab merge request origin/main KEY-7"
        );
    }

    #[test]
    fn without_a_profile_the_name_is_the_repository_and_blocks_drop() {
        let template = "The product is {{product.name}}.{{#if product.summary}} \
                        {{product.summary}}{{/if}}{{#if product.areas}} Areas: \
                        {{product.areas}}.{{/if}} {{#if product.evidence_tools}}Logs: \
                        {{product.evidence_tools}}{{else}}The harness is your \
                        evidence.{{/if}} Use {{host_cli}} for the {{pr_noun}}.";
        assert_eq!(
            bare().fill(template, card("x")),
            "The product is pantry. The harness is your evidence. Use gh for the pull request."
        );
        assert_eq!(
            full(InAppGate::None).fill(template, card("x")),
            "The product is Pantry. A shared grocery list. Areas: Lists, Sharing. \
             Logs: The app logs to logs/app.log.\n\nIf none of these show the failure, the \
             harness is your evidence. Use glab for the merge request."
        );
        assert_eq!(
            PlaybookContext::default().vars()["product.name"],
            UNNAMED_PRODUCT
        );
    }

    fn log_source(label: &str, paths: &[&str]) -> EvidenceSourceConfig {
        EvidenceSourceConfig {
            kind: evidence::EvidenceKind::LogFiles,
            label: Some(label.to_string()),
            fields: BTreeMap::from([("paths".to_string(), serde_json::json!(paths))]),
        }
    }

    #[test]
    fn configured_sources_are_described_with_the_profile_text_as_their_appendix() {
        let configured = [(
            TargetOrigin::Global,
            log_source("Worker log", &["logs/worker.log"]),
        )];
        let own = [log_source("App log", &["logs/app.log"])];
        let briefs = evidence_briefs(&configured, &own, None);
        let labels: Vec<&str> = briefs.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Worker log", "App log"]);
        let vars = full(InAppGate::None).with_evidence(briefs.clone()).vars();
        let tools = &vars["product.evidence_tools"];
        assert_eq!(
            tools,
            &format!(
                "{}\n\n{}\n\nThe app logs to logs/app.log.\n\n{}",
                briefs[0].describe,
                briefs[1].describe,
                evidence::HARNESS_FALLBACK
            )
        );
        assert_eq!(vars["product.evidence_sources"], "- Worker log\n- App log");

        let none = bare().vars();
        assert_eq!(none["product.evidence_tools"], "");
        assert_eq!(none["product.evidence_sources"], "");
    }

    /// The toolkit's generated skills fill `{{product.evidence_tools}}` from
    /// the Pantry manifest in Python (`toolkit/generator/generate.py`); this
    /// keeps that rendering identical to the orchestrator's.
    #[test]
    fn the_toolkit_skills_carry_the_orchestrators_evidence_text() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("docs/examples/scripts.toml");
        let manifest = crate::services::scripts::manifest::Manifest::parse(
            &std::fs::read_to_string(&path).unwrap(),
            &path,
        )
        .unwrap();
        let profile = manifest.profile.unwrap();
        let context = PlaybookContext::new(
            "pantry",
            Some(&profile),
            "origin/main".to_string(),
            "Prepcook".to_string(),
            None,
            InAppGate::None,
        )
        .with_evidence(evidence_briefs(&[], &profile.evidence, None));
        let tools = context.vars()["product.evidence_tools"].clone();
        assert!(
            tools.starts_with("Pantry app log (Log files): `logs/app.log`"),
            "{tools}"
        );
        let skill =
            std::fs::read_to_string(root.join("toolkit/skills/prepcook-rca/SKILL.md")).unwrap();
        assert!(
            skill.contains(&tools),
            "toolkit/skills/prepcook-rca/SKILL.md does not carry:\n{tools}\n\nRun \
             `python3 toolkit/generator/generate.py`, or bring `evidence_tools` in generate.py \
             back in line with `evidence::log_files::describe_paths`."
        );
    }

    #[test]
    fn the_lavish_gate_opens_a_review_session_on_the_spec() {
        let filled = full(InAppGate::Lavish).fill(
            "Finally: {{gate_instructions}}",
            card("docs/specs/KEY-7.html"),
        );
        assert!(
            filled.contains("npx lavish-axi docs/specs/KEY-7.html --no-open"),
            "{filled}"
        );
        assert!(filled.contains("window.lavish.sendQueuedPrompts()"));
        assert!(filled.contains("spec_approved = true"));
        assert!(!filled.contains("{{"), "{filled}");
    }

    #[test]
    fn without_an_in_app_gate_the_decision_comes_as_a_follow_up() {
        let filled = full(InAppGate::None).fill(
            "Finally: {{gate_instructions}}",
            card("docs/specs/KEY-7.html"),
        );
        assert!(!filled.to_lowercase().contains("lavish"), "{filled}");
        assert!(filled.contains("approvals API"));
        assert!(filled.contains("END YOUR TURN"));
        assert!(filled.contains("spec_approved = true"));
    }

    /// An investigation card has no review gate in either gate setting: its
    /// report marks it triaged.
    #[test]
    fn an_investigation_is_told_its_report_marks_it_triaged() {
        for gate in [InAppGate::Lavish, InAppGate::None] {
            let filled = full(gate).fill_for_class(
                &default_playbooks()[INVESTIGATION_PLAYBOOK_KEY],
                card("docs/specs/KEY-7.html"),
                Some(WorkClass::Investigation),
            );
            assert!(filled.contains("marks the card triaged"), "{filled}");
            assert!(filled.contains("TRIAGE REPORT"));
            assert!(!filled.to_lowercase().contains("lavish"), "{filled}");
            assert!(!filled.contains("Proposed fix"), "{filled}");
            assert!(!filled.contains("{{"), "{filled}");
        }
        // Every other class keeps the board's gate.
        let bug = full(InAppGate::Lavish).fill_for_class(
            "{{gate_instructions}}",
            card("docs/specs/KEY-7.html"),
            Some(WorkClass::Bug),
        );
        assert!(bug.contains("lavish-axi"), "{bug}");
        assert_eq!(
            full(InAppGate::Lavish).vars()[INVESTIGATION_GATE_VAR],
            gate_instructions_for(InAppGate::None, Some(WorkClass::Investigation))
        );
    }

    #[test]
    fn unknown_placeholders_stay_and_values_are_not_rescanned() {
        let mut vars = BTreeMap::new();
        vars.insert("title".to_string(), "uses {{ticket_key}}".to_string());
        assert_eq!(
            render_playbook("{{title}} {{comments}} {{ not a name }}", &vars),
            "uses {{ticket_key}} {{comments}} {{ not a name }}"
        );
        // An unclosed block is left as written rather than eating the rest.
        assert_eq!(
            render_playbook("a {{#if title}} b", &vars),
            "a {{#if title}} b"
        );
    }

    #[test]
    fn every_default_playbook_fills_completely_in_both_contexts() {
        for context in [
            full(InAppGate::Lavish),
            full(InAppGate::None),
            bare(),
            PlaybookContext::default(),
        ] {
            for (key, template) in default_playbooks() {
                let filled = context.fill(&template, card("docs/specs/KEY-7.html"));
                // The comment-reply agent's comments are filled by its launcher.
                let filled = if key == COMMENT_REPLY_PLAYBOOK_KEY {
                    filled.replace("{{comments}}", "")
                } else {
                    filled
                };
                assert!(!filled.contains("{{"), "{key}: {filled}");
                assert!(!filled.contains("}}"), "{key}: {filled}");
                for gap in [": .", "  ", " ,", " .\n", ". .", "— —"] {
                    assert!(!filled.contains(gap), "{key} contains {gap:?}: {filled}");
                }
            }
        }
    }

    #[test]
    fn every_planning_playbook_uses_the_gate() {
        for key in [
            "bug:planning",
            "story:planning",
            "epic:planning",
            "subtask:planning",
            INVESTIGATION_PLAYBOOK_KEY,
            PATTERN_PLAYBOOK_KEY,
        ] {
            let template = &default_playbooks()[key];
            assert!(template.contains("{{gate_instructions}}"), "{key}");
            assert!(!template.contains("lavish"), "{key}");
        }
    }
}
