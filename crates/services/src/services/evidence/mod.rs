//! The evidence seam: where a failure leaves traces outside the reproduction
//! harness (an error tracker, a log service, the app's own log files), what
//! the agent is told about each place, and searching them on its behalf.
//!
//! Sources live in `evidence.sources` (several at once), a board can switch
//! the global ones off or add its own (`ProjectEvidenceOverride`), and a
//! repository's `.prepcook/scripts.toml` can name its own log files under
//! `[[profile.evidence]]`. Every configured source:
//!
//! - writes one paragraph into `{{product.evidence_tools}}` ([`EvidenceSource::describe`]);
//! - may put an MCP server into the agent sessions' MCP file
//!   ([`EvidenceSource::mcp_server`], see `agent_mcp`);
//! - may be searched through `POST /api/evidence/search` and the
//!   `evidence_search` MCP tool ([`EvidenceSource::search`]).
//!
//! Adding a provider: a file here with a struct implementing
//! [`EvidenceSource`] and a `descriptor()`, a variant in [`EvidenceKind`] and
//! [`EvidenceProvider`], and its arm in [`EvidenceKind::descriptor`] and
//! [`build_provider`]. Settings and the configuration reference pick it up
//! from the descriptor (see `docs/evidence.md`).

pub mod cloudwatch;
pub mod datadog;
pub mod dataset;
pub mod log_files;
pub mod sentry;

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use ts_rs::TS;
use uuid::Uuid;

use self::{
    cloudwatch::CloudWatchSource, datadog::DatadogSource, dataset::DataSetSource,
    log_files::LogFilesSource, sentry::SentrySource,
};
use crate::services::{
    config::{Config, EvidenceSourceConfig, TargetOrigin},
    integrations::{Descriptor, Field, FieldKind, SecretPrecedence, resolve_secret, secret_name},
    secrets::{self, Secrets},
};

/// The integration area name, as the descriptors route and Settings use it.
pub const AREA: &str = "evidence";

/// Timeout on every request a provider makes.
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Hits returned when the caller names no limit.
pub const DEFAULT_LIMIT: usize = 50;
/// The most hits one search returns, across every source.
pub const MAX_LIMIT: usize = 200;
/// The window searched when the caller names no start.
pub const DEFAULT_WINDOW: chrono::Duration = chrono::Duration::hours(24);
/// Characters kept of one hit's message.
const MESSAGE_CAP: usize = 2_000;

/// The sentence `{{product.evidence_tools}}` ends with whenever it is not
/// empty.
pub const HARNESS_FALLBACK: &str =
    "If none of these show the failure, the harness is your evidence.";

/// The places evidence can be read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Sentry,
    Dataset,
    Datadog,
    Cloudwatch,
    LogFiles,
}

impl EvidenceKind {
    pub const ALL: [EvidenceKind; 5] = [
        Self::Sentry,
        Self::Dataset,
        Self::Datadog,
        Self::Cloudwatch,
        Self::LogFiles,
    ];

    /// The config string (`kind` in a source).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sentry => "sentry",
            Self::Dataset => "dataset",
            Self::Datadog => "datadog",
            Self::Cloudwatch => "cloudwatch",
            Self::LogFiles => "log_files",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    pub fn descriptor(self) -> Descriptor {
        match self {
            Self::Sentry => SentrySource::descriptor(),
            Self::Dataset => DataSetSource::descriptor(),
            Self::Datadog => DatadogSource::descriptor(),
            Self::Cloudwatch => CloudWatchSource::descriptor(),
            Self::LogFiles => LogFilesSource::descriptor(),
        }
    }

    /// The provider's name as people know it ("Sentry", "Log files").
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Sentry => "Sentry",
            Self::Dataset => "DataSet",
            Self::Datadog => "Datadog",
            Self::Cloudwatch => "CloudWatch Logs",
            Self::LogFiles => "Log files",
        }
    }
}

impl std::fmt::Display for EvidenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every provider's descriptor, in [`EvidenceKind::ALL`] order.
pub fn descriptors() -> Vec<Descriptor> {
    EvidenceKind::ALL
        .into_iter()
        .map(EvidenceKind::descriptor)
        .collect()
}

/// The fields every source has besides its provider's own: a label first,
/// then (after the provider's fields) the product areas it covers and notes
/// for the agent.
pub(crate) fn with_common_fields(kind: EvidenceKind, own: Vec<Field>) -> Vec<Field> {
    let mut fields = vec![Field::new(
        "label",
        "Label",
        FieldKind::Text,
        false,
        &format!(
            "What the agent and evidence_search call this source, e.g. \"Production errors\". \
             Blank uses \"{}\"; give each source of the same kind its own.",
            kind.display_name()
        ),
    )];
    fields.extend(own);
    fields.push(Field::new(
        "areas",
        "Product areas",
        FieldKind::Text,
        false,
        "Comma-separated [profile] areas this source covers. A search narrowed to an area skips \
         sources that name other areas; blank covers every area.",
    ));
    fields.push(Field::new(
        "notes",
        "Notes for the agent",
        FieldKind::Text,
        false,
        "A sentence or two appended to what the agent is told about this source: what is logged \
         there, how long it is kept, which fields matter.",
    ));
    fields
}

