//! What every integration area shares: the descriptor a provider publishes
//! about itself, and how its secret-shaped fields are resolved.
//!
//! An area (notifications, ticketing, reproduction, evidence) is a module
//! under `services/<area>/` with one file per provider. Each provider describes
//! itself with a [`Descriptor`]; Settings renders one generic section per area
//! from those descriptors, and `docs/configuration.md` carries a table
//! generated from the same data, so a new provider shows up in both without
//! touching either. The coding agents are listed alongside them as the
//! `agents` area, described by the executors themselves.

use std::collections::BTreeMap;

use executors::descriptor::AgentDescriptor;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::{
    agents, config::Config, evidence, notifications, reproduction, secrets::Secrets, tracker,
};

/// How Settings renders one field and what the config holds for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    /// Rendered as a password input. Also read from the secrets store under
    /// the provider's secret name for the field (see [`secret_name`]).
    Secret,
    Url,
    Select {
        options: Vec<String>,
    },
    Bool,
    Number,
}

/// One setting a provider reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Field {
    /// The key in the provider's config entry.
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    pub help: String,
}

impl Field {
    pub fn new(key: &str, label: &str, kind: FieldKind, required: bool, help: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            kind,
            required,
            help: help.to_string(),
        }
    }
}

/// What a provider says about itself: enough for Settings to render it and
/// for the configuration reference to document it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Descriptor {
    /// The config string naming this provider (`kind` in its config entry).
    pub kind: String,
    pub display_name: String,
    /// One sentence.
    pub summary: String,
    pub fields: Vec<Field>,
    /// Names in the secrets store this provider reads, one per secret field.
    pub secrets: Vec<String>,
    pub docs_url: String,
    /// False for a provider that is named but not implemented: Settings shows
    /// it and does not let it be chosen.
    pub available: bool,
    /// Set for a coding agent (the `agents` area), which is not configured
    /// through fields: what it needs installed, how it signs in and how its
    /// sessions get MCP servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent: Option<AgentDescriptor>,
}

impl Descriptor {
    /// Builds a descriptor whose `secrets` are derived from its secret fields.
    pub fn new(
        kind: &str,
        display_name: &str,
        summary: &str,
        fields: Vec<Field>,
        docs_url: &str,
    ) -> Self {
        let secrets = fields
            .iter()
            .filter(|field| field.kind == FieldKind::Secret)
            .map(|field| secret_name(kind, &field.key))
            .collect();
        Self {
            kind: kind.to_string(),
            display_name: display_name.to_string(),
            summary: summary.to_string(),
            fields,
            secrets,
            docs_url: docs_url.to_string(),
            available: true,
            agent: None,
        }
    }

    /// Marks the provider as named but not implemented.
    pub fn unavailable(mut self) -> Self {
        self.available = false;
        self
    }
}

/// The result of a provider's `test()`, as the Test connection button shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TestOutcome {
    pub ok: bool,
    pub message: String,
}

/// Every integration area and its providers' descriptors, keyed by area.
pub fn descriptors() -> BTreeMap<String, Vec<Descriptor>> {
    BTreeMap::from([
        (
            notifications::AREA.to_string(),
            notifications::descriptors(),
        ),
        (tracker::AREA.to_string(), tracker::descriptors()),
        (reproduction::AREA.to_string(), reproduction::descriptors()),
        (evidence::AREA.to_string(), evidence::descriptors()),
        (agents::AREA.to_string(), agents::descriptors()),
    ])
}

/// Runs one provider's connection test. `fields` are the provider's settings
/// as Settings holds them (possibly unsaved); `None` tests what the config
/// holds (for notifications, the first configured entry of that kind).
pub async fn test(
    config: &Config,
    area: &str,
    kind: &str,
    fields: Option<BTreeMap<String, serde_json::Value>>,
) -> TestOutcome {
    let result = match area {
        notifications::AREA => notifications::test_target(config, kind, fields).await,
        tracker::AREA => tracker::test_provider(config, kind, fields).await,
        reproduction::AREA => reproduction::test_target(config, kind, fields).await,
        evidence::AREA => evidence::test_source(config, kind, fields).await,
        agents::AREA => agents::test_agent(kind).await,
        other => Err(format!("unknown integration area: {other}")),
    };
    match result {
        Ok(message) => TestOutcome { ok: true, message },
        Err(message) => TestOutcome { ok: false, message },
    }
}

/// The secrets-store name a provider's secret field is read under:
/// `<KIND>_<FIELD>`, upper-cased (`slack` + `bot_token` → `SLACK_BOT_TOKEN`).
/// A field already named after its kind is not prefixed twice
/// (`pagerduty` + `pagerduty_api_token` → `PAGERDUTY_API_TOKEN`). The same
/// name is the environment variable consulted last.
pub fn secret_name(kind: &str, field: &str) -> String {
    let own = field
        .strip_prefix(kind)
        .is_some_and(|rest| rest.starts_with('_'));
    if own {
        field.to_ascii_uppercase()
    } else {
        format!("{kind}_{field}").to_ascii_uppercase()
    }
}

