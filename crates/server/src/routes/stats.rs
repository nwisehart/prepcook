//! Read-only progress numbers for the orchestrator's boards: how many merge
//! requests it opened and how many reached their target, and what the RCA
//! knowledge store says. The figures the monthly all-hands deck quotes,
//! computed from the same records the orchestrator works from instead of by
//! hand.
//!
//! - `GET /api/stats` — everything below, plus the running version
//! - `GET /api/stats/merge-requests`
//! - `GET /api/stats/rcas`
//!
//! Each takes `?since=YYYY-MM-DD` (UTC, inclusive) to count only what was
//! created on or after that day, and `?project_id=` to narrow to one board;
//! without it every board in Settings › Orchestrator is counted.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::OnceLock,
    time::{Duration, Instant},
};

use api_types::{
    KbDocumentSummary, KbEdge, KbNode, PullRequest, PullRequestStatus, SearchIssuesRequest,
};
use axum::{
    Router,
    extract::{Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use chrono::{DateTime, NaiveDate, Utc};
use deployment::Deployment;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

/// Listing merge requests costs one remote call per issue, so the raw list is
/// kept this long. Counts are recomputed from it on every request, so a
/// different `since` never waits on the cache.
const PR_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const ISSUE_PAGE: i32 = 200;
const PR_LOOKUPS_IN_FLIGHT: usize = 12;

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/stats", get(get_stats))
        .route("/stats/merge-requests", get(get_merge_request_stats))
        .route("/stats/rcas", get(get_rca_stats))
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    /// Inclusive lower bound, YYYY-MM-DD (UTC creation date).
    pub since: Option<String>,
    /// One board; default every configured board.
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub version: &'static str,
    pub generated_at: DateTime<Utc>,
    pub since: Option<NaiveDate>,
    pub merge_requests: MergeRequestStats,
    pub rcas: RcaStats,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct MergeRequestCounts {
    /// Every merge request the orchestrator opened: merged + closed + open.
    pub opened: u32,
    pub merged: u32,
    /// Closed without merging.
    pub closed: u32,
    /// Still open.
    pub open: u32,
    /// merged / opened, as a whole percentage; None when nothing was opened.
    pub merge_rate_percent: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoMergeRequestCounts {
    /// `group/project`, read from the merge request's URL.
    pub repo: String,
    #[serde(flatten)]
    pub counts: MergeRequestCounts,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct MergeRequestStats {
    pub total: MergeRequestCounts,
    /// Most merged first.
    pub by_repo: Vec<RepoMergeRequestCounts>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct RcaStats {
    pub total: u32,
    /// `origin:` tag values (regression, dormant); `untagged` for neither.
    pub by_origin: BTreeMap<String, u32>,
    /// `culprit:` tag values on regression RCAs only.
    pub regressions_by_culprit: BTreeMap<String, u32>,
    /// `severity:` tag values (p0, p1, …).
    pub by_severity: BTreeMap<String, u32>,
    /// `detectable-by:` tag values. A document can carry several.
    pub detectable_by: BTreeMap<String, u32>,
    /// Root-cause-class graph nodes with the RCAs in range linked to them,
    /// most first. Classes with none in range are left out.
    pub root_cause_classes: Vec<RootCauseClassCount>,
    /// Investigation stories filed, over all time: one per class that crossed
    /// the recurrence threshold.
    pub investigations_filed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootCauseClassCount {
    pub name: String,
    pub count: u32,
    pub investigation_jira_key: Option<String>,
}

async fn get_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<StatsQuery>,
) -> Result<ResponseJson<ApiResponse<StatsResponse>>, ApiError> {
    let since = parse_since(&query.since)?;
    let projects = board_ids(&deployment, query.project_id).await?;
    let merge_requests =
        tally_merge_requests(&load_pull_requests(&deployment, &projects).await?, since);
    let rcas = load_rca_stats(&deployment, &projects, since).await?;
    Ok(ResponseJson(ApiResponse::success(StatsResponse {
        version: utils::version::APP_VERSION,
        generated_at: Utc::now(),
        since,
        merge_requests,
        rcas,
    })))
}

async fn get_merge_request_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<StatsQuery>,
) -> Result<ResponseJson<ApiResponse<MergeRequestStats>>, ApiError> {
    let since = parse_since(&query.since)?;
    let projects = board_ids(&deployment, query.project_id).await?;
    let pull_requests = load_pull_requests(&deployment, &projects).await?;
    Ok(ResponseJson(ApiResponse::success(tally_merge_requests(
        &pull_requests,
        since,
    ))))
}

async fn get_rca_stats(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<StatsQuery>,
) -> Result<ResponseJson<ApiResponse<RcaStats>>, ApiError> {
    let since = parse_since(&query.since)?;
    let projects = board_ids(&deployment, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(
        load_rca_stats(&deployment, &projects, since).await?,
    )))
}

fn parse_since(value: &Option<String>) -> Result<Option<NaiveDate>, ApiError> {
    match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(day) => NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| ApiError::BadRequest(format!("since {day:?} is not YYYY-MM-DD"))),
    }
}

/// The boards to count: the one asked for, else every configured board.
async fn board_ids(
    deployment: &DeploymentImpl,
    requested: Option<Uuid>,
) -> Result<Vec<Uuid>, ApiError> {
    if let Some(project_id) = requested {
        return Ok(vec![project_id]);
    }
    let config = deployment.config().read().await;
    let mut ids: Vec<Uuid> = config
        .orchestrator
        .project_defaults
        .keys()
        .filter_map(|key| Uuid::parse_str(key).ok())
        .collect();
    ids.sort();
    if ids.is_empty() {
        return Err(ApiError::BadRequest(
            "No projects configured in Settings > Orchestrator".to_string(),
        ));
    }
    Ok(ids)
}

type PrCache = Mutex<HashMap<Vec<Uuid>, (Instant, Vec<PullRequest>)>>;

fn pr_cache() -> &'static PrCache {
    static CACHE: OnceLock<PrCache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Every merge request linked to any issue on these boards, once each.
async fn load_pull_requests(
    deployment: &DeploymentImpl,
    projects: &[Uuid],
) -> Result<Vec<PullRequest>, ApiError> {
    if let Some((at, cached)) = pr_cache().lock().await.get(projects)
        && at.elapsed() < PR_CACHE_TTL
    {
        return Ok(cached.clone());
    }

    let client = deployment.remote_client()?;
    let mut issue_ids = Vec::new();
    for &project_id in projects {
        let mut offset = 0;
        loop {
            let page = client
                .search_issues(&SearchIssuesRequest {
                    project_id,
                    status_id: None,
                    status_ids: None,
                    priority: None,
                    parent_issue_id: None,
                    search: None,
                    simple_id: None,
                    assignee_user_id: None,
                    tag_id: None,
                    tag_ids: None,
                    sort_field: None,
                    sort_direction: None,
                    limit: Some(ISSUE_PAGE),
                    offset: Some(offset),
                })
                .await?;
            let fetched = page.issues.len() as i32;
            issue_ids.extend(page.issues.into_iter().map(|issue| issue.id));
            if fetched < ISSUE_PAGE {
                break;
            }
            offset += ISSUE_PAGE;
        }
    }

    let client = &client;
    let lookups: Vec<_> = stream::iter(issue_ids)
        .map(|issue_id| async move { client.list_pull_requests(issue_id).await })
        .buffer_unordered(PR_LOOKUPS_IN_FLIGHT)
        .collect()
        .await;

    // A merge request linked to two issues is still one merge request.
    let mut seen = HashSet::new();
    let mut pull_requests = Vec::new();
    for lookup in lookups {
        for pr in lookup?.pull_requests {
            if seen.insert(pr.id) {
                pull_requests.push(pr);
            }
        }
    }

    pr_cache()
        .lock()
        .await
        .insert(projects.to_vec(), (Instant::now(), pull_requests.clone()));
    Ok(pull_requests)
}

async fn load_rca_stats(
    deployment: &DeploymentImpl,
    projects: &[Uuid],
    since: Option<NaiveDate>,
) -> Result<RcaStats, ApiError> {
    let client = deployment.remote_client()?;
    let mut documents = Vec::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for &project_id in projects {
        documents.extend(
            client
                .list_kb_documents(project_id, Some("rca"), false)
                .await?
                .documents,
        );
        nodes.extend(client.list_kb_nodes(project_id).await?.nodes);
        edges.extend(client.list_kb_edges(project_id, None, None).await?.edges);
    }
    Ok(tally_rcas(&documents, &nodes, &edges, since))
}

fn in_range(created_at: &DateTime<Utc>, since: Option<NaiveDate>) -> bool {
    since.is_none_or(|day| created_at.date_naive() >= day)
}

/// `group/project` from a GitLab or GitHub merge request URL; the URL itself
/// when it has neither shape.
fn repo_of(url: &str) -> String {
    git_host::parse_pr_url(url)
        .map(|(repo, _)| repo.path())
        .unwrap_or_else(|_| url.to_string())
}

fn counts_of<'a>(pull_requests: impl Iterator<Item = &'a PullRequest>) -> MergeRequestCounts {
    let mut counts = MergeRequestCounts::default();
    for pr in pull_requests {
        counts.opened += 1;
        match pr.status {
            PullRequestStatus::Merged => counts.merged += 1,
            PullRequestStatus::Closed => counts.closed += 1,
            PullRequestStatus::Open => counts.open += 1,
        }
    }
    counts.merge_rate_percent = (counts.opened > 0)
        .then(|| ((counts.merged as f64 / counts.opened as f64) * 100.0).round() as u32);
    counts
}