// ---------------------------------------------------------------------------
// Searching
// ---------------------------------------------------------------------------

/// One search, as every provider receives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchQuery {
    /// Text to find, matched as a literal phrase in every source.
    pub pattern: String,
    /// `(from, to)`, inclusive.
    pub window: (DateTime<Utc>, DateTime<Utc>),
    /// Most hits wanted from this source.
    pub limit: usize,
    /// A `[profile] areas` entry the search is about; sources that name other
    /// areas are skipped (see [`SourceSettings::covers`]).
    pub area: Option<String>,
    /// Where relative log file paths resolve: the agent's worktree. A
    /// `log_files` source without one uses the repository's checkout.
    pub worktree: Option<PathBuf>,
}

impl SearchQuery {
    /// Whether `at` falls in the window.
    pub fn contains(&self, at: DateTime<Utc>) -> bool {
        self.window.0 <= at && at <= self.window.1
    }
}

/// One thing a source found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// When it happened; `None` for a log line without a timestamp.
    pub at: Option<DateTime<Utc>>,
    /// The label of the source it came from.
    pub source: String,
    pub message: String,
    /// Where a person reads it in full.
    pub link: Option<String>,
    /// The provider's structured fields (service, level, file and line, …).
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl Hit {
    pub(crate) fn new(source: &str, at: Option<DateTime<Utc>>, message: &str) -> Self {
        Self {
            at,
            source: source.to_string(),
            message: clip(message.trim_end(), MESSAGE_CAP),
            link: None,
            fields: BTreeMap::new(),
        }
    }

    pub(crate) fn link(mut self, link: Option<String>) -> Self {
        self.link = link;
        self
    }

    pub(crate) fn field(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        let value = value.into();
        if !value.is_null() {
            self.fields.insert(key.to_string(), value);
        }
        self
    }
}

/// What one source (or a merged search) found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Newest first; hits without a time last.
    pub hits: Vec<Hit>,
    /// More matched than were returned.
    pub truncated: bool,
}

/// What a merged search did with one source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReport {
    pub label: String,
    pub kind: String,
    /// False when the source was skipped (no API Prep Cook drives, or an area
    /// it does not cover); `note` says which.
    pub searched: bool,
    pub hits: usize,
    pub truncated: bool,
    pub error: Option<String>,
    pub note: Option<String>,
}

/// A search across every source (or the one named), merged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MergedSearch {
    pub hits: Vec<Hit>,
    pub truncated: bool,
    pub sources: Vec<SourceReport>,
}

