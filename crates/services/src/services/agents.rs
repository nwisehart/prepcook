//! Which coding agent runs which orchestrator stage, and the `agents`
//! integration area that lists every agent Prepcook can drive.
//!
//! A stage's profile resolves, most specific first: the board's
//! `agents.stages.<stage>`, the global `agents.stages.<stage>`, then the
//! stage's default, which is built from the board's `executor_profile` (else
//! the global one). Planning runs that profile as it is; every other stage
//! takes the agent's economy variant when the profile names no variant (see
//! [`economy_variant_of`]).
//!
//! A card filed from outside the team plans under `agents.external` instead
//! (the board's, then the global one), which defaults to the planning
//! profile's agent with its guarded variant ([`guarded_variant_of`]): the
//! agent's own tools narrowed, never the shell confined. Executing and
//! Reviewing are unaffected.
//!
//! Every session the orchestrator launches or resumes runs under
//! `agents.permissions` ([`executor_config`]): the owner's choice of whether
//! agents ask before they use a tool, unless the profile's own variant sets a
//! policy of its own.

use api_types::IssueExtMetadata;
use executors::{
    descriptor::{self as agent_descriptor, economy_variant_of, guarded_variant_of},
    executors::BaseCodingAgent,
    profile::{ExecutorConfig, ExecutorConfigs, ExecutorProfileId},
};
use utils::approvals::APPROVAL_TIMEOUT_SECONDS;

use crate::services::{
    config::{AgentPermissions, AgentsConfig, StageProfiles},
    integrations::{Descriptor, Field, FieldKind, SelectOption},
};

/// The integration area name the descriptors route keys the agents under.
pub const AREA: &str = "agents";

/// An orchestrator stage that starts an agent session of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentStage {
    /// Specs, RCAs and investigations, and the follow-ups that revise them.
    Planning,
    /// The change itself, and the fix rounds, CI fixes, rebases and MR
    /// descriptions that run in its worktree.
    Executing,
    /// The independent review, in a fresh worktree.
    Reviewing,
    /// Answers to comments on shipped tickets.
    CommentReply,
    /// Pattern reviews, and the RCA-graph analysis that finds the patterns.
    Pattern,
}

impl AgentStage {
    pub const ALL: [AgentStage; 5] = [
        AgentStage::Planning,
        AgentStage::Executing,
        AgentStage::Reviewing,
        AgentStage::CommentReply,
        AgentStage::Pattern,
    ];

    /// The key under `agents.stages`.
    pub fn key(self) -> &'static str {
        match self {
            AgentStage::Planning => "planning",
            AgentStage::Executing => "executing",
            AgentStage::Reviewing => "reviewing",
            AgentStage::CommentReply => "comment_reply",
            AgentStage::Pattern => "pattern",
        }
    }

    /// The profile this stage runs when no `agents.stages` entry names one.
    ///
    /// Planning runs the default profile as it is: it is where the design
    /// judgment is. Every other stage runs the cheaper variant. Executing is
    /// many sessions (the change, every fix round, CI fixes, rebases, the MR
    /// description) carrying out a plan a person already approved; a review
    /// is a fresh full-diff session per round; comment replies and the RCA
    /// graph's analysis are frequent and analyze-only. Together they spend
    /// most of the seat's session budget. An explicit variant on the default
    /// profile wins.
    pub fn default_profile(self, default: &ExecutorProfileId) -> ExecutorProfileId {
        match self {
            AgentStage::Planning => default.clone(),
            _ => economy_variant_of(default),
        }
    }
}

impl StageProfiles {
    /// This stage's explicit profile, if one is set.
    pub fn get(&self, stage: AgentStage) -> Option<&ExecutorProfileId> {
        match stage {
            AgentStage::Planning => self.planning.as_ref(),
            AgentStage::Executing => self.executing.as_ref(),
            AgentStage::Reviewing => self.reviewing.as_ref(),
            AgentStage::CommentReply => self.comment_reply.as_ref(),
            AgentStage::Pattern => self.pattern.as_ref(),
        }
    }
}

