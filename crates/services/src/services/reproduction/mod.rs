//! The reproduction seam: where a card's app runs while an agent reproduces a
//! bug against it, and while a branch is verified before it merges.
//!
//! Every caller — the orchestrator's launches and verifications, the Settings
//! Scripts panel, the agents' `app_status` / `app_restart` tools — stands an
//! app up through a [`ReproTarget`], chosen per board (`reproduction.target`
//! in the board's settings, else `app_runs.default_target`):
//!
//! - `local` ([`LocalTarget`]) builds and starts the app from `[app]` in the
//!   card's worktree, on a port from `app_runs.port_range`.
//! - `remote` ([`RemoteTarget`]) runs `[app.remote]` in the worktree to
//!   deploy the branch somewhere reachable (a preview environment, a dev
//!   tenant, a per-branch staging slot) and works against the URL the deploy
//!   reports.
//!
//! Both run the same stages — stand-up, seed, test, teardown — on the shared
//! [`ScriptRunner`] engine, and hand back the same [`AppRun`], so what the
//! harness, the verification and the agent tools see is a URL either way.
//! See `docs/scripts.md`.
//!
//! Adding a target: a file here with a struct implementing [`ReproTarget`]
//! and a `descriptor()`, a variant in [`ReproductionTarget`] and in
//! [`ReproTargetKind`], and its arms in [`descriptor`] and [`build`].

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ts_rs::TS;
use utils::msg_store::MsgStore;
use uuid::Uuid;

pub use self::{local::LocalTarget, remote::RemoteTarget};
pub use crate::services::scripts::RunStatus;
use crate::services::{
    config::Config,
    integrations::Descriptor,
    scripts::{
        AppRun, AppRunStatus, Manifest, RunStep, ScriptRunner, SlotReservation, StandUpOptions,
        StandUpOutcome, StandUpReport, StepReport, load_manifest, runner,
    },
};

pub mod local;
pub mod remote;

/// The integration area name, as the descriptors route and Settings use it.
pub const AREA: &str = "reproduction";

/// Which target a board reproduces against: the config string.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionTarget {
    /// This machine, from `[app]`.
    #[default]
    Local,
    /// A deploy from `[app.remote]`.
    Remote,
}

impl ReproductionTarget {
    pub const ALL: [ReproductionTarget; 2] = [Self::Local, Self::Remote];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }
}

impl std::fmt::Display for ReproductionTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Environment variables, by name.
pub type Env = HashMap<String, String>;

/// What to stand up, and for whom.
#[derive(Default)]
pub struct StandUpRequest {
    /// The worktree (or repository checkout) whose manifest is run.
    pub worktree: PathBuf,
    /// The card's key, as `$PREPCOOK_TICKET_KEY`; may be empty.
    pub ticket_key: String,
    /// The branch `$PREPCOOK_CHANGED_FILES` is computed against.
    pub target_branch: Option<String>,
    /// A slot taken earlier with [`ReproTarget::try_reserve`]; without one,
    /// the target takes its own or answers `Busy`.
    pub reservation: Option<SlotReservation>,
    /// Where the run's output goes; a fresh store when absent.
    pub log: Option<Arc<MsgStore>>,
    pub run_id: Option<Uuid>,
}

impl StandUpRequest {
    pub fn new(worktree: impl Into<PathBuf>) -> Self {
        Self {
            worktree: worktree.into(),
            ..Default::default()
        }
    }

    fn into_parts(self) -> (PathBuf, StandUpOptions) {
        (
            self.worktree,
            StandUpOptions {
                ticket_key: self.ticket_key,
                target_branch: self.target_branch,
                reservation: self.reservation,
                log: self.log,
                run_id: self.run_id,
                preferred_port: None,
            },
        )
    }
}

/// Why a target did not stand an app up.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReproError {
    /// The stand-up ran and did not come to `ok`; the report says which step
    /// and whose fault.
    #[error("the app did not stand up: {}", .0.outcome.label())]
    Run(StandUpReport),
    /// The board's target cannot serve this repository's manifest (a board
    /// set to `remote` whose manifest has no `[app.remote]`). A runner fault:
    /// nothing ran.
    #[error("{0}")]
    Unconfigured(String),
}