/// Counts merge requests opened on or after `since`, and what became of them.
fn tally_merge_requests(
    pull_requests: &[PullRequest],
    since: Option<NaiveDate>,
) -> MergeRequestStats {
    let in_window: Vec<&PullRequest> = pull_requests
        .iter()
        .filter(|pr| in_range(&pr.created_at, since))
        .collect();

    let mut by_repo: BTreeMap<String, Vec<&PullRequest>> = BTreeMap::new();
    for pr in &in_window {
        by_repo.entry(repo_of(&pr.url)).or_default().push(pr);
    }
    let mut by_repo: Vec<RepoMergeRequestCounts> = by_repo
        .into_iter()
        .map(|(repo, prs)| RepoMergeRequestCounts {
            repo,
            counts: counts_of(prs.into_iter()),
        })
        .collect();
    by_repo.sort_by(|a, b| {
        b.counts
            .merged
            .cmp(&a.counts.merged)
            .then(a.repo.cmp(&b.repo))
    });

    MergeRequestStats {
        total: counts_of(in_window.into_iter()),
        by_repo,
    }
}

fn tag_values<'a>(document: &'a KbDocumentSummary, namespace: &str) -> Vec<&'a str> {
    document
        .tags
        .iter()
        .filter_map(|tag| tag.split_once(':'))
        .filter(|(ns, value)| *ns == namespace && !value.is_empty())
        .map(|(_, value)| value)
        .collect()
}