/// Every stage's profile for one board, resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageAgents {
    pub planning: ExecutorProfileId,
    pub executing: ExecutorProfileId,
    pub reviewing: ExecutorProfileId,
    pub comment_reply: ExecutorProfileId,
    pub pattern: ExecutorProfileId,
    /// Planning for a card filed from outside the team.
    pub external: ExecutorProfileId,
    /// The permission policy every session runs under (global only; see
    /// [`AgentsConfig::permissions`]).
    pub permissions: AgentPermissions,
}

impl StageAgents {
    /// Resolves each stage: the board's override, then the global one, then
    /// the stage's default built from `default` (the board's default profile,
    /// already resolved against the global one). `external` resolves the same
    /// way, its default built from the resolved planning profile.
    pub fn resolve(
        board: &AgentsConfig,
        global: &AgentsConfig,
        default: &ExecutorProfileId,
    ) -> Self {
        let resolve = |stage: AgentStage| {
            board
                .stages
                .get(stage)
                .or_else(|| global.stages.get(stage))
                .cloned()
                .unwrap_or_else(|| stage.default_profile(default))
        };
        let planning = resolve(AgentStage::Planning);
        let external = board
            .external
            .as_ref()
            .or(global.external.as_ref())
            .cloned()
            .unwrap_or_else(|| guarded_variant_of(&planning));
        Self {
            planning,
            executing: resolve(AgentStage::Executing),
            reviewing: resolve(AgentStage::Reviewing),
            comment_reply: resolve(AgentStage::CommentReply),
            pattern: resolve(AgentStage::Pattern),
            external,
            permissions: global.permissions.unwrap_or_default(),
        }
    }

    /// The profile a launch of `stage` runs for this card: the stage's own,
    /// except Planning for a card filed from outside the team, which runs
    /// [`Self::external`].
    pub fn for_launch(&self, stage: AgentStage, card: &IssueExtMetadata) -> &ExecutorProfileId {
        if stage == AgentStage::Planning && card.is_filed_externally() {
            &self.external
        } else {
            self.get(stage)
        }
    }

    /// Whether this launch runs a profile other than the stage's own because
    /// the card was filed from outside the team, which the card then records.
    pub fn plans_guarded(&self, stage: AgentStage, card: &IssueExtMetadata) -> bool {
        self.for_launch(stage, card) != self.get(stage)
    }

    pub fn get(&self, stage: AgentStage) -> &ExecutorProfileId {
        match stage {
            AgentStage::Planning => &self.planning,
            AgentStage::Executing => &self.executing,
            AgentStage::Reviewing => &self.reviewing,
            AgentStage::CommentReply => &self.comment_reply,
            AgentStage::Pattern => &self.pattern,
        }
    }
}

/// The executor config a session of `profile` launches or resumes with: the
/// profile, under the owner's permission setting wherever the agent can be
/// supervised and the variant sets no policy of its own (see
/// [`ExecutorConfigs::launch_permission_policy`]).
pub fn executor_config(
    profile: ExecutorProfileId,
    permissions: AgentPermissions,
) -> ExecutorConfig {
    executor_config_with(&ExecutorConfigs::get_cached(), profile, permissions)
}

/// [`executor_config`] against an explicit set of profiles.
pub fn executor_config_with(
    profiles: &ExecutorConfigs,
    profile: ExecutorProfileId,
    permissions: AgentPermissions,
) -> ExecutorConfig {
    let permission_policy = profiles.launch_permission_policy(&profile, permissions.into());
    ExecutorConfig {
        permission_policy,
        ..profile.into()
    }
}

/// The `kind` of the agents area's one descriptor that is not an agent: the
/// `agents` section's own settings, rendered as fields.
pub const SETTINGS_KIND: &str = "settings";

/// Every coding agent as an integration descriptor (no fields to configure,
/// the agent's own description under `agent`), then the `agents` section's
/// own settings as [`SETTINGS_KIND`].
pub fn descriptors() -> Vec<Descriptor> {
    let mut all: Vec<Descriptor> = agent_descriptor::descriptors()
        .into_iter()
        .map(|agent| Descriptor {
            kind: agent.executor.to_string(),
            display_name: agent.display_name.clone(),
            summary: agent.summary.clone(),
            fields: Vec::new(),
            secrets: Vec::new(),
            docs_url: agent.docs_url.clone(),
            available: true,
            agent: Some(agent),
        })
        .collect();
    all.push(settings_descriptor());
    all
}