impl ReproError {
    /// The report the orchestrator routes on: an unconfigured target is the
    /// runner fault `target-unconfigured`, with the reason as its message.
    pub fn into_report(self) -> StandUpReport {
        match self {
            Self::Run(report) => report,
            Self::Unconfigured(message) => StandUpReport::new(
                StandUpOutcome::TargetUnconfigured,
                Vec::new(),
                Some(message),
            ),
        }
    }
}

/// Where an app runs while it is reproduced against and verified. Every
/// target runs the same stages and hands back an [`AppRun`] whose URL is all
/// a caller needs.
#[async_trait]
#[enum_dispatch(ReproTargetKind)]
pub trait ReproTarget: Send + Sync {
    /// The config string naming this target.
    fn kind(&self) -> &'static str;

    /// Takes one of the target's slots, or `None` when all are in use.
    /// Callers that must not start work they cannot finish take a slot first
    /// and hand it to the stand-up.
    fn try_reserve(&self) -> Option<SlotReservation>;

    /// Stands the app up for `req.worktree`; on success the returned run owns
    /// it until [`Self::tear_down`] or until its last handle is dropped.
    async fn stand_up(&self, req: StandUpRequest) -> Result<AppRun, ReproError>;

    /// Runs the seed again against a stood-up app.
    async fn seed(&self, run: &AppRun) -> StepReport;

    /// Runs the test command against a stood-up app, with
    /// `$PREPCOOK_CHANGED_FILES` set.
    async fn test(&self, run: &AppRun) -> StepReport;

    /// Stand-up, test, teardown: the pre-merge verification.
    async fn verify(&self, req: StandUpRequest) -> StandUpReport;

    /// One step on its own, or the whole stand-up torn down again: the
    /// Settings Scripts panel's test runs.
    async fn run_step(&self, req: StandUpRequest, step: RunStep) -> StandUpReport;

    /// Stops the app and gives back what it held. Always completes; a
    /// second call is a no-op.
    async fn tear_down(&self, run: &AppRun);

    /// Stands a workspace's run up again, keeping its binding to the agent
    /// process that owns it.
    async fn restart(&self, run_id: Uuid) -> Result<(AppRunStatus, StandUpReport), String>;

    /// What an agent session working against `run` receives.
    fn session_env(&self, run: &AppRun) -> Env;

    /// Whether the app answers now.
    async fn status(&self, run: &AppRun) -> RunStatus;
}

/// The built target: one variant per kind.
#[enum_dispatch]
#[derive(Clone)]
pub enum ReproTargetKind {
    Local(LocalTarget),
    Remote(RemoteTarget),
}

impl ReproTargetKind {
    /// Which target this is, as the config names it.
    pub fn target(&self) -> ReproductionTarget {
        match self {
            Self::Local(_) => ReproductionTarget::Local,
            Self::Remote(_) => ReproductionTarget::Remote,
        }
    }
}

/// One target's descriptor.
pub fn descriptor(kind: ReproductionTarget) -> Descriptor {
    match kind {
        ReproductionTarget::Local => LocalTarget::descriptor(),
        ReproductionTarget::Remote => RemoteTarget::descriptor(),
    }
}

/// Every target's descriptor, in [`ReproductionTarget::ALL`] order.
pub fn descriptors() -> Vec<Descriptor> {
    ReproductionTarget::ALL
        .into_iter()
        .map(descriptor)
        .collect()
}

/// Builds the target a board uses for a repository whose manifest is
/// `manifest`. `None` when there is no manifest (nothing to stand up); an
/// error when the board's target is one the manifest does not describe.
pub fn build(
    runner: &ScriptRunner,
    target: ReproductionTarget,
    manifest: Option<&Manifest>,
    root: &Path,
) -> Result<Option<ReproTargetKind>, ReproError> {
    let Some(manifest) = manifest else {
        return Ok(None);
    };
    match target {
        ReproductionTarget::Local => Ok(Some(LocalTarget::new(runner.clone()).into())),
        ReproductionTarget::Remote => match &manifest.app.remote {
            Some(remote) => Ok(Some(
                RemoteTarget::new(runner.clone(), remote.max_parallel as usize).into(),
            )),
            None => Err(ReproError::Unconfigured(runner::remote_missing_message(
                root,
            ))),
        },
    }
}