/// Merges per-source results: newest first, hits without a time after every
/// dated one, at most `limit`. Truncated when any source was, or when the
/// merge dropped hits.
pub fn merge(results: Vec<SearchResult>, limit: usize) -> SearchResult {
    let mut truncated = results.iter().any(|result| result.truncated);
    let mut hits: Vec<Hit> = results.into_iter().flat_map(|result| result.hits).collect();
    // Stable: hits a source returned in its own order keep it among equals.
    hits.sort_by(|a, b| match (a.at, b.at) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    if hits.len() > limit {
        hits.truncate(limit);
        truncated = true;
    }
    SearchResult { hits, truncated }
}

/// Keeps the newest `limit` of a source's hits and says whether it dropped
/// any; for providers that fetch one more than asked to learn that.
pub(crate) fn capped(hits: Vec<Hit>, limit: usize, more: bool) -> SearchResult {
    merge(
        vec![SearchResult {
            hits,
            truncated: more,
        }],
        limit,
    )
}

/// `text` clipped to `max` characters, with an ellipsis when cut.
pub(crate) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `pattern` as a quoted phrase, for query languages where `"…"` matches the
/// text literally.
pub(crate) fn quoted(pattern: &str) -> String {
    format!(
        "\"{}\"",
        pattern.trim().replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// Percent-encodes a query value.
pub(crate) fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

// ---------------------------------------------------------------------------
// The provider seam
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum EvidenceError {
    /// The source lacks a setting it needs.
    #[error("not configured: {0}")]
    NotConfigured(String),
    /// The request did not complete (connect error, timeout, a CLI that would
    /// not start).
    #[error("request failed: {0}")]
    Http(String),
    /// The service answered with a failure status.
    #[error("HTTP {status}: {body}")]
    Rejected { status: u16, body: String },
    /// The service answered but its answer says it failed, or could not be
    /// read.
    #[error("{0}")]
    Api(String),
    /// The caller asked for something that cannot be searched.
    #[error("{0}")]
    Invalid(String),
}

impl EvidenceError {
    pub(crate) fn http(error: reqwest::Error) -> Self {
        Self::Http(secrets::redact(&error.without_url().to_string()))
    }

    /// The error for a non-2xx answer, body clipped and redacted.
    pub(crate) async fn rejected(response: reqwest::Response) -> Self {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        Self::Rejected {
            status,
            body: secrets::redact(&clip(body.trim(), 300)),
        }
    }
}

/// An MCP server a source gives agent sessions: the key it is written under
/// in the session MCP file, and its entry.
#[derive(Clone, Debug, PartialEq)]
pub struct McpServerEntry {
    pub name: String,
    pub server: serde_json::Value,
}

/// One place evidence can be read from.
#[async_trait]
#[enum_dispatch(EvidenceProvider)]
pub trait EvidenceSource: Send + Sync {
    /// The config string of this provider.
    fn kind(&self) -> &'static str;

    /// What the agent and `evidence_search` call this source.
    fn label(&self) -> String;

    /// The paragraph `{{product.evidence_tools}}` gets for this source: what
    /// it is, how to query it, what to look for.
    fn describe(&self) -> String;

    /// An MCP server for agent sessions, with its secrets baked in; `None`
    /// when the provider has none.
    fn mcp_server(&self) -> Option<McpServerEntry>;

    /// Searches the source. `Ok(None)` when Prep Cook drives no API for it as
    /// configured (the agent reads it another way, which `describe` says).
    async fn search(&self, query: &SearchQuery) -> Result<Option<SearchResult>, EvidenceError>;

    /// A cheap check that the settings work, returning what it found.
    async fn test(&self) -> Result<String, EvidenceError>;
}

#[enum_dispatch]
pub enum EvidenceProvider {
    Sentry(SentrySource),
    Dataset(DataSetSource),
    Datadog(DatadogSource),
    Cloudwatch(CloudWatchSource),
    LogFiles(LogFilesSource),
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// Where a source's entry came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceOrigin {
    /// `evidence.sources`.
    Global,
    /// A board's own `evidence.sources`.
    Board,
    /// The repository's `[[profile.evidence]]`.
    Repository,
}

/// One source's settings with its secrets resolved: what a provider is built
/// from, and what the cache compares to decide whether to rebuild.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSettings {
    pub kind: EvidenceKind,
    pub origin: SourceOrigin,
    pub fields: BTreeMap<String, String>,
}

impl SourceSettings {
    /// A field's value, `None` when unset or blank.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    /// A list field: comma- or newline-separated.
    pub fn list(&self, key: &str) -> Vec<String> {
        self.get(key)
            .map(|value| {
                value
                    .split(['\n', ','])
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The label, else the provider's name.
    pub fn label(&self) -> String {
        self.get("label")
            .map(str::to_string)
            .unwrap_or_else(|| self.kind.display_name().to_string())
    }

    /// The notes for the agent, as a sentence ready to append.
    pub fn notes(&self) -> Option<String> {
        self.get("notes").map(|notes| {
            let notes = notes.trim();
            if notes.ends_with(['.', '!', '?']) {
                notes.to_string()
            } else {
                format!("{notes}.")
            }
        })
    }

    /// Whether a search about `area` should read this source: it names no
    /// areas, or names that one (case-insensitively).
    pub fn covers(&self, area: Option<&str>) -> bool {
        let Some(area) = area.map(str::trim).filter(|area| !area.is_empty()) else {
            return true;
        };
        let areas = self.list("areas");
        areas.is_empty() || areas.iter().any(|own| own.eq_ignore_ascii_case(area))
    }

    /// "Production errors (Sentry)", or just "Sentry" when unlabelled.
    pub fn heading(&self) -> String {
        let label = self.label();
        if label == self.kind.display_name() {
            label
        } else {
            format!("{label} ({})", self.kind.display_name())
        }
    }
}

/// What a source contributes to a playbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBrief {
    pub label: String,
    pub kind: EvidenceKind,
    /// The paragraph from [`EvidenceSource::describe`].
    pub describe: String,
}

/// `{{product.evidence_tools}}`: one paragraph per source, then the
/// manifest's `[profile] evidence_tools` when set, then
/// [`HARNESS_FALLBACK`]. Empty when there is neither a source nor an
/// appendix, so a playbook's `{{#if product.evidence_tools}}` can say there
/// is nothing to consult.
pub fn render_evidence_tools(briefs: &[SourceBrief], appendix: &str) -> String {
    let mut parts: Vec<String> = briefs
        .iter()
        .map(|brief| brief.describe.trim().to_string())
        .collect();
    if !appendix.trim().is_empty() {
        parts.push(appendix.trim().to_string());
    }
    if parts.is_empty() {
        return String::new();
    }
    parts.push(HARNESS_FALLBACK.to_string());
    parts.join("\n\n")
}

/// `{{product.evidence_sources}}`: one `- label` line per source.
pub fn render_evidence_sources(briefs: &[SourceBrief]) -> String {
    briefs
        .iter()
        .map(|brief| format!("- {}", brief.label))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Builds one provider. `root` is where relative paths resolve when a search
/// names no worktree (the repository's checkout).
pub fn build_provider(
    settings: &SourceSettings,
    root: Option<PathBuf>,
) -> Result<EvidenceProvider, EvidenceError> {
    Ok(match settings.kind {
        EvidenceKind::Sentry => SentrySource::new(settings)?.into(),
        EvidenceKind::Dataset => DataSetSource::new(settings)?.into(),
        EvidenceKind::Datadog => DatadogSource::new(settings)?.into(),
        EvidenceKind::Cloudwatch => CloudWatchSource::new(settings)?.into(),
        EvidenceKind::LogFiles => LogFilesSource::new(settings, root)?.into(),
    })
}

/// One built source.
pub struct BuiltSource {
    pub settings: SourceSettings,
    pub provider: EvidenceProvider,
}

/// The sources for one scope (a board and its repository), built.
pub struct EvidenceSet {
    sources: Vec<BuiltSource>,
}

/// Builds every source, skipping (and logging) one whose settings are
/// incomplete so it cannot hide the others.
pub fn build(settings: &[SourceSettings], root: Option<PathBuf>) -> EvidenceSet {
    let mut sources = Vec::new();
    for entry in settings {
        match build_provider(entry, root.clone()) {
            Ok(provider) => sources.push(BuiltSource {
                settings: entry.clone(),
                provider,
            }),
            Err(error) => tracing::warn!(
                kind = %entry.kind,
                "evidence: skipping the {} source {:?}: {error}",
                entry.kind,
                entry.label()
            ),
        }
    }
    EvidenceSet { sources }
}

impl EvidenceSet {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn sources(&self) -> &[BuiltSource] {
        &self.sources
    }

    /// What each source contributes to a playbook, in configuration order.
    pub fn briefs(&self) -> Vec<SourceBrief> {
        self.sources
            .iter()
            .map(|source| SourceBrief {
                label: source.provider.label(),
                kind: source.settings.kind,
                describe: source.provider.describe(),
            })
            .collect()
    }

    /// Every source's MCP server, keyed uniquely: the first server of a name
    /// keeps it, a later one with a different entry gets `-2`, `-3`, … and an
    /// identical one is dropped.
    pub fn mcp_servers(&self) -> Vec<McpServerEntry> {
        let mut out: Vec<McpServerEntry> = Vec::new();
        for entry in self
            .sources
            .iter()
            .filter_map(|source| source.provider.mcp_server())
        {
            add_mcp_server(&mut out, entry);
        }
        out
    }

    /// Searches every source (or the one `source` names, by label or kind)
    /// concurrently and merges what they found. A failing source is reported
    /// in `sources`, never fails the search; naming a source that does not
    /// exist does.
    pub async fn search(
        &self,
        query: &SearchQuery,
        source: Option<&str>,
    ) -> Result<MergedSearch, EvidenceError> {
        let wanted: Vec<&BuiltSource> = match source.map(str::trim).filter(|s| !s.is_empty()) {
            None => self.sources.iter().collect(),
            Some(name) => {
                let by_label: Vec<&BuiltSource> = self
                    .sources
                    .iter()
                    .filter(|built| built.provider.label().eq_ignore_ascii_case(name))
                    .collect();
                let chosen = if by_label.is_empty() {
                    self.sources
                        .iter()
                        .filter(|built| built.provider.kind().eq_ignore_ascii_case(name))
                        .collect()
                } else {
                    by_label
                };
                if chosen.is_empty() {
                    return Err(EvidenceError::Invalid(format!(
                        "no evidence source is called {name:?}; the configured sources are: {}",
                        self.label_list()
                    )));
                }
                chosen
            }
        };
        if wanted.is_empty() {
            return Err(EvidenceError::NotConfigured(
                "no evidence source is configured for this board".to_string(),
            ));
        }

        let searches = wanted.iter().map(|built| async move {
            let label = built.provider.label();
            let kind = built.provider.kind().to_string();
            let mut report = SourceReport {
                label: label.clone(),
                kind,
                searched: false,
                hits: 0,
                truncated: false,
                error: None,
                note: None,
            };
            if !built.settings.covers(query.area.as_deref()) {
                report.note = Some(format!(
                    "skipped: covers {}, not {}",
                    built.settings.list("areas").join(", "),
                    query.area.as_deref().unwrap_or_default()
                ));
                return (report, None);
            }
            match built.provider.search(query).await {
                Ok(Some(mut result)) => {
                    for hit in &mut result.hits {
                        hit.source = label.clone();
                        hit.message = secrets::redact(&hit.message);
                    }
                    report.searched = true;
                    report.hits = result.hits.len();
                    report.truncated = result.truncated;
                    (report, Some(result))
                }
                Ok(None) => {
                    report.note = Some(
                        "not searchable as configured; the agent reads it as its description says"
                            .to_string(),
                    );
                    (report, None)
                }
                Err(error) => {
                    tracing::warn!(source = %label, "evidence: search failed: {error}");
                    report.searched = true;
                    report.error = Some(secrets::redact(&error.to_string()));
                    (report, None)
                }
            }
        });
        let outcomes = futures::future::join_all(searches).await;
        let mut sources = Vec::new();
        let mut results = Vec::new();
        for (report, result) in outcomes {
            sources.push(report);
            results.extend(result);
        }
        let merged = merge(results, query.limit);
        Ok(MergedSearch {
            hits: merged.hits,
            truncated: merged.truncated,
            sources,
        })
    }

    fn label_list(&self) -> String {
        let labels: Vec<String> = self
            .sources
            .iter()
            .map(|built| format!("{} ({})", built.provider.label(), built.provider.kind()))
            .collect();
        if labels.is_empty() {
            "none".to_string()
        } else {
            labels.join(", ")
        }
    }
}

/// Adds `entry` to `servers` under a name no other server holds.
pub fn add_mcp_server(servers: &mut Vec<McpServerEntry>, entry: McpServerEntry) {
    if servers
        .iter()
        .any(|existing| existing.server == entry.server)
    {
        return;
    }
    let mut name = entry.name.clone();
    let mut n = 2;
    while servers.iter().any(|existing| existing.name == name) {
        name = format!("{}-{n}", entry.name);
        n += 1;
    }
    servers.push(McpServerEntry {
        name,
        server: entry.server,
    });
}

// ---------------------------------------------------------------------------
// Resolving the configured entries
// ---------------------------------------------------------------------------

fn field_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        // A list (`paths = ["a", "b"]` in a manifest) is one item per line.
        serde_json::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(field_text)
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        other => Some(other.to_string()),
    }
}

/// Resolves one entry: its fields as text, `label` among them, each secret
/// field read through the store with `precedence`.
pub fn resolve_source(
    entry: &EvidenceSourceConfig,
    origin: SourceOrigin,
    store: &Secrets,
    precedence: SecretPrecedence,
) -> SourceSettings {
    let mut fields: BTreeMap<String, String> = entry
        .fields
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), field_text(value)?)))
        .collect();
    match entry.label.as_deref().map(str::trim) {
        Some(label) if !label.is_empty() => {
            fields.insert("label".to_string(), label.to_string());
        }
        _ => {}
    }
    for field in entry.kind.descriptor().fields {
        if field.kind != FieldKind::Secret {
            continue;
        }
        let name = secret_name(entry.kind.as_str(), &field.key);
        match resolve_secret(
            fields.get(&field.key).map(String::as_str),
            &name,
            store,
            precedence,
        ) {
            Some(value) => {
                secrets::register_redaction(&value);
                fields.insert(field.key, value);
            }
            None => {
                fields.remove(&field.key);
            }
        }
    }
    SourceSettings {
        kind: entry.kind,
        origin,
        fields,
    }
}