/// The `agents` section's settings beyond the per-stage profiles, each a key
/// of `agents` (and of a board's `agents`).
pub fn settings_descriptor() -> Descriptor {
    Descriptor::new(
        SETTINGS_KIND,
        "Agent settings",
        "How the orchestrator picks a profile beyond the per-stage ones.",
        vec![
            Field::new(
                "permissions",
                "Permissions",
                FieldKind::Select {
                    options: vec![
                        SelectOption::new("auto", "Auto — agents run without asking"),
                        SelectOption::new(
                            "supervised",
                            "Supervised — tool permission requests come to you",
                        ),
                    ],
                },
                false,
                &permissions_help(&agent_descriptor::descriptors()),
            ),
            Field::new(
                "external",
                "Externally filed cards",
                FieldKind::Profile,
                false,
                &external_help(&agent_descriptor::descriptors()),
            ),
        ],
        "https://github.com/nwisehart/prepcook/blob/main/docs/configuration.md#agents",
    )
}

/// The `permissions` field's help: what each policy does, which agents honour
/// it, and what the others do instead.
fn permissions_help(agents: &[agent_descriptor::AgentDescriptor]) -> String {
    let supervised: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.supervised)
        .map(|agent| agent.display_name.as_str())
        .collect();
    let unsupervised: Vec<&str> = agents
        .iter()
        .filter(|agent| !agent.supervised)
        .map(|agent| agent.display_name.as_str())
        .collect();
    let hours = APPROVAL_TIMEOUT_SECONDS / 3600;
    let mut help = format!(
        "Whether the agents the orchestrator runs ask before they use a tool, on every board. \
         Supervised sends each tool permission request to you as an approval on the card's \
         session; one nobody answers within {hours} hours is refused. \
         A profile whose variant sets its own policy keeps it."
    );
    if !supervised.is_empty() {
        help.push_str(&format!(" Honoured by {}.", names(&supervised)));
    }
    if !unsupervised.is_empty() {
        help.push_str(&format!(
            " {} cannot route a request to you and run as their profile says under either.",
            names(&unsupervised)
        ));
    }
    help
}

/// The `external` field's help: what it is for, and what each agent falls
/// back to when it is unset, from whether the agent ships a guarded variant.
fn external_help(agents: &[agent_descriptor::AgentDescriptor]) -> String {
    let guarded: Vec<String> = agents
        .iter()
        .filter_map(|agent| {
            agent
                .guarded_variant
                .as_ref()
                .map(|variant| format!("{} (`{variant}`)", agent.display_name))
        })
        .collect();
    let unguarded: Vec<&str> = agents
        .iter()
        .filter(|agent| agent.guarded_variant.is_none())
        .map(|agent| agent.display_name.as_str())
        .collect();
    let mut help = "Planning profile for cards filed from outside the team (API, public issues). \
                    Unset, the planning profile's agent with its guarded variant, which narrows \
                    the agent's own tools and does not confine its shell"
        .to_string();
    if !guarded.is_empty() {
        help.push_str(&format!(": {}", names(&guarded)));
    }
    help.push('.');
    if !unguarded.is_empty() {
        help.push_str(&format!(
            " {} can express nothing comparable and plan these cards under the planning profile \
             unchanged.",
            names(&unguarded)
        ));
    }
    help
}

/// "A", "A and B", "A, B and C".
fn names<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
    }
}

/// The Test button: the agent's version probe on this machine.
pub async fn test_agent(kind: &str) -> Result<String, String> {
    let executor: BaseCodingAgent = kind
        .parse()
        .map_err(|_| format!("unknown coding agent: {kind}"))?;
    let outcome = agent_descriptor::probe(executor).await;
    if outcome.ok {
        Ok(outcome.message)
    } else {
        Err(outcome.message)
    }
}

#[cfg(test)]
mod tests {
    use executors::model_selector::PermissionPolicy;