/// [`build`] from the manifest in `root`. A manifest that is missing or does
/// not parse gives the local target, whose stand-up reports
/// `manifest-missing` or `manifest-invalid` with the file's own error — the
/// same report whichever target the board uses.
pub fn resolve(
    runner: &ScriptRunner,
    target: ReproductionTarget,
    root: &Path,
) -> Result<ReproTargetKind, ReproError> {
    let manifest = load_manifest(root).ok();
    Ok(build(runner, target, manifest.as_ref(), root)?
        .unwrap_or_else(|| LocalTarget::new(runner.clone()).into()))
}

/// The target that stood `run` up, for operations on a run that already
/// exists (teardown, restart, status). Its slot limit is not consulted.
pub fn for_run(runner: &ScriptRunner, run: &AppRun) -> ReproTargetKind {
    for_kind(runner, run.target_kind)
}

/// A target of `kind` for operations on runs that already exist.
pub fn for_kind(runner: &ScriptRunner, kind: ReproductionTarget) -> ReproTargetKind {
    match kind {
        ReproductionTarget::Local => LocalTarget::new(runner.clone()).into(),
        ReproductionTarget::Remote => RemoteTarget::new(runner.clone(), 1).into(),
    }
}

/// Runs a target's connection test: `fields` are Settings' unsaved values
/// over the saved `app_runs` section.
pub async fn test_target(
    config: &Config,
    kind: &str,
    fields: Option<BTreeMap<String, serde_json::Value>>,
) -> Result<String, String> {
    let kind = ReproductionTarget::parse(kind)
        .ok_or_else(|| format!("unknown reproduction kind: {kind}"))?;
    let mut section = serde_json::to_value(&config.app_runs).map_err(|e| e.to_string())?;
    if let (Some(object), Some(fields)) = (section.as_object_mut(), fields) {
        object.extend(fields);
    }
    let app_runs = serde_json::from_value(section)
        .map_err(|e| format!("the settings do not read as an app_runs section: {e}"))?;
    match kind {
        ReproductionTarget::Local => LocalTarget::test(&app_runs),
        ReproductionTarget::Remote => RemoteTarget::test(&app_runs),
    }
}

/// Hands out the target a board uses, against the live config.
///
/// Targets are cheap views over the shared runner, so they are built per
/// call; what is kept is the runner's settings, which follow `app_runs` when
/// it changes (see [`ScriptRunner::sync_settings`]).
#[derive(Clone)]
pub struct ReproductionSource {
    config: Arc<RwLock<Config>>,
    runner: ScriptRunner,
}

impl ReproductionSource {
    pub fn new(config: Arc<RwLock<Config>>, runner: ScriptRunner) -> Self {
        Self { config, runner }
    }

    pub fn runner(&self) -> &ScriptRunner {
        &self.runner
    }

    /// The target `board` (the global default for `None`) uses for the
    /// repository checked out at `root`.
    pub async fn current(
        &self,
        board: Option<Uuid>,
        root: &Path,
    ) -> Result<ReproTargetKind, ReproError> {
        let target = {
            let config = self.config.read().await;
            self.sync(&config);
            config.reproduction_target(board)
        };
        resolve(&self.runner, target, root)
    }

    /// The target for a checkout of one of `repo_ids`: the first board that
    /// lists one of them decides, else the global default.
    pub async fn for_repos(
        &self,
        repo_ids: &[Uuid],
        root: &Path,
    ) -> Result<ReproTargetKind, ReproError> {
        let target = {
            let config = self.config.read().await;
            self.sync(&config);
            config.reproduction_target_for_repos(repo_ids)
        };
        resolve(&self.runner, target, root)
    }

    /// The target kind those repositories' board uses, without building it.
    pub async fn target_for_repos(&self, repo_ids: &[Uuid]) -> ReproductionTarget {
        self.config
            .read()
            .await
            .reproduction_target_for_repos(repo_ids)
    }