fn tally_rcas(
    documents: &[KbDocumentSummary],
    nodes: &[KbNode],
    edges: &[KbEdge],
    since: Option<NaiveDate>,
) -> RcaStats {
    let documents: Vec<&KbDocumentSummary> = documents
        .iter()
        .filter(|doc| doc.kind == "rca" && in_range(&doc.created_at, since))
        .collect();
    let mut stats = RcaStats {
        total: documents.len() as u32,
        ..Default::default()
    };

    for doc in &documents {
        let origins = tag_values(doc, "origin");
        if origins.is_empty() {
            *stats.by_origin.entry("untagged".to_string()).or_default() += 1;
        }
        for origin in &origins {
            *stats.by_origin.entry((*origin).to_string()).or_default() += 1;
        }
        if origins.contains(&"regression") {
            let culprits = tag_values(doc, "culprit");
            if culprits.is_empty() {
                *stats
                    .regressions_by_culprit
                    .entry("unknown".to_string())
                    .or_default() += 1;
            }
            for culprit in culprits {
                *stats
                    .regressions_by_culprit
                    .entry(culprit.to_string())
                    .or_default() += 1;
            }
        }
        for severity in tag_values(doc, "severity") {
            *stats.by_severity.entry(severity.to_string()).or_default() += 1;
        }
        for channel in tag_values(doc, "detectable-by") {
            *stats.detectable_by.entry(channel.to_string()).or_default() += 1;
        }
    }

    // Counted from the edges rather than the node's own edge_count, so `since`
    // applies and non-RCA documents linked to a node are not counted.
    let in_scope: HashSet<Uuid> = documents.iter().map(|doc| doc.id).collect();
    let classes: HashMap<Uuid, &KbNode> = nodes
        .iter()
        .filter(|node| node.kind == "root_cause_class")
        .map(|node| (node.id, node))
        .collect();
    let mut linked: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
    for edge in edges {
        if classes.contains_key(&edge.node_id) && in_scope.contains(&edge.document_id) {
            linked
                .entry(edge.node_id)
                .or_default()
                .insert(edge.document_id);
        }
    }
    stats.root_cause_classes = linked
        .into_iter()
        .map(|(node_id, docs)| {
            let node = classes[&node_id];
            RootCauseClassCount {
                name: node.name.clone(),
                count: docs.len() as u32,
                investigation_jira_key: node.investigation_jira_key.clone(),
            }
        })
        .collect();
    stats
        .root_cause_classes
        .sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    stats.investigations_filed = classes
        .values()
        .filter(|node| node.investigation_jira_key.is_some())
        .count() as u32;
    stats
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, day, 12, 0, 0).unwrap()
    }

    #[allow(deprecated)]
    fn pr(url: &str, status: PullRequestStatus, day: u32) -> PullRequest {
        PullRequest {
            id: Uuid::new_v4(),
            url: url.to_string(),
            number: 1,
            status,
            merged_at: None,
            merge_commit_sha: None,
            target_branch_name: "master".to_string(),
            project_id: Uuid::nil(),
            issue_id: Uuid::nil(),
            workspace_id: None,
            created_at: at(day),
            updated_at: at(day),
        }
    }

    fn doc(tags: &[&str], day: u32) -> KbDocumentSummary {
        KbDocumentSummary {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            kind: "rca".to_string(),
            title: String::new(),
            issue_id: None,
            jira_key: None,
            work_class: None,
            summary: None,
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            created_at: at(day),
            updated_at: at(day),
        }
    }

    fn class(name: &str, investigation: Option<&str>) -> KbNode {
        KbNode {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            kind: "root_cause_class".to_string(),
            name: name.to_string(),
            description: None,
            seeded: false,
            investigation_jira_key: investigation.map(str::to_string),
            investigation_edge_count: None,
            edge_count: 0,
            created_at: at(1),
        }
    }

    fn edge(node: &KbNode, document: &KbDocumentSummary) -> KbEdge {
        KbEdge {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            node_id: node.id,
            document_id: document.id,
            rationale: None,
            confidence: None,
            created_at: at(1),
        }
    }

    #[test]
    fn a_repo_is_read_from_the_merge_request_url() {
        assert_eq!(
            repo_of("https://gitlab.com/example-org/example-app/-/merge_requests/10069"),
            "example-org/example-app"
        );
        assert_eq!(
            repo_of("https://github.com/acme/widgets/pull/7"),
            "acme/widgets"
        );
        assert_eq!(repo_of("not a url"), "not a url");
    }

    #[test]
    fn merge_requests_are_counted_by_repo_and_window() {
        let app = "https://gitlab.com/example-org/example-app/-/merge_requests/1";
        let other = "https://gitlab.com/example-org/other/-/merge_requests/2";
        let prs = vec![
            pr(app, PullRequestStatus::Merged, 10),
            pr(app, PullRequestStatus::Merged, 25),
            pr(app, PullRequestStatus::Closed, 26),
            pr(app, PullRequestStatus::Open, 27),
            pr(other, PullRequestStatus::Merged, 28),
        ];

        let all = tally_merge_requests(&prs, None);
        assert_eq!(all.total.opened, 5);
        assert_eq!(all.total.merged, 3);
        assert_eq!(all.total.merge_rate_percent, Some(60));
        assert_eq!(all.by_repo[0].repo, "example-org/example-app");
        assert_eq!(all.by_repo[0].counts.opened, 4);

        let since = NaiveDate::from_ymd_opt(2026, 9, 24);
        let recent = tally_merge_requests(&prs, since);
        assert_eq!(recent.total.opened, 4);
        assert_eq!(recent.total.merged, 2);
        assert_eq!(recent.total.closed, 1);
        assert_eq!(recent.total.open, 1);

        assert_eq!(
            tally_merge_requests(&[], None).total.merge_rate_percent,
            None
        );
    }

    #[test]
    fn rcas_are_counted_by_tag_and_class() {
        let a = doc(
            &[
                "origin:regression",
                "culprit:human",
                "severity:p0",
                "detectable-by:automated-tests",
            ],
            10,
        );
        let b = doc(
            &[
                "origin:regression",
                "severity:p1",
                "detectable-by:automated-tests",
                "detectable-by:code-review",
            ],
            25,
        );
        let c = doc(&["origin:dormant", "severity:p1"], 26);
        let untagged = doc(&[], 27);
        let mut note = doc(&["origin:regression"], 27);
        note.kind = "note".to_string();

        let sibling = class(
            "Invariant enforced on one path but not a sibling",
            Some("BUG-10630"),
        );
        let fallback = class("Failure replaced with a benign fallback", None);
        let edges = vec![
            edge(&sibling, &a),
            edge(&sibling, &b),
            edge(&sibling, &b), // a duplicate edge counts the document once
            edge(&fallback, &c),
            edge(&fallback, &note), // not an RCA
        ];
        let docs = vec![a, b, c, untagged, note];
        let nodes = vec![sibling, fallback];

        let all = tally_rcas(&docs, &nodes, &edges, None);
        assert_eq!(all.total, 4);
        assert_eq!(all.by_origin["regression"], 2);
        assert_eq!(all.by_origin["dormant"], 1);
        assert_eq!(all.by_origin["untagged"], 1);
        assert_eq!(all.regressions_by_culprit["human"], 1);
        assert_eq!(all.regressions_by_culprit["unknown"], 1);
        assert_eq!(all.by_severity["p1"], 2);
        assert_eq!(all.detectable_by["automated-tests"], 2);
        assert_eq!(all.root_cause_classes[0].count, 2);
        assert_eq!(
            all.root_cause_classes[0].investigation_jira_key.as_deref(),
            Some("BUG-10630")
        );
        assert_eq!(all.root_cause_classes[1].count, 1);
        assert_eq!(all.investigations_filed, 1);

        let recent = tally_rcas(&docs, &nodes, &edges, NaiveDate::from_ymd_opt(2026, 9, 24));
        assert_eq!(recent.total, 3);
        assert_eq!(recent.root_cause_classes[0].count, 1);
    }

    #[test]
    fn since_must_be_a_date() {
        assert_eq!(parse_since(&None).unwrap(), None);
        assert_eq!(parse_since(&Some(" ".to_string())).unwrap(), None);
        assert!(parse_since(&Some("24 Sep".to_string())).is_err());
    }
}