    use super::*;
    use crate::services::config::{Config, ProjectAutomationDefaults};

    fn profile(executor: BaseCodingAgent, variant: Option<&str>) -> ExecutorProfileId {
        ExecutorProfileId {
            executor,
            variant: variant.map(str::to_string),
        }
    }

    fn claude(variant: Option<&str>) -> ExecutorProfileId {
        profile(BaseCodingAgent::ClaudeCode, variant)
    }

    #[test]
    fn a_fresh_config_runs_planning_on_the_default_and_the_rest_on_the_economy_variant() {
        let config = Config::default();
        let settings = config.project_settings(uuid::Uuid::new_v4());
        let agents = &settings.agents;
        assert_eq!(agents.planning, claude(None));
        assert_eq!(agents.executing, claude(Some("SONNET")));
        assert_eq!(agents.reviewing, claude(Some("SONNET")));
        assert_eq!(agents.comment_reply, claude(Some("SONNET")));
        assert_eq!(agents.pattern, claude(Some("SONNET")));
    }

    #[test]
    fn the_economy_defaults_follow_the_default_profile() {
        for stage in AgentStage::ALL {
            // A variant chosen on the default profile is kept, not replaced.
            let chosen = claude(Some("DEFAULT"));
            assert_eq!(stage.default_profile(&chosen), chosen, "{stage:?}");
            // An agent without an economy variant runs as configured.
            let codex = profile(BaseCodingAgent::Codex, None);
            assert_eq!(stage.default_profile(&codex), codex, "{stage:?}");
        }
        assert_eq!(
            AgentStage::Planning.default_profile(&claude(None)),
            claude(None)
        );
    }