/// Resolves the entries a scope reads: the config's (store first for global
/// sources, the board's own value first for a board's), then the
/// repository's.
pub fn resolve_sources(
    configured: &[(TargetOrigin, EvidenceSourceConfig)],
    repository: &[EvidenceSourceConfig],
    store: &Secrets,
) -> Vec<SourceSettings> {
    configured
        .iter()
        .map(|(origin, entry)| match origin {
            TargetOrigin::Global => resolve_source(
                entry,
                SourceOrigin::Global,
                store,
                SecretPrecedence::StoreFirst,
            ),
            TargetOrigin::Board => resolve_source(
                entry,
                SourceOrigin::Board,
                store,
                SecretPrecedence::ConfigFirst,
            ),
        })
        .chain(repository.iter().map(|entry| {
            resolve_source(
                entry,
                SourceOrigin::Repository,
                store,
                SecretPrecedence::StoreFirst,
            )
        }))
        .collect()
}

/// Runs a provider's `test()`: against `fields` when given (Settings' unsaved
/// values, which win over the secrets store so what was typed is what is
/// tested), else against the first global source of that kind.
pub async fn test_source(
    config: &Config,
    kind: &str,
    fields: Option<BTreeMap<String, serde_json::Value>>,
) -> Result<String, String> {
    let kind = EvidenceKind::parse(kind).ok_or_else(|| format!("unknown evidence kind: {kind}"))?;
    let (entry, precedence) = match fields {
        Some(mut fields) => {
            let label = fields
                .remove("label")
                .and_then(|value| value.as_str().map(str::to_string));
            (
                EvidenceSourceConfig {
                    kind,
                    label,
                    fields,
                },
                SecretPrecedence::ConfigFirst,
            )
        }
        None => (
            config
                .evidence
                .sources
                .iter()
                .find(|entry| entry.kind == kind)
                .cloned()
                .ok_or_else(|| format!("no {kind} source is configured"))?,
            SecretPrecedence::StoreFirst,
        ),
    };
    let settings = resolve_source(&entry, SourceOrigin::Global, &secrets::load(), precedence);
    let provider = build_provider(&settings, None).map_err(|e| e.to_string())?;
    provider
        .test()
        .await
        .map_err(|e| secrets::redact(&e.to_string()))
}

