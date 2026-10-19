//! Which coding agent runs which orchestrator stage, and the `agents`
//! integration area that lists every agent Prepcook can drive.
//!
//! A stage's profile resolves, most specific first: the board's
//! `agents.stages.<stage>`, the global `agents.stages.<stage>`, then the
//! stage's default, which is built from the board's `executor_profile` (else
//! the global one). Planning runs that profile as it is; every other stage
//! takes the agent's economy variant when the profile names no variant (see
//! [`economy_variant_of`]).

use executors::{
    descriptor::{self as agent_descriptor, economy_variant_of},
    executors::BaseCodingAgent,
    profile::ExecutorProfileId,
};

use crate::services::{config::StageProfiles, integrations::Descriptor};

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
}

impl StageAgents {
    /// Resolves each stage: the board's override, then the global one, then
    /// the stage's default built from `default` (the board's default profile,
    /// already resolved against the global one).
    pub fn resolve(
        board: &StageProfiles,
        global: &StageProfiles,
        default: &ExecutorProfileId,
    ) -> Self {
        let resolve = |stage: AgentStage| {
            board
                .get(stage)
                .or_else(|| global.get(stage))
                .cloned()
                .unwrap_or_else(|| stage.default_profile(default))
        };
        Self {
            planning: resolve(AgentStage::Planning),
            executing: resolve(AgentStage::Executing),
            reviewing: resolve(AgentStage::Reviewing),
            comment_reply: resolve(AgentStage::CommentReply),
            pattern: resolve(AgentStage::Pattern),
        }
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

/// Every coding agent as an integration descriptor: no fields to configure,
/// the agent's own description under `agent`.
pub fn descriptors() -> Vec<Descriptor> {
    agent_descriptor::descriptors()
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
        .collect()
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
    use super::*;
    use crate::services::config::{AgentsConfig, Config, ProjectAutomationDefaults};

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
                    &config.orchestrator.project_defaults[&board_id.to_string()]
                        .agents
                        .stages,
                    &config.agents.stages,
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

    #[test]
    fn every_agent_is_described_and_unknown_kinds_fail_the_test() {
        let described = descriptors();
        assert_eq!(described.len(), BaseCodingAgent::all().len());
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