    #[test]
    fn stages_resolve_board_override_then_global_override_then_default() {
        let mut config = Config {
            executor_profile: profile(BaseCodingAgent::Codex, None),
            ..Default::default()
        };
        config.agents.stages.reviewing = Some(profile(BaseCodingAgent::Gemini, None));
        config.agents.stages.pattern = Some(claude(Some("SONNET")));

        let board_id = uuid::Uuid::new_v4();
        let board = ProjectAutomationDefaults {
            executor_profile: Some(claude(None)),
            agents: AgentsConfig {
                stages: StageProfiles {
                    pattern: Some(profile(BaseCodingAgent::Amp, None)),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        config
            .orchestrator
            .project_defaults
            .insert(board_id.to_string(), board);

        // A board with no overrides: the global stage override, else the
        // global default.
        let other = config.project_settings(uuid::Uuid::new_v4()).agents;
        assert_eq!(other.planning, profile(BaseCodingAgent::Codex, None));
        assert_eq!(other.executing, profile(BaseCodingAgent::Codex, None));
        assert_eq!(other.reviewing, profile(BaseCodingAgent::Gemini, None));
        assert_eq!(other.pattern, claude(Some("SONNET")));

        // The board's default replaces the global default, a global stage
        // override still beats it, and the board's own stage override beats
        // both.
        let agents = config.project_settings(board_id).agents;
        assert_eq!(agents.planning, claude(None));
        assert_eq!(agents.executing, claude(Some("SONNET")));
        assert_eq!(agents.comment_reply, claude(Some("SONNET")));
        assert_eq!(agents.reviewing, profile(BaseCodingAgent::Gemini, None));
        assert_eq!(agents.pattern, profile(BaseCodingAgent::Amp, None));
        for stage in AgentStage::ALL {
            assert_eq!(
                agents.get(stage),
                StageAgents::resolve(
                    &config.orchestrator.project_defaults[&board_id.to_string()].agents,
                    &config.agents,
                    &claude(None),
                )
                .get(stage)
            );
        }
    }

    #[test]
    fn stage_overrides_round_trip_through_the_config_file() {
        let raw = serde_json::json!({
            "stages": {
                "executing": { "executor": "CLAUDE_CODE", "variant": "DEFAULT" },
                "comment_reply": { "executor": "CODEX" }
            }
        });
        let parsed: AgentsConfig = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.stages.executing, Some(claude(Some("DEFAULT"))));
        assert_eq!(
            parsed.stages.get(AgentStage::CommentReply),
            Some(&profile(BaseCodingAgent::Codex, None))
        );
        assert_eq!(parsed.stages.planning, None);
        // A config without the section reads as no overrides.
        let empty: AgentsConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(empty, AgentsConfig::default());
        assert_eq!(empty.external, None);
        let keys: Vec<&str> = AgentStage::ALL.iter().map(|stage| stage.key()).collect();
        let written = serde_json::to_value(StageProfiles::default()).unwrap();
        let mut written: Vec<&str> = written
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        written.sort();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(written, sorted);
    }

    fn card(metadata: serde_json::Value) -> IssueExtMetadata {
        IssueExtMetadata::from_value(&metadata)
    }

    /// Which profile plans a card, by where the card came from.
    #[test]
    fn externally_filed_cards_plan_under_the_external_profile() {
        let agents = Config::default()
            .project_settings(uuid::Uuid::new_v4())
            .agents;
        let guarded = claude(Some("GUARDED"));
        assert_eq!(agents.external, guarded);

        let tammy = card(serde_json::json!({
            "work_class": "investigation",
            "origin": { "kind": "tammy", "ref": "https://acme.slack.com/archives/C1/p1" }
        }));
        let api = card(serde_json::json!({
            "work_class": "bug",
            "origin": { "kind": "api", "ref": "ticket-9" }
        }));
        let outside_issue = card(serde_json::json!({
            "work_class": "bug",
            "jira": { "key": "acme/widgets#12" },
            "filed_externally": true
        }));
        let owners_issue = card(serde_json::json!({
            "work_class": "bug",
            "jira": { "key": "acme/widgets#13" }
        }));
        let jira = card(serde_json::json!({
            "work_class": "bug",
            "jira": { "key": "PC-42", "issue_type": "Bug" }
        }));
        let made_on_the_board = card(serde_json::json!({ "work_class": "bug" }));

        for (name, metadata) in [
            ("tammy", &tammy),
            ("api", &api),
            ("public issue", &outside_issue),
        ] {
            assert_eq!(
                agents.for_launch(AgentStage::Planning, metadata),
                &guarded,
                "{name}"
            );
            assert!(
                agents.plans_guarded(AgentStage::Planning, metadata),
                "{name}"
            );
            // Executing and Reviewing run their own profiles whoever filed it.
            for stage in [AgentStage::Executing, AgentStage::Reviewing] {
                assert_eq!(
                    agents.for_launch(stage, metadata),
                    agents.get(stage),
                    "{name}"
                );
                assert!(!agents.plans_guarded(stage, metadata), "{name}");
            }
        }
        for (name, metadata) in [
            ("owner's issue", &owners_issue),
            ("jira", &jira),
            ("board", &made_on_the_board),
        ] {
            assert_eq!(
                agents.for_launch(AgentStage::Planning, metadata),
                &agents.planning,
                "{name}"
            );
            assert!(
                !agents.plans_guarded(AgentStage::Planning, metadata),
                "{name}"
            );
        }
    }

    /// `agents.external` resolves board, then global, then the planning
    /// profile's guarded variant; an agent without one plans external cards
    /// under the planning profile, and nothing is recorded as guarded.
    #[test]
    fn the_external_profile_resolves_board_then_global_then_the_guarded_variant() {
        let api = card(serde_json::json!({ "origin": { "kind": "api", "ref": "x" } }));

        let mut config = Config::default();
        config.agents.stages.planning = Some(claude(Some("SONNET")));
        let agents = config.project_settings(uuid::Uuid::new_v4()).agents;
        assert_eq!(agents.external, claude(Some("GUARDED")));

        config.agents.external = Some(profile(BaseCodingAgent::Gemini, None));
        let board_id = uuid::Uuid::new_v4();
        let board = ProjectAutomationDefaults {
            agents: AgentsConfig {
                external: Some(claude(Some("DEFAULT"))),
                ..Default::default()
            },
            ..Default::default()
        };
        config
            .orchestrator
            .project_defaults
            .insert(board_id.to_string(), board);
        assert_eq!(
            config
                .project_settings(uuid::Uuid::new_v4())
                .agents
                .external,
            profile(BaseCodingAgent::Gemini, None)
        );
        assert_eq!(
            config.project_settings(board_id).agents.external,
            claude(Some("DEFAULT"))
        );

        let codex = Config {
            executor_profile: profile(BaseCodingAgent::Codex, None),
            ..Default::default()
        }
        .project_settings(uuid::Uuid::new_v4())
        .agents;
        assert_eq!(codex.external, codex.planning);
        assert_eq!(
            codex.for_launch(AgentStage::Planning, &api),
            &codex.planning
        );
        assert!(!codex.plans_guarded(AgentStage::Planning, &api));
    }

    /// The settings descriptor's fields are keys of `agents`, the saved
    /// value reads back, and a config saved without the key reads as unset.
    #[test]
    fn the_external_field_round_trips_through_its_descriptor_and_the_config() {
        let descriptor = settings_descriptor();
        let written = serde_json::to_value(AgentsConfig::default()).unwrap();
        for field in &descriptor.fields {
            assert!(
                written.get(&field.key).is_some(),
                "{} is not an agents key",
                field.key
            );
        }
        let external = descriptor
            .fields
            .iter()
            .find(|field| field.key == "external")
            .unwrap();
        assert_eq!(external.kind, FieldKind::Profile);
        assert!(external.help.starts_with(
            "Planning profile for cards filed from outside the team (API, public issues)."
        ));
        assert!(external.help.contains("Claude Code (`GUARDED`)"));
        assert!(external.help.contains("Codex"));
        assert!(external.help.contains("planning profile unchanged"));
        assert!(external.help.contains("does not confine its shell"));
        assert!(descriptors().iter().any(|d| d.kind == SETTINGS_KIND));

        assert_eq!(external.key, "external");
        let saved = serde_json::json!({
            "stages": {},
            "external": { "executor": "CLAUDE_CODE", "variant": "GUARDED" }
        });
        let parsed: AgentsConfig = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(parsed.external, Some(claude(Some("GUARDED"))));
        assert_eq!(
            serde_json::to_value(&parsed).unwrap()["external"],
            saved["external"]
        );

        // A config file saved before the key existed.
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        raw["agents"] = serde_json::json!({ "stages": { "planning": { "executor": "CODEX" } } });
        let config = Config::from(raw.to_string());
        assert_eq!(config.agents.external, None);
        assert_eq!(
            config.agents.stages.planning,
            Some(profile(BaseCodingAgent::Codex, None))
        );
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        raw.as_object_mut().unwrap().remove("agents");
        let config = Config::from(raw.to_string());
        assert_eq!(config.agents, AgentsConfig::default());
        assert_eq!(
            config
                .project_settings(uuid::Uuid::new_v4())
                .agents
                .external,
            claude(Some("GUARDED"))
        );
    }

    fn permissions_field() -> Field {
        settings_descriptor()
            .fields
            .into_iter()
            .find(|field| field.key == "permissions")
            .expect("a permissions field")
    }

    /// The field offers exactly the two policies, by the values the config
    /// holds; a saved value reads back and writes out the same; its help
    /// names which agents honour it and which do not.
    #[test]
    fn the_permissions_field_round_trips_through_its_descriptor_and_the_config() {
        let field = permissions_field();
        assert_eq!(field.label, "Permissions");
        let FieldKind::Select { options } = &field.kind else {
            panic!("permissions is a select");
        };
        let values: Vec<&str> = options.iter().map(|option| option.value.as_str()).collect();
        assert_eq!(values, ["auto", "supervised"]);
        assert_eq!(options[0].label, "Auto — agents run without asking");
        assert_eq!(
            options[1].label,
            "Supervised — tool permission requests come to you"
        );
        for value in values {
            let parsed: AgentsConfig =
                serde_json::from_value(serde_json::json!({ "permissions": value })).unwrap();
            assert_eq!(
                serde_json::to_value(&parsed).unwrap()["permissions"],
                serde_json::json!(value)
            );
        }
        // `plan` is the executors' own mode, not a setting.
        assert!(
            serde_json::from_value::<AgentsConfig>(serde_json::json!({ "permissions": "plan" }))
                .is_err()
        );

        let help = &field.help;
        assert!(
            help.contains("Honoured by Claude Code, Gemini CLI, Codex, OpenCode and Qwen Code.")
        );
        assert!(help.contains(
            "Amp, Cursor Agent, GitHub Copilot CLI and Droid cannot route a request to you"
        ));
        assert!(help.contains("10 hours"));
    }

    /// A config saved before the field existed runs every session as before:
    /// auto.
    #[test]
    fn a_config_without_permissions_loads_as_auto() {
        let mut raw = serde_json::to_value(Config::default()).unwrap();
        raw["agents"] = serde_json::json!({ "stages": {}, "external": null });
        let config = Config::from(raw.to_string());
        assert_eq!(config.agents.permissions, None);
        let agents = config.project_settings(uuid::Uuid::new_v4()).agents;
        assert_eq!(agents.permissions, AgentPermissions::Auto);
        assert_eq!(
            executor_config(agents.planning.clone(), agents.permissions).permission_policy,
            Some(PermissionPolicy::Auto)
        );
    }

    /// The setting is global: every board's sessions run under it, and a
    /// board's own value is not read.
    #[test]
    fn the_permission_setting_is_the_same_on_every_board() {
        let mut config = Config::default();
        config.agents.permissions = Some(AgentPermissions::Supervised);
        let board_id = uuid::Uuid::new_v4();
        config.orchestrator.project_defaults.insert(
            board_id.to_string(),
            ProjectAutomationDefaults {
                agents: AgentsConfig {
                    permissions: Some(AgentPermissions::Auto),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        for board in [board_id, uuid::Uuid::new_v4()] {
            assert_eq!(
                config.project_settings(board).agents.permissions,
                AgentPermissions::Supervised
            );
        }
    }

    /// The setting reaches the executor config of every shipped profile an
    /// agent that can be supervised runs; a variant with a policy of its own
    /// keeps it; an agent that cannot be supervised gets none.
    #[test]
    fn a_profiles_own_policy_wins_over_the_setting() {
        let mut profiles = ExecutorConfigs::from_defaults();
        profiles
            .executors
            .get_mut(&BaseCodingAgent::ClaudeCode)
            .unwrap()
            .set_variant(
                "ASKS".to_string(),
                serde_json::from_value(serde_json::json!({ "CLAUDE_CODE": { "approvals": true } }))
                    .unwrap(),
            )
            .unwrap();
        let supervised = AgentPermissions::Supervised;
        for profile in [
            claude(None),
            claude(Some("SONNET")),
            claude(Some("GUARDED")),
        ] {
            let config = executor_config_with(&profiles, profile.clone(), supervised);
            assert_eq!(config.profile_id(), profile);
            assert_eq!(config.permission_policy, Some(PermissionPolicy::Supervised));
        }
        // Its own approvals stand under either setting.
        for setting in [AgentPermissions::Auto, AgentPermissions::Supervised] {
            let config = executor_config_with(&profiles, claude(Some("ASKS")), setting);
            assert_eq!(config.permission_policy, None);
            assert_eq!(config.variant.as_deref(), Some("ASKS"));
        }
        let cursor = executor_config_with(
            &profiles,
            profile(BaseCodingAgent::CursorAgent, None),
            supervised,
        );
        assert_eq!(cursor.permission_policy, None);
    }

    #[test]
    fn every_agent_is_described_and_unknown_kinds_fail_the_test() {
        let described = descriptors();
        assert_eq!(described.len(), BaseCodingAgent::all().len() + 1);
        let claude = described
            .iter()
            .find(|descriptor| descriptor.kind == "CLAUDE_CODE")
            .unwrap();
        assert_eq!(claude.display_name, "Claude Code");
        assert_eq!(
            claude.agent.as_ref().unwrap().economy_variant.as_deref(),
            Some("SONNET")
        );
        let outcome = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(test_agent("NOT_AN_AGENT"));
        assert_eq!(
            outcome,
            Err("unknown coding agent: NOT_AN_AGENT".to_string())
        );
    }
}