/// Which source a secret field consults first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretPrecedence {
    /// Secrets store, then the config value, then the environment: values
    /// that should never sit in `config.json` win wherever they are set.
    StoreFirst,
    /// The config value, then the store, then the environment: for entries
    /// that exist to differ from the shared value (a board's own target), so
    /// the shared store entry cannot override them.
    ConfigFirst,
}

/// Resolves one secret-shaped field. Blank values count as unset.
pub fn resolve_secret(
    configured: Option<&str>,
    name: &str,
    store: &Secrets,
    precedence: SecretPrecedence,
) -> Option<String> {
    let clean = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let from_config = clean(configured);
    let from_store = clean(store.get(name));
    let first_two = match precedence {
        SecretPrecedence::StoreFirst => from_store.or(from_config),
        SecretPrecedence::ConfigFirst => from_config.or(from_store),
    };
    first_two.or_else(|| clean(std::env::var(name).ok().as_deref()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn store(pairs: &[(&str, &str)]) -> Secrets {
        Secrets::load_from(
            Path::new("/nonexistent/prepcook-test-secrets.env"),
            pairs
                .iter()
                .map(|(name, value)| (format!("PREPCOOK_SECRET_{name}"), value.to_string())),
        )
    }

    #[test]
    fn secret_names_follow_kind_and_field() {
        assert_eq!(secret_name("slack", "bot_token"), "SLACK_BOT_TOKEN");
        assert_eq!(secret_name("webhook", "secret"), "WEBHOOK_SECRET");
        assert_eq!(
            secret_name("pagerduty", "pagerduty_api_token"),
            "PAGERDUTY_API_TOKEN"
        );
        assert_eq!(secret_name("linear", "linear_api_key"), "LINEAR_API_KEY");
        assert_eq!(secret_name("jira", "api_token"), "JIRA_API_TOKEN");
    }

    #[test]
    fn the_store_wins_for_shared_entries_and_config_wins_for_board_entries() {
        let secrets = store(&[("PCTEST_ONE_TOKEN", "from-store")]);
        assert_eq!(
            resolve_secret(
                Some("from-config"),
                "PCTEST_ONE_TOKEN",
                &secrets,
                SecretPrecedence::StoreFirst
            )
            .as_deref(),
            Some("from-store")
        );
        assert_eq!(
            resolve_secret(
                Some("from-config"),
                "PCTEST_ONE_TOKEN",
                &secrets,
                SecretPrecedence::ConfigFirst
            )
            .as_deref(),
            Some("from-config")
        );
        // A blank config value falls through to the store.
        assert_eq!(
            resolve_secret(
                Some("  "),
                "PCTEST_ONE_TOKEN",
                &secrets,
                SecretPrecedence::ConfigFirst
            )
            .as_deref(),
            Some("from-store")
        );
        assert_eq!(
            resolve_secret(
                None,
                "PCTEST_UNSET_TOKEN",
                &secrets,
                SecretPrecedence::StoreFirst
            ),
            None
        );
    }

    #[test]
    fn descriptors_list_every_area_and_derive_their_secret_names() {
        let all = descriptors();
        let notifications = &all["notifications"];
        let slack = notifications
            .iter()
            .find(|descriptor| descriptor.kind == "slack")
            .unwrap();
        assert!(slack.secrets.contains(&"SLACK_BOT_TOKEN".to_string()));
        for descriptor in notifications {
            assert!(!descriptor.summary.is_empty(), "{}", descriptor.kind);
            assert!(!descriptor.fields.is_empty(), "{}", descriptor.kind);
        }
        let ticketing: Vec<&str> = all["ticketing"]
            .iter()
            .map(|descriptor| descriptor.kind.as_str())
            .collect();
        assert_eq!(ticketing, ["jira", "github_issues", "linear", "pagerduty"]);
        let reproduction: Vec<&str> = all["reproduction"]
            .iter()
            .map(|descriptor| descriptor.kind.as_str())
            .collect();
        assert_eq!(reproduction, ["local", "remote"]);
        let evidence: Vec<&str> = all["evidence"]
            .iter()
            .map(|descriptor| descriptor.kind.as_str())
            .collect();
        assert_eq!(
            evidence,
            ["sentry", "dataset", "datadog", "cloudwatch", "log_files"]
        );
        for descriptor in &all["agents"] {
            let agent = descriptor.agent.as_ref().expect("agent details");
            assert_eq!(descriptor.kind, agent.executor.to_string());
            assert!(descriptor.fields.is_empty());
        }
    }
}