    fn sync(&self, config: &Config) {
        self.runner
            .sync_settings(&config.app_runs.port_range, config.app_runs.max_parallel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(raw: &str) -> Manifest {
        Manifest::parse(raw, Path::new("/repo/.prepcook/scripts.toml")).unwrap()
    }

    #[test]
    fn descriptors_cover_both_targets() {
        let all = descriptors();
        let kinds: Vec<&str> = all.iter().map(|d| d.kind.as_str()).collect();
        assert_eq!(kinds, ["local", "remote"]);
        let local = &all[0];
        let keys: Vec<&str> = local.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["port_range", "max_parallel"]);
        // Every local field is a key of the `app_runs` section.
        let section =
            serde_json::to_value(crate::services::config::AppRunsConfig::default()).unwrap();
        for field in &local.fields {
            assert!(section.get(&field.key).is_some(), "{}", field.key);
        }
        // Remote targets are configured in each repository's manifest.
        let remote = &all[1];
        assert!(remote.fields.is_empty());
        assert!(remote.summary.contains("[app.remote]"));
        for descriptor in &all {
            assert!(descriptor.secrets.is_empty());
            assert!(descriptor.available);
            assert!(descriptor.docs_url.contains("scripts.md"));
        }
    }

    #[test]
    fn build_follows_the_board_and_refuses_remote_without_a_table() {
        let runner = ScriptRunner::default();
        let root = Path::new("/repo");
        let local_only = manifest("[app]\nstart = \"x\"\n");
        let with_remote = manifest(
            "[app]\nstart = \"x\"\n[app.remote]\nstart = \"./deploy.sh\"\nmax_parallel = 3\n",
        );

        assert!(
            build(&runner, ReproductionTarget::Remote, None, root)
                .unwrap()
                .is_none(),
            "no manifest, nothing to build"
        );
        let local = build(&runner, ReproductionTarget::Local, Some(&with_remote), root)
            .unwrap()
            .unwrap();
        assert_eq!(local.kind(), "local");
        let remote = build(
            &runner,
            ReproductionTarget::Remote,
            Some(&with_remote),
            root,
        )
        .unwrap()
        .unwrap();
        assert_eq!(remote.target(), ReproductionTarget::Remote);

        let error = build(&runner, ReproductionTarget::Remote, Some(&local_only), root)
            .err()
            .expect("remote without [app.remote] is refused");
        assert!(error.to_string().contains("[app.remote]"), "{error}");
        let report = error.into_report();
        assert_eq!(report.outcome, StandUpOutcome::TargetUnconfigured);
        assert!(report.outcome.is_runner_fault());
        assert!(report.evidence().contains("set the board's reproduction"));
    }

    #[test]
    fn a_missing_manifest_resolves_to_local_so_the_stand_up_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let target = resolve(
            &ScriptRunner::default(),
            ReproductionTarget::Remote,
            dir.path(),
        )
        .unwrap();
        assert_eq!(target.target(), ReproductionTarget::Local);
    }

    #[tokio::test]
    async fn the_connection_tests_read_unsaved_fields() {
        let config = Config::default();
        let ok = test_target(&config, "local", None).await.unwrap();
        assert!(ok.contains("4200-4299"), "{ok}");
        let bad = test_target(
            &config,
            "local",
            Some(BTreeMap::from([(
                "port_range".to_string(),
                serde_json::json!("9-1"),
            )])),
        )
        .await
        .unwrap_err();
        assert!(bad.contains("9-1"), "{bad}");
        assert!(
            test_target(&config, "remote", None)
                .await
                .unwrap()
                .contains("[app.remote]")
        );
        assert!(test_target(&config, "elsewhere", None).await.is_err());
    }

    #[tokio::test]
    async fn the_source_picks_each_boards_target() {
        let board = Uuid::new_v4();
        let repo = Uuid::new_v4();
        let mut config = Config::default();
        let defaults = config
            .orchestrator
            .project_defaults
            .entry(board.to_string())
            .or_default();
        defaults.repo_ids = vec![repo];
        defaults.reproduction.target = Some(ReproductionTarget::Remote);
        let source =
            ReproductionSource::new(Arc::new(RwLock::new(config)), ScriptRunner::default());
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".prepcook")).unwrap();
        std::fs::write(
            dir.path().join(".prepcook/scripts.toml"),
            "[app]\nstart = \"x\"\n[app.remote]\nstart = \"./deploy.sh\"\n",
        )
        .unwrap();

        let remote = source.current(Some(board), dir.path()).await.unwrap();
        assert_eq!(remote.target(), ReproductionTarget::Remote);
        let other = source.current(None, dir.path()).await.unwrap();
        assert_eq!(other.target(), ReproductionTarget::Local);
        let by_repo = source.for_repos(&[repo], dir.path()).await.unwrap();
        assert_eq!(by_repo.target(), ReproductionTarget::Remote);
        assert_eq!(
            source.target_for_repos(&[Uuid::new_v4()]).await,
            ReproductionTarget::Local
        );
    }
}