/// The scope a cached set was built for, and what it was built from.
type Built = HashMap<(Option<Uuid>, Option<PathBuf>), (Vec<SourceSettings>, Arc<EvidenceSet>)>;

/// Hands out the evidence sources the live config describes.
///
/// Built per scope (a board and the repository checkout its relative paths
/// resolve in) and reused; rebuilt when that scope's resolved settings change,
/// secrets included, so a change in Settings or the secrets store applies
/// without a restart.
#[derive(Clone)]
pub struct EvidenceSources {
    config: Arc<RwLock<Config>>,
    built: Arc<std::sync::Mutex<Built>>,
}

impl EvidenceSources {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            built: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// The sources for `board` (the global ones for `None`) plus
    /// `repository`'s own, with relative paths resolving under `root`.
    pub async fn current(
        &self,
        board: Option<Uuid>,
        repository: &[EvidenceSourceConfig],
        root: Option<PathBuf>,
    ) -> Arc<EvidenceSet> {
        let wanted = {
            let config = self.config.read().await;
            resolve_sources(
                &config.evidence_sources(board),
                repository,
                &secrets::load(),
            )
        };
        let mut built = self
            .built
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (board, root.clone());
        if let Some((settings, set)) = built.get(&key)
            && *settings == wanted
        {
            return set.clone();
        }
        let set = Arc::new(build(&wanted, root));
        built.insert(key, (wanted, set.clone()));
        set
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub fn settings(kind: EvidenceKind, fields: &[(&str, &str)]) -> SourceSettings {
        SourceSettings {
            kind,
            origin: SourceOrigin::Global,
            fields: fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    pub fn at(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }

    pub fn query(pattern: &str, from: &str, to: &str, limit: usize) -> SearchQuery {
        SearchQuery {
            pattern: pattern.to_string(),
            window: (at(from), at(to)),
            limit,
            area: None,
            worktree: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{test_support::*, *};

    fn hit(source: &str, at_raw: Option<&str>, message: &str) -> Hit {
        Hit::new(source, at_raw.map(at), message)
    }

    fn brief(label: &str, kind: EvidenceKind, describe: &str) -> SourceBrief {
        SourceBrief {
            label: label.to_string(),
            kind,
            describe: describe.to_string(),
        }
    }

    #[test]
    fn kinds_are_stable_and_every_provider_describes_itself() {
        // `kind` is what configs and manifests store; a rename must show up
        // as a diff here.
        let kinds: Vec<&str> = EvidenceKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            kinds,
            ["sentry", "dataset", "datadog", "cloudwatch", "log_files"]
        );
        for kind in EvidenceKind::ALL {
            assert_eq!(EvidenceKind::parse(kind.as_str()), Some(kind));
            let descriptor = kind.descriptor();
            assert_eq!(descriptor.kind, kind.as_str());
            assert_eq!(descriptor.display_name, kind.display_name());
            assert!(descriptor.summary.ends_with('.'), "{kind}");
            assert!(descriptor.available, "{kind}");
            let keys: Vec<&str> = descriptor.fields.iter().map(|f| f.key.as_str()).collect();
            assert_eq!(keys.first(), Some(&"label"), "{kind}");
            assert_eq!(&keys[keys.len() - 2..], ["areas", "notes"], "{kind}");
            assert!(descriptor.fields.iter().any(|f| f.required), "{kind}");
        }
        let secrets: Vec<Vec<String>> = descriptors().into_iter().map(|d| d.secrets).collect();
        assert_eq!(
            secrets,
            [
                vec!["SENTRY_ACCESS_TOKEN".to_string()],
                vec!["DATASET_READ_TOKEN".to_string()],
                vec![
                    "DATADOG_API_KEY".to_string(),
                    "DATADOG_APPLICATION_KEY".to_string()
                ],
                vec![],
                vec![],
            ]
        );
    }

    #[test]
    fn the_generated_placeholder_with_no_sources_is_empty_or_the_appendix() {
        assert_eq!(render_evidence_tools(&[], ""), "");
        assert_eq!(render_evidence_tools(&[], "  "), "");
        assert_eq!(render_evidence_sources(&[]), "");
        assert_eq!(
            render_evidence_tools(&[], "There is no production log access."),
            "There is no production log access.\n\n\
             If none of these show the failure, the harness is your evidence."
        );
    }

    #[test]
    fn the_generated_placeholder_with_one_source() {
        let briefs = [brief(
            "App log",
            EvidenceKind::LogFiles,
            "App log (Log files): `logs/app.log`.",
        )];
        assert_eq!(
            render_evidence_tools(&briefs, ""),
            "App log (Log files): `logs/app.log`.\n\n\
             If none of these show the failure, the harness is your evidence."
        );
        assert_eq!(render_evidence_sources(&briefs), "- App log");
    }

    #[test]
    fn the_generated_placeholder_with_three_sources_and_an_appendix() {
        let briefs = [
            brief("Sentry", EvidenceKind::Sentry, "Sentry: errors.\n"),
            brief(
                "Web logs",
                EvidenceKind::Datadog,
                "Web logs (Datadog): logs.",
            ),
            brief(
                "App log",
                EvidenceKind::LogFiles,
                "App log (Log files): lines.",
            ),
        ];
        assert_eq!(
            render_evidence_tools(&briefs, "Ask #ops for older logs."),
            "Sentry: errors.\n\n\
             Web logs (Datadog): logs.\n\n\
             App log (Log files): lines.\n\n\
             Ask #ops for older logs.\n\n\
             If none of these show the failure, the harness is your evidence."
        );
        assert_eq!(
            render_evidence_sources(&briefs),
            "- Sentry\n- Web logs\n- App log"
        );
    }

    #[test]
    fn merging_puts_the_newest_first_and_undated_lines_last() {
        let merged = merge(
            vec![
                SearchResult {
                    hits: vec![
                        hit("a", Some("2026-10-18T10:00:00Z"), "a1"),
                        hit("a", None, "a-undated"),
                    ],
                    truncated: false,
                },
                SearchResult {
                    hits: vec![
                        hit("b", Some("2026-10-18T11:00:00Z"), "b1"),
                        hit("b", Some("2026-10-18T09:00:00Z"), "b2"),
                    ],
                    truncated: false,
                },
            ],
            10,
        );
        let order: Vec<&str> = merged.hits.iter().map(|h| h.message.as_str()).collect();
        assert_eq!(order, ["b1", "a1", "b2", "a-undated"]);
        assert!(!merged.truncated);

        let capped = merge(
            vec![SearchResult {
                hits: merged.hits.clone(),
                truncated: false,
            }],
            2,
        );
        assert_eq!(capped.hits.len(), 2);
        assert!(capped.truncated, "dropping hits truncates");

        let passed_on = merge(
            vec![SearchResult {
                hits: vec![],
                truncated: true,
            }],
            2,
        );
        assert!(
            passed_on.truncated,
            "a truncated source truncates the merge"
        );
    }

    #[test]
    fn messages_are_clipped() {
        let long = "x".repeat(MESSAGE_CAP + 10);
        let hit = Hit::new("s", None, &long);
        assert_eq!(hit.message.chars().count(), MESSAGE_CAP);
        assert!(hit.message.ends_with('…'));
    }

    #[test]
    fn sources_cover_their_areas_or_every_area() {
        let open = settings(EvidenceKind::Sentry, &[]);
        assert!(open.covers(Some("Lists")));
        let narrow = settings(EvidenceKind::Sentry, &[("areas", "Sign-in, Sharing")]);
        assert!(narrow.covers(None));
        assert!(narrow.covers(Some("sharing")));
        assert!(!narrow.covers(Some("Lists")));
        assert_eq!(narrow.heading(), "Sentry");
        let labelled = settings(EvidenceKind::Datadog, &[("label", "Web logs")]);
        assert_eq!(labelled.heading(), "Web logs (Datadog)");
        let noted = settings(EvidenceKind::Datadog, &[("notes", "Kept 15 days")]);
        assert_eq!(noted.notes().as_deref(), Some("Kept 15 days."));
    }

    #[test]
    fn entries_resolve_lists_labels_and_secrets() {
        let store = Secrets::load_from(
            Path::new("/nonexistent/prepcook-test-secrets.env"),
            [(
                "PREPCOOK_SECRET_SENTRY_ACCESS_TOKEN".to_string(),
                "from-store".to_string(),
            )],
        );
        let sentry = EvidenceSourceConfig {
            kind: EvidenceKind::Sentry,
            label: Some(" Errors ".to_string()),
            fields: BTreeMap::from([
                ("access_token".to_string(), serde_json::json!("from-config")),
                ("org".to_string(), serde_json::json!("acme")),
            ]),
        };
        let files = EvidenceSourceConfig {
            kind: EvidenceKind::LogFiles,
            label: None,
            fields: BTreeMap::from([(
                "paths".to_string(),
                serde_json::json!(["logs/app.log", "logs/worker-*.log"]),
            )]),
        };
        let resolved = resolve_sources(
            &[
                (TargetOrigin::Global, sentry.clone()),
                (TargetOrigin::Board, sentry),
            ],
            &[files],
            &store,
        );
        assert_eq!(resolved[0].get("access_token"), Some("from-store"));
        assert_eq!(resolved[0].label(), "Errors");
        assert_eq!(resolved[1].get("access_token"), Some("from-config"));
        assert_eq!(resolved[1].origin, SourceOrigin::Board);
        assert_eq!(resolved[2].origin, SourceOrigin::Repository);
        assert_eq!(
            resolved[2].list("paths"),
            ["logs/app.log", "logs/worker-*.log"]
        );
        assert_eq!(resolved[2].label(), "Log files");
    }

    #[test]
    fn mcp_servers_get_unique_names() {
        let mut servers = Vec::new();
        let entry = |token: &str| McpServerEntry {
            name: "sentry".to_string(),
            server: serde_json::json!({ "env": { "TOKEN": token } }),
        };
        add_mcp_server(&mut servers, entry("a"));
        add_mcp_server(&mut servers, entry("a"));
        add_mcp_server(&mut servers, entry("b"));
        add_mcp_server(&mut servers, entry("c"));
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["sentry", "sentry-2", "sentry-3"]);
    }

    #[tokio::test]
    async fn a_merged_search_reads_every_source_reports_each_and_names_unknown_ones() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("logs")).unwrap();
        std::fs::write(
            dir.path().join("logs/app.log"),
            "{\"time\":\"2026-10-18T10:00:00Z\",\"message\":\"SeedMissing list\"}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("logs/worker.log"),
            "2026-10-18T11:00:00Z worker SeedMissing\n",
        )
        .unwrap();
        let set = build(
            &[
                settings(
                    EvidenceKind::LogFiles,
                    &[("label", "App log"), ("paths", "logs/app.log")],
                ),
                settings(
                    EvidenceKind::LogFiles,
                    &[
                        ("label", "Worker log"),
                        ("paths", "logs/worker.log"),
                        ("areas", "Jobs"),
                    ],
                ),
                // Incomplete: skipped at build time, not an error.
                settings(EvidenceKind::Datadog, &[]),
            ],
            Some(dir.path().to_path_buf()),
        );
        assert_eq!(set.sources().len(), 2);
        let query = query(
            "seedmissing",
            "2026-10-18T00:00:00Z",
            "2026-10-18T23:00:00Z",
            10,
        );

        let all = set.search(&query, None).await.unwrap();
        let messages: Vec<(&str, &str)> = all
            .hits
            .iter()
            .map(|h| (h.source.as_str(), h.message.as_str()))
            .collect();
        assert_eq!(
            messages,
            [
                ("Worker log", "2026-10-18T11:00:00Z worker SeedMissing"),
                (
                    "App log",
                    "{\"time\":\"2026-10-18T10:00:00Z\",\"message\":\"SeedMissing list\"}"
                ),
            ]
        );
        assert_eq!(all.sources.len(), 2);
        assert!(all.sources.iter().all(|s| s.searched && s.hits == 1));

        let one = set.search(&query, Some("app log")).await.unwrap();
        assert_eq!(one.hits.len(), 1);
        assert_eq!(one.sources.len(), 1);
        let by_kind = set.search(&query, Some("log_files")).await.unwrap();
        assert_eq!(by_kind.sources.len(), 2);

        let narrowed = set
            .search(
                &SearchQuery {
                    area: Some("Lists".to_string()),
                    ..query.clone()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(narrowed.hits.len(), 1);
        assert!(!narrowed.sources[1].searched);

        let unknown = set.search(&query, Some("Splunk")).await.unwrap_err();
        assert!(
            unknown
                .to_string()
                .contains("App log (log_files), Worker log (log_files)"),
            "{unknown}"
        );

        let empty = build(&[], None);
        assert!(matches!(
            empty.search(&query, None).await,
            Err(EvidenceError::NotConfigured(_))
        ));
    }
}
