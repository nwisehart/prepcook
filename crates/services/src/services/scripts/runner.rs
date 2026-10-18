//! Runs a repository's `.prepcook/scripts.toml` in a worktree: stands the app up,
//! tests it, and tears it down. This is the engine behind both reproduction
//! targets (see [`crate::services::reproduction`]): `local` serves the app on
//! a port this process allocates, `remote` runs `[app.remote]` to deploy it
//! and works against the URL the deploy reports.
//!
//! A stood-up app is an [`AppRun`]. A local app's start command runs in its
//! own process group, so tearing it down kills everything the command
//! spawned; a remote app is torn down by its `stop` command. Teardown is
//! explicit and also happens when the last handle to a run is dropped, so a
//! run that a caller forgets cannot hold a port, a slot, a process or a
//! deployed environment past its owner.
//!
//! Local runs are capped at `app_runs.max_parallel` and remote runs at
//! `[app.remote] max_parallel`, each with its own slots. A caller that cannot
//! get a slot gets [`StandUpOutcome::Busy`] immediately rather than a queue:
//! the orchestrator leaves the card in its column and asks again on its next
//! tick.
//!
//! Every line a script prints goes to the run's [`MsgStore`] with known secret
//! values redacted, which is what lets the execution-process log routes stream
//! a run's output unchanged.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    ops::RangeInclusive,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex, RwLock, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use command_group::AsyncGroupChild;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tracing::{debug, info, warn};
use ts_rs::TS;
use utils::{
    command_ext::GroupSpawnNoWindowExt, log_msg::LogMsg, msg_store::MsgStore,
    shell::get_shell_command,
};
use uuid::Uuid;

use super::{
    manifest::{self, Manifest, ManifestError, ReadyCheck, StepTimeouts},
    ports::{self, PortAllocator},
};
use crate::services::{
    reproduction::{ReproTarget, ReproTargetKind, ReproductionTarget, StandUpRequest},
    secrets::{self, Secrets},
};

/// Lines kept for a step's `log_tail`.
const LOG_TAIL_LINES: usize = 60;
/// Finished script runs kept for `GET /api/script-runs/{id}`.
const SCRIPT_RUN_HISTORY: usize = 50;
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Bound on reading what a finished command left in its pipes. A command that
/// backgrounds a child holding stdout open would otherwise hold the step.
const PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Bound on one ready probe made for a status check.
const STATUS_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// The `$PREPCOOK_ENV_OUT` line a remote deploy reports its address in.
const APP_URL_VAR: &str = "PREPCOOK_APP_URL";
/// The `$PREPCOOK_ENV_OUT` line a remote deploy may name what it deployed in
/// (an environment name, a deployment id); handed back to `stop` and
/// `restart` as `$PREPCOOK_TARGET_REF`.
const TARGET_REF_VAR: &str = "PREPCOOK_TARGET_REF";

/// One step of the manifest's `[app]` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    Setup,
    Build,
    Start,
    Ready,
    Seed,
    Test,
    Stop,
}

impl Step {
    pub fn label(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::Build => "build",
            Self::Start => "start",
            Self::Ready => "ready",
            Self::Seed => "seed",
            Self::Test => "test",
            Self::Stop => "stop",
        }
    }
}

/// What the Settings "Test run" button can ask for: one step, or the whole
/// stand-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum RunStep {
    Setup,
    Build,
    Start,
    Ready,
    Seed,
    Test,
    Stop,
    StandUp,
}

/// How one step went.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StepReport {
    pub step: Step,
    /// `None` when the command was killed (timeout, signal) or never started.
    pub exit: Option<i32>,
    pub timed_out: bool,
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// The last 60 lines the step printed, secrets redacted.
    pub log_tail: String,
}

impl StepReport {
    pub fn succeeded(&self) -> bool {
        !self.timed_out && self.exit == Some(0)
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms)
    }

    fn note(step: Step, text: impl Into<String>) -> Self {
        Self {
            step,
            exit: Some(0),
            timed_out: false,
            duration_ms: 0,
            log_tail: text.into(),
        }
    }
}

/// What a stand-up (or a verify, or a single step) came to.
///
/// The faults split two ways, and the split is what the orchestrator routes
/// on: [`Self::is_branch_fault`] means the code in the worktree is the problem
/// and the agent that wrote it can act on the evidence;
/// [`Self::is_runner_fault`] means the setup is the problem — no manifest, a
/// secret not set, no free port, a board set to a target the manifest does
/// not describe — and a person has to act.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum StandUpOutcome {
    Ok,
    ManifestMissing,
    ManifestInvalid,
    /// The board's reproduction target is one the manifest has no table for
    /// (`remote` without `[app.remote]`).
    TargetUnconfigured,
    SetupFailed,
    BuildFailed,
    StartFailed,
    NotReady,
    SeedFailed,
    TestFailed,
    PortUnavailable,
    /// The names of the required secrets with no value.
    SecretMissing(String),
    /// Every slot is taken; nothing ran.
    Busy,
    Error(String),
}

impl StandUpOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    pub fn is_branch_fault(&self) -> bool {
        matches!(
            self,
            Self::SetupFailed
                | Self::BuildFailed
                | Self::StartFailed
                | Self::NotReady
                | Self::SeedFailed
                | Self::TestFailed
        )
    }

    pub fn is_runner_fault(&self) -> bool {
        matches!(
            self,
            Self::ManifestMissing
                | Self::ManifestInvalid
                | Self::TargetUnconfigured
                | Self::PortUnavailable
                | Self::SecretMissing(_)
                | Self::Error(_)
        )
    }

    /// The kebab-case word recorded on a card.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ManifestMissing => "manifest-missing",
            Self::ManifestInvalid => "manifest-invalid",
            Self::TargetUnconfigured => "target-unconfigured",
            Self::SetupFailed => "setup-failed",
            Self::BuildFailed => "build-failed",
            Self::StartFailed => "start-failed",
            Self::NotReady => "not-ready",
            Self::SeedFailed => "seed-failed",
            Self::TestFailed => "test-failed",
            Self::PortUnavailable => "port-unavailable",
            Self::SecretMissing(_) => "secret-missing",
            Self::Busy => "busy",
            Self::Error(_) => "error",
        }
    }

    /// Whether a recorded label names a runner fault or a busy runner — the
    /// durable record on a card keeps the word, not the variant.
    pub fn is_runner_fault_label(label: &str) -> bool {
        matches!(
            label,
            "manifest-missing"
                | "manifest-invalid"
                | "target-unconfigured"
                | "port-unavailable"
                | "secret-missing"
                | "busy"
                | "error"
        )
    }

    fn failed_step(step: Step) -> Self {
        match step {
            Step::Setup => Self::SetupFailed,
            Step::Build => Self::BuildFailed,
            Step::Start => Self::StartFailed,
            Step::Ready => Self::NotReady,
            Step::Seed => Self::SeedFailed,
            Step::Test => Self::TestFailed,
            Step::Stop => Self::Error("the stop script failed".to_string()),
        }
    }
}

/// Every step that ran, and what it came to.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StandUpReport {
    pub steps: Vec<StepReport>,
    pub outcome: StandUpOutcome,
    /// A sentence for a person when the outcome needs one (a manifest error,
    /// the secrets that are missing).
    pub message: Option<String>,
}

impl StandUpReport {
    pub(crate) fn new(
        outcome: StandUpOutcome,
        steps: Vec<StepReport>,
        message: Option<String>,
    ) -> Self {
        Self {
            steps,
            outcome,
            message,
        }
    }

    /// The step the outcome is about: the last one that ran.
    pub fn last_step(&self) -> Option<&StepReport> {
        self.steps.last()
    }

    /// The last step's log tail, already redacted.
    pub fn log_tail(&self) -> &str {
        self.last_step().map(|s| s.log_tail.as_str()).unwrap_or("")
    }

    /// The message and the last step's output, fenced for a prompt or a
    /// comment. Bounded by the tail, so a runaway log cannot become a runaway
    /// prompt.
    pub fn evidence(&self) -> String {
        let mut out = String::new();
        if let Some(message) = &self.message {
            out.push_str(&secrets::redact(message));
            out.push_str("\n\n");
        }
        let tail = self.log_tail().trim();
        match self.last_step() {
            Some(step) if !tail.is_empty() => {
                out.push_str(&format!(
                    "Last lines of `{}`{}:\n```\n{tail}\n```",
                    step.step.label(),
                    match (step.timed_out, step.exit) {
                        (true, _) => " (timed out)".to_string(),
                        (false, Some(code)) => format!(" (exit {code})"),
                        (false, None) => String::new(),
                    }
                ));
            }
            Some(step) => out.push_str(&format!("(`{}` printed nothing)", step.step.label())),
            None if out.is_empty() => out.push_str("(nothing ran)"),
            None => {}
        }
        out.trim_end().to_string()
    }
}

/// Port range and concurrency, from `app_runs` in the config.
#[derive(Debug, Clone)]
pub struct RunnerSettings {
    pub port_range: RangeInclusive<u16>,
    pub max_parallel: usize,
}

impl Default for RunnerSettings {
    fn default() -> Self {
        Self {
            port_range: ports::parse_range(ports::DEFAULT_PORT_RANGE).expect("default range"),
            max_parallel: 2,
        }
    }
}

impl RunnerSettings {
    /// A range that does not parse falls back to the default, loudly: the
    /// runner must keep working while the setting is wrong.
    pub fn from_config(port_range: &str, max_parallel: u32) -> Self {
        let range = ports::parse_range(port_range).unwrap_or_else(|e| {
            warn!("app runs: {e}; using {}", ports::DEFAULT_PORT_RANGE);
            Self::default().port_range
        });
        Self {
            port_range: range,
            max_parallel: (max_parallel as usize).max(1),
        }
    }
}

/// A held app-run slot. Dropping it frees the slot.
#[derive(Debug)]
pub struct SlotReservation {
    slots: Arc<AtomicUsize>,
}

impl Drop for SlotReservation {
    fn drop(&mut self) {
        self.slots.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub struct StandUpOptions {
    /// The card's key, as `$PREPCOOK_TICKET_KEY`; may be empty.
    pub ticket_key: String,
    /// The branch `$PREPCOOK_CHANGED_FILES` is computed against.
    pub target_branch: Option<String>,
    /// A slot taken earlier with the target's `try_reserve`; without one,
    /// stand-up takes its own or answers `Busy`.
    pub reservation: Option<SlotReservation>,
    /// Where the run's output goes; a fresh store when absent.
    pub log: Option<Arc<MsgStore>>,
    pub run_id: Option<Uuid>,
    /// A port to try before the range.
    pub preferred_port: Option<u16>,
}

/// The commands a run executes, resolved from the manifest for its target.
#[derive(Debug, Clone)]
struct Commands {
    setup: Option<String>,
    build: Option<String>,
    start: String,
    ready: Option<ReadyCheck>,
    seed: Option<String>,
    test: Option<String>,
    stop: Option<String>,
    restart: Option<String>,
    timeouts: StepTimeouts,
    /// Where the target's own commands come from, for messages.
    section: &'static str,
    /// Remote runs up at once; unused for local runs, which read the config.
    max_parallel: usize,
}

impl Commands {
    fn local(manifest: &Manifest) -> Self {
        let app = &manifest.app;
        Self {
            setup: app.setup.clone(),
            build: app.build.clone(),
            start: app.start.clone(),
            ready: app.ready.clone(),
            seed: app.seed.clone(),
            test: app.test.clone(),
            stop: app.stop.clone(),
            restart: None,
            timeouts: app.timeouts,
            section: "[app]",
            max_parallel: 0,
        }
    }

    /// `None` when the manifest has no `[app.remote]`.
    fn remote(manifest: &Manifest) -> Option<Self> {
        let app = &manifest.app;
        let remote = app.remote.as_ref()?;
        Some(Self {
            setup: app.setup.clone(),
            build: app.build.clone(),
            start: remote.start.clone(),
            ready: remote.ready.clone(),
            seed: remote.seed.clone(),
            test: remote.test.clone(),
            stop: remote.stop.clone(),
            restart: remote.restart.clone(),
            timeouts: StepTimeouts {
                setup: app.timeouts.setup,
                build: app.timeouts.build,
                start: remote.timeouts.start,
                seed: remote.timeouts.seed,
                test: remote.timeouts.test,
                stop: remote.timeouts.stop,
            },
            section: "[app.remote]",
            max_parallel: (remote.max_parallel as usize).max(1),
        })
    }

    /// The table `step`'s command is read from.
    fn section_of(&self, step: Step) -> &'static str {
        match step {
            Step::Setup | Step::Build => "[app]",
            _ => self.section,
        }
    }

    fn nothing_to_run(&self, step: Step) -> StepReport {
        StepReport::note(
            step,
            format!(
                "no `{}` in {}; nothing to run",
                step.label(),
                self.section_of(step)
            ),
        )
    }
}

/// Why a board set to `remote` cannot stand up from `root`'s manifest.
pub(crate) fn remote_missing_message(root: &Path) -> String {
    format!(
        "this board reproduces against a remote target, but {} in {} has no [app.remote] \
         table. Add one (docs/scripts.md#remote-targets) or set the board's reproduction \
         target to local.",
        manifest::MANIFEST_PATH,
        root.display()
    )
}

/// A remote redeploy in place: `[app.remote] restart` run as the start step,
/// with the address and reference of what it replaces.
struct Redeploy {
    command: String,
    url: String,
    target_ref: Option<String>,
}

/// The parts of a run teardown needs, shared by every handle to it.
struct RunInner {
    run_id: Uuid,
    kind: ReproductionTarget,
    /// Local runs only.
    port: Option<u16>,
    /// A remote run's address is known once its deploy reports it.
    url: Mutex<String>,
    /// What a remote deploy named the thing it deployed.
    target_ref: Mutex<Option<String>>,
    worktree: PathBuf,
    ticket_key: String,
    target_branch: Option<String>,
    manifest: Manifest,
    commands: Commands,
    /// What every script receives on top of the process environment. A
    /// remote run gains `PREPCOOK_APP_URL` once its deploy reports it.
    script_env: Mutex<Vec<(String, String)>>,
    secret_env: Vec<(String, String)>,
    run_dir: PathBuf,
    env_out: PathBuf,
    log: Arc<MsgStore>,
    started_at: DateTime<Utc>,
    child: Mutex<Option<AsyncGroupChild>>,
    start_tail: Arc<Mutex<VecDeque<String>>>,
    /// A remote run's `start` has run, so teardown owes it a `stop`.
    deployed: AtomicBool,
    ports: Arc<PortAllocator>,
    slot: Mutex<Option<SlotReservation>>,
    torn_down: AtomicBool,
}

impl RunInner {
    fn url(&self) -> String {
        lock(&self.url).clone()
    }

    fn target_ref(&self) -> Option<String> {
        lock(&self.target_ref).clone()
    }

    fn script_env(&self) -> Vec<(String, String)> {
        lock(&self.script_env).clone()
    }

    /// Sets (or replaces) one variable every later script receives.
    fn set_script_var(&self, name: &str, value: &str) {
        let mut env = lock(&self.script_env);
        env.retain(|(key, _)| key != name);
        env.push((name.to_string(), value.to_string()));
    }
}

impl Drop for RunInner {
    fn drop(&mut self) {
        if self.torn_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let child = self
            .child
            .get_mut()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if let Some(mut child) = child {
            warn!(
                "app run {} dropped without a teardown; killing its process group",
                self.run_id
            );
            let _ = child.start_kill();
            // Reap it so the group leader does not linger as a zombie.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
        self.ports.release(self.run_id);
        self.slot
            .get_mut()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        // A deployed remote app outlives this process unless its `stop`
        // runs; run it in the background, then clean up the run directory.
        if self.kind == ReproductionTarget::Remote
            && self.deployed.load(Ordering::SeqCst)
            && let Some(stop) = self.commands.stop.clone()
            && let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            warn!(
                "app run {} dropped without a teardown; running its remote stop",
                self.run_id
            );
            let worktree = self.worktree.clone();
            let env = self
                .script_env
                .get_mut()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            let log = self.log.clone();
            let timeout = Duration::from_secs(self.commands.timeouts.stop);
            let run_dir = self.run_dir.clone();
            handle.spawn(async move {
                run_command(Step::Stop, &stop, &worktree, &env, timeout, &log).await;
                let _ = std::fs::remove_dir_all(&run_dir);
            });
            return;
        }
        let _ = std::fs::remove_dir_all(&self.run_dir);
    }
}

/// A stood-up app. Cheap to clone; the app stays up until its target tears
/// it down or until the last clone is dropped.
#[derive(Clone)]
pub struct AppRun {
    pub run_id: Uuid,
    /// The target that stood it up.
    pub target_kind: ReproductionTarget,
    /// The port a local app listens on; `None` for a remote app.
    pub port: Option<u16>,
    pub url: String,
    /// What a remote deploy named the thing it deployed, when it said.
    /// Opaque: handed back to its `stop` and `restart` scripts.
    pub target_ref: Option<String>,
    /// What the seed script (and a remote deploy) wrote to `$PREPCOOK_ENV_OUT`.
    pub env: BTreeMap<String, String>,
    pub started_at: DateTime<Utc>,
    pub log: Arc<MsgStore>,
    pub worktree: PathBuf,
    /// The stand-up's steps, for the report.
    pub steps: Vec<StepReport>,
    inner: Arc<RunInner>,
}

impl std::fmt::Debug for AppRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppRun")
            .field("run_id", &self.run_id)
            .field("target_kind", &self.target_kind)
            .field("url", &self.url)
            .field("worktree", &self.worktree)
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AppRun {
    /// A handle on a live run that was never handed back to a caller (a
    /// verification in progress), with no seed output or steps.
    fn from_inner(inner: Arc<RunInner>) -> Self {
        Self {
            run_id: inner.run_id,
            target_kind: inner.kind,
            port: inner.port,
            url: inner.url(),
            target_ref: inner.target_ref(),
            env: BTreeMap::new(),
            started_at: inner.started_at,
            log: inner.log.clone(),
            worktree: inner.worktree.clone(),
            steps: Vec::new(),
            inner,
        }
    }

    /// The variables an agent session working against this run receives: the
    /// app's address, the seed's output and every secret.
    pub(crate) fn session_env(&self) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = self.inner.secret_env.iter().cloned().collect();
        env.extend(self.env.clone());
        env.insert("PREPCOOK_APP_URL".into(), self.url.clone());
        if let Some(port) = self.port {
            env.insert("PREPCOOK_PORT".into(), port.to_string());
        }
        env.insert(
            "PREPCOOK_WORKTREE".into(),
            self.worktree.to_string_lossy().into_owned(),
        );
        env.insert("PREPCOOK_RUN_ID".into(), self.run_id.to_string());
        env
    }

    pub fn status(&self, workspace_id: Option<Uuid>) -> AppRunStatus {
        AppRunStatus {
            run_id: self.run_id,
            target_kind: self.target_kind,
            worktree: self.worktree.to_string_lossy().into_owned(),
            url: self.url.clone(),
            port: self.port,
            seed_env_names: self.env.keys().cloned().collect(),
            started_at: self.started_at,
            workspace_id,
            health: None,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.inner.manifest
    }
}

/// Whether a run's app answers, as its target's status check found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunStatus {
    pub up: bool,
    /// What was checked and what it answered.
    pub message: String,
}

/// A run as the API shows it. Names of the seed's variables, never values.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AppRunStatus {
    pub run_id: Uuid,
    pub target_kind: ReproductionTarget,
    pub worktree: String,
    pub url: String,
    /// `None` for a remote app.
    pub port: Option<u16>,
    pub seed_env_names: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub workspace_id: Option<Uuid>,
    /// Filled in when the caller asked the target to check.
    pub health: Option<RunStatus>,
}

/// A Settings "Test run": in progress until `finished_at` is set.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ScriptRunState {
    pub run_id: Uuid,
    pub step: RunStep,
    /// The reproduction target the step ran on.
    pub target: ReproductionTarget,
    pub worktree: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub report: Option<StandUpReport>,
}

struct ScriptRunEntry {
    state: ScriptRunState,
    log: Arc<MsgStore>,
}

/// A run handed to a workspace: torn down when the agent process it was bound
/// to finishes, or when the workspace is cleaned up.
struct Adoption {
    run: AppRun,
    workspace_id: Uuid,
    process_id: Option<Uuid>,
}

struct RunnerInner {
    settings: RwLock<RunnerSettings>,
    /// The `app_runs` values the settings were last built from; see
    /// [`ScriptRunner::sync_settings`].
    applied: Mutex<Option<(String, u32)>>,
    ports: Arc<PortAllocator>,
    slots: Arc<AtomicUsize>,
    remote_slots: Arc<AtomicUsize>,
    live: Mutex<HashMap<Uuid, Weak<RunInner>>>,
    adopted: Mutex<HashMap<Uuid, Adoption>>,
    setup_done: Mutex<HashSet<PathBuf>>,
    script_runs: Mutex<VecDeque<ScriptRunEntry>>,
}

/// The steps an operation runs, in manifest order.
#[derive(Debug, Clone, Copy, Default)]
struct Plan {
    setup: bool,
    build: bool,
    start: bool,
    seed: bool,
    test: bool,
    stop: bool,
}

impl Plan {
    const STAND_UP: Plan = Plan {
        setup: true,
        build: true,
        start: true,
        seed: true,
        test: false,
        stop: false,
    };

    /// A remote deploy builds where it deploys, so `[app] build` is not part
    /// of a remote stand-up; `setup` is, because the deploy, seed and test
    /// scripts run here and need the worktree's dependencies.
    const REMOTE_STAND_UP: Plan = Plan {
        build: false,
        ..Self::STAND_UP
    };

    fn stand_up(kind: ReproductionTarget) -> Self {
        match kind {
            ReproductionTarget::Local => Self::STAND_UP,
            ReproductionTarget::Remote => Self::REMOTE_STAND_UP,
        }
    }

    fn for_step(kind: ReproductionTarget, step: RunStep) -> Self {
        match step {
            RunStep::StandUp => Self::stand_up(kind),
            RunStep::Setup => Plan {
                setup: true,
                ..Default::default()
            },
            RunStep::Build => Plan {
                build: true,
                ..Default::default()
            },
            RunStep::Start | RunStep::Ready => Plan {
                start: true,
                ..Default::default()
            },
            RunStep::Seed => Plan {
                seed: true,
                ..Default::default()
            },
            RunStep::Test => Plan {
                test: true,
                ..Default::default()
            },
            RunStep::Stop => Plan {
                stop: true,
                ..Default::default()
            },
        }
    }
}

/// Runs `.prepcook/scripts.toml`. Held by the deployment; clones share state.
/// Stand-ups go through a [`ReproTarget`], which picks how the app is stood
/// up; the bookkeeping every target shares (slots, runs handed to
/// workspaces, the Settings test runs) lives here.
#[derive(Clone)]
pub struct ScriptRunner {
    inner: Arc<RunnerInner>,
}

impl Default for ScriptRunner {
    fn default() -> Self {
        Self::new(RunnerSettings::default())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

impl ScriptRunner {
    pub fn new(settings: RunnerSettings) -> Self {
        Self {
            inner: Arc::new(RunnerInner {
                settings: RwLock::new(settings),
                applied: Mutex::new(None),
                ports: Arc::new(PortAllocator::new()),
                slots: Arc::new(AtomicUsize::new(0)),
                remote_slots: Arc::new(AtomicUsize::new(0)),
                live: Mutex::new(HashMap::new()),
                adopted: Mutex::new(HashMap::new()),
                setup_done: Mutex::new(HashSet::new()),
                script_runs: Mutex::new(VecDeque::new()),
            }),
        }
    }

    pub fn apply_settings(&self, settings: RunnerSettings) {
        *self
            .inner
            .settings
            .write()
            .unwrap_or_else(|p| p.into_inner()) = settings;
    }

    /// Rebuilds the settings from `app_runs` when those values changed since
    /// the last call, so a config edited anywhere reaches the runner without
    /// re-parsing (and re-warning about) an unchanged range on every call.
    pub fn sync_settings(&self, port_range: &str, max_parallel: u32) {
        let wanted = (port_range.to_string(), max_parallel);
        let mut applied = lock(&self.inner.applied);
        if applied.as_ref() == Some(&wanted) {
            return;
        }
        self.apply_settings(RunnerSettings::from_config(port_range, max_parallel));
        *applied = Some(wanted);
    }

    pub(crate) fn settings(&self) -> RunnerSettings {
        self.inner
            .settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn load_manifest(&self, worktree: &Path) -> Result<Manifest, ManifestError> {
        manifest::load_manifest(worktree)
    }

    /// Takes a local slot, or `None` when all `max_parallel` are in use.
    pub(crate) fn try_reserve(&self) -> Option<SlotReservation> {
        let max = self.settings().max_parallel;
        reserve(&self.inner.slots, max)
    }

    /// Takes a remote slot, or `None` when `max` are in use.
    pub(crate) fn try_reserve_remote(&self, max: usize) -> Option<SlotReservation> {
        reserve(&self.inner.remote_slots, max.max(1))
    }

    fn try_reserve_for(
        &self,
        commands: &Commands,
        kind: ReproductionTarget,
    ) -> Option<SlotReservation> {
        match kind {
            ReproductionTarget::Local => self.try_reserve(),
            ReproductionTarget::Remote => self.try_reserve_remote(commands.max_parallel),
        }
    }

    pub fn slots_in_use(&self) -> usize {
        self.inner.slots.load(Ordering::SeqCst)
    }

    pub fn remote_slots_in_use(&self) -> usize {
        self.inner.remote_slots.load(Ordering::SeqCst)
    }

    /// Whether a stand-up in this process already ran `setup` successfully in
    /// `worktree`, so the workspace setup hook need not run it again.
    pub fn has_completed_setup(&self, worktree: &Path) -> bool {
        lock(&self.inner.setup_done).contains(worktree)
    }

    // -- Stand-up, test, teardown ------------------------------------------

    /// The stand-up for `kind`, in `worktree`. On success the app is up and
    /// the returned run owns it.
    pub(crate) async fn stand_up_as(
        &self,
        kind: ReproductionTarget,
        worktree: &Path,
        opts: StandUpOptions,
    ) -> Result<AppRun, StandUpReport> {
        self.execute(worktree, opts, Plan::stand_up(kind), kind, None)
            .await
    }

    /// Runs `test` against a stood-up app, with `$PREPCOOK_CHANGED_FILES` set.
    pub(crate) async fn test(&self, run: &AppRun) -> StepReport {
        let inner = &run.inner;
        let Some(command) = inner.commands.test.clone() else {
            return inner.commands.nothing_to_run(Step::Test);
        };
        let mut env = inner.script_env();
        env.extend(run.env.clone());
        env.push((
            "PREPCOOK_CHANGED_FILES".into(),
            changed_files(&inner.worktree, inner.target_branch.as_deref()).await,
        ));
        run_command(
            Step::Test,
            &command,
            &inner.worktree,
            &env,
            Duration::from_secs(inner.commands.timeouts.test),
            &inner.log,
        )
        .await
    }

    /// Runs `seed` again against a stood-up app. What it writes is registered
    /// for redaction; the run's recorded environment is the stand-up's.
    pub(crate) async fn seed(&self, run: &AppRun) -> StepReport {
        let inner = &run.inner;
        let Some(command) = inner.commands.seed.clone() else {
            return inner.commands.nothing_to_run(Step::Seed);
        };
        let report = run_command(
            Step::Seed,
            &command,
            &inner.worktree,
            &inner.script_env(),
            Duration::from_secs(inner.commands.timeouts.seed),
            &inner.log,
        )
        .await;
        register_sensitive(&read_env_out(&inner.env_out));
        report
    }

    /// Stand-up, then `test`, then teardown: the pre-merge verification.
    pub(crate) async fn verify_as(
        &self,
        kind: ReproductionTarget,
        worktree: &Path,
        opts: StandUpOptions,
    ) -> StandUpReport {
        let run = match self.stand_up_as(kind, worktree, opts).await {
            Ok(run) => run,
            Err(report) => return report,
        };
        let test = self.test(&run).await;
        let mut steps = run.steps.clone();
        let passed = test.succeeded();
        steps.push(test);
        self.tear_down(&run).await;
        StandUpReport::new(
            if passed {
                StandUpOutcome::Ok
            } else {
                StandUpOutcome::TestFailed
            },
            steps,
            None,
        )
    }

    /// Runs `stop` (bounded by its timeout), kills a local app's start
    /// process group and gives back the port and the slot. Always completes,
    /// whatever `stop` does; a second call is a no-op.
    pub(crate) async fn tear_down(&self, run: &AppRun) {
        lock(&self.inner.adopted).remove(&run.run_id);
        self.tear_down_inner(&run.inner, true).await;
    }

    /// `run_stop` false skips the `stop` command: for a remote run about to
    /// be redeployed in place, whose environment must survive.
    async fn tear_down_inner(&self, inner: &Arc<RunInner>, run_stop: bool) {
        if inner.torn_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let child = lock(&inner.child).take();
        let owes_stop = match inner.kind {
            ReproductionTarget::Local => child.is_some(),
            ReproductionTarget::Remote => inner.deployed.load(Ordering::SeqCst),
        };
        if run_stop
            && owes_stop
            && let Some(stop) = inner.commands.stop.clone()
        {
            let report = run_command(
                Step::Stop,
                &stop,
                &inner.worktree,
                &inner.script_env(),
                Duration::from_secs(inner.commands.timeouts.stop),
                &inner.log,
            )
            .await;
            if !report.succeeded() {
                let how = if report.timed_out {
                    "timed out".to_string()
                } else {
                    format!("exited {:?}", report.exit)
                };
                match inner.kind {
                    ReproductionTarget::Local => warn!(
                        "app run {}: stop {how} — killing the process group regardless",
                        inner.run_id
                    ),
                    ReproductionTarget::Remote => warn!(
                        "app run {}: remote stop {how} — what it deployed may still be up",
                        inner.run_id
                    ),
                }
            }
        }
        if let Some(mut child) = child
            && let Err(e) = utils::process::kill_process_group(&mut child).await
        {
            warn!("app run {}: kill failed: {e}", inner.run_id);
        }
        inner.ports.release(inner.run_id);
        lock(&inner.slot).take();
        lock(&self.inner.live).remove(&inner.run_id);
        let _ = std::fs::remove_dir_all(&inner.run_dir);
        debug!("app run {} torn down", inner.run_id);
    }

    /// One step on its own, for the Settings "Test run" button. `start` and
    /// `ready` stand the app up without setup, build or seed and tear it down
    /// once it is ready (for a local app with no ready check, once
    /// `start`'s timeout passes with the process still up).
    pub(crate) async fn run_one_as(
        &self,
        kind: ReproductionTarget,
        worktree: &Path,
        step: RunStep,
        opts: StandUpOptions,
    ) -> StandUpReport {
        match self
            .execute(worktree, opts, Plan::for_step(kind, step), kind, None)
            .await
        {
            Ok(run) => {
                let steps = run.steps.clone();
                self.tear_down(&run).await;
                StandUpReport::new(StandUpOutcome::Ok, steps, None)
            }
            Err(report) => report,
        }
    }

    async fn execute(
        &self,
        worktree: &Path,
        opts: StandUpOptions,
        plan: Plan,
        kind: ReproductionTarget,
        redeploy: Option<Redeploy>,
    ) -> Result<AppRun, StandUpReport> {
        let log = opts.log.clone().unwrap_or_default();
        let fail = |outcome: StandUpOutcome, message: String| -> Result<AppRun, StandUpReport> {
            log.push(LogMsg::Stderr(format!("{}\n", secrets::redact(&message))));
            Err(StandUpReport::new(outcome, Vec::new(), Some(message)))
        };
        let busy = |max: usize| {
            fail(
                StandUpOutcome::Busy,
                format!("all {max} app-run slots are in use"),
            )
        };

        // A local slot is taken before the manifest is read; a remote one
        // after, because its limit is in the manifest.
        let mut reservation = opts.reservation;
        if kind == ReproductionTarget::Local && plan.start && reservation.is_none() {
            match self.try_reserve() {
                Some(taken) => reservation = Some(taken),
                None => return busy(self.settings().max_parallel),
            }
        }

        let manifest = match manifest::load_manifest(worktree) {
            Ok(manifest) => manifest,
            Err(e @ ManifestError::Missing { .. }) => {
                return fail(StandUpOutcome::ManifestMissing, e.to_string());
            }
            Err(e @ ManifestError::Invalid { .. }) => {
                return fail(StandUpOutcome::ManifestInvalid, e.to_string());
            }
            Err(e @ ManifestError::Unreadable { .. }) => {
                return fail(StandUpOutcome::Error(e.to_string()), e.to_string());
            }
        };
        let commands = match kind {
            ReproductionTarget::Local => Commands::local(&manifest),
            ReproductionTarget::Remote => match Commands::remote(&manifest) {
                Some(commands) => commands,
                None => {
                    return fail(
                        StandUpOutcome::TargetUnconfigured,
                        remote_missing_message(worktree),
                    );
                }
            },
        };
        if kind == ReproductionTarget::Remote && plan.start && reservation.is_none() {
            match self.try_reserve_for(&commands, kind) {
                Some(taken) => reservation = Some(taken),
                None => return busy(commands.max_parallel),
            }
        }

        let secrets = Secrets::load();
        if let Err(missing) = secrets.check_required(&manifest.secrets.requires) {
            let names = missing.join(", ");
            return fail(
                StandUpOutcome::SecretMissing(names.clone()),
                format!(
                    "required secret(s) not set: {names}. Set each with `prepcook secret set <NAME>` \
                     (stored in {}).",
                    secrets.path().display()
                ),
            );
        }

        let run_id = opts.run_id.unwrap_or_else(Uuid::new_v4);
        let port = match kind {
            ReproductionTarget::Local => {
                let range = self.settings().port_range;
                match self
                    .inner
                    .ports
                    .allocate_preferring(&range, run_id, opts.preferred_port)
                {
                    Ok(port) => Some(port),
                    Err(e) => return fail(StandUpOutcome::PortUnavailable, e.to_string()),
                }
            }
            ReproductionTarget::Remote => None,
        };

        let run_dir = std::env::temp_dir()
            .join("prepcook-app-runs")
            .join(run_id.simple().to_string());
        let env_out = run_dir.join("env.out");
        if let Err(e) =
            std::fs::create_dir_all(&run_dir).and_then(|()| std::fs::write(&env_out, b""))
        {
            self.inner.ports.release(run_id);
            return fail(
                StandUpOutcome::Error(format!("could not create {}: {e}", run_dir.display())),
                format!("could not create {}: {e}", run_dir.display()),
            );
        }

        let secret_env = secrets.env_vars();
        let (url, target_ref, mut script_env) = match (port, &redeploy) {
            (Some(port), _) => {
                let url = format!("http://127.0.0.1:{port}");
                let env: Vec<(String, String)> = vec![
                    ("PREPCOOK_PORT".into(), port.to_string()),
                    ("PORT".into(), port.to_string()),
                    ("PREPCOOK_APP_URL".into(), url.clone()),
                    (
                        "PREPCOOK_WORKTREE".into(),
                        worktree.to_string_lossy().into_owned(),
                    ),
                    ("PREPCOOK_TICKET_KEY".into(), opts.ticket_key.clone()),
                    ("PREPCOOK_RUN_ID".into(), run_id.to_string()),
                    (
                        "PREPCOOK_ENV_OUT".into(),
                        env_out.to_string_lossy().into_owned(),
                    ),
                ];
                (url, None, env)
            }
            (None, redeploy) => {
                let mut env: Vec<(String, String)> = vec![
                    (
                        "PREPCOOK_WORKTREE".into(),
                        worktree.to_string_lossy().into_owned(),
                    ),
                    ("PREPCOOK_TICKET_KEY".into(), opts.ticket_key.clone()),
                    ("PREPCOOK_RUN_ID".into(), run_id.to_string()),
                    ("PREPCOOK_BRANCH".into(), current_branch(worktree).await),
                    (
                        "PREPCOOK_ENV_OUT".into(),
                        env_out.to_string_lossy().into_owned(),
                    ),
                ];
                // A redeploy is told what it replaces.
                let (url, target_ref) = match redeploy {
                    Some(previous) => {
                        env.push((APP_URL_VAR.into(), previous.url.clone()));
                        if let Some(target_ref) = &previous.target_ref {
                            env.push((TARGET_REF_VAR.into(), target_ref.clone()));
                        }
                        (previous.url.clone(), previous.target_ref.clone())
                    }
                    None => (String::new(), None),
                };
                (url, target_ref, env)
            }
        };
        script_env.extend(secret_env.iter().cloned());

        let inner = Arc::new(RunInner {
            run_id,
            kind,
            port,
            url: Mutex::new(url),
            target_ref: Mutex::new(target_ref),
            worktree: worktree.to_path_buf(),
            ticket_key: opts.ticket_key.clone(),
            target_branch: opts.target_branch.clone(),
            manifest: manifest.clone(),
            commands: commands.clone(),
            script_env: Mutex::new(script_env),
            secret_env,
            run_dir,
            env_out: env_out.clone(),
            log: log.clone(),
            started_at: Utc::now(),
            child: Mutex::new(None),
            start_tail: Arc::new(Mutex::new(VecDeque::new())),
            deployed: AtomicBool::new(false),
            ports: self.inner.ports.clone(),
            slot: Mutex::new(reservation),
            torn_down: AtomicBool::new(false),
        });
        {
            let mut live = lock(&self.inner.live);
            live.retain(|_, weak| weak.strong_count() > 0);
            live.insert(run_id, Arc::downgrade(&inner));
        }
        match port {
            Some(port) => info!("app run {run_id}: {} on port {port}", worktree.display()),
            None => info!(
                "app run {run_id}: {} on a remote target",
                worktree.display()
            ),
        }

        let mut steps: Vec<StepReport> = Vec::new();
        let timeouts = commands.timeouts;
        let simple = [
            (
                plan.setup,
                Step::Setup,
                commands.setup.as_deref(),
                timeouts.setup,
            ),
            (
                plan.build,
                Step::Build,
                commands.build.as_deref(),
                timeouts.build,
            ),
        ];
        for (wanted, step, command, timeout) in simple {
            if !wanted {
                continue;
            }
            let Some(command) = command else {
                if !plan.start {
                    steps.push(commands.nothing_to_run(step));
                }
                continue;
            };
            let report = run_command(
                step,
                command,
                worktree,
                &inner.script_env(),
                Duration::from_secs(timeout),
                &log,
            )
            .await;
            let ok = report.succeeded();
            steps.push(report);
            if !ok {
                return Err(self.abort(&inner, steps, step).await);
            }
            if step == Step::Setup {
                lock(&self.inner.setup_done).insert(worktree.to_path_buf());
            }
        }

        let mut seed_env = BTreeMap::new();
        if plan.start {
            let started = match kind {
                ReproductionTarget::Local => self.start_and_wait(&inner, plan.seed).await,
                ReproductionTarget::Remote => {
                    let command = redeploy.map(|r| r.command);
                    self.deploy_and_wait(&inner, command).await
                }
            };
            match started {
                Ok(reports) => steps.extend(reports),
                Err((reports, outcome, message)) => {
                    steps.extend(reports);
                    let report = self.abort_with(&inner, steps, outcome, message).await;
                    return Err(report);
                }
            }
            if kind == ReproductionTarget::Remote {
                seed_env = handed_on(read_env_out(&env_out));
            }
        }

        if plan.seed {
            match commands.seed.as_deref() {
                Some(command) => {
                    let report = run_command(
                        Step::Seed,
                        command,
                        worktree,
                        &inner.script_env(),
                        Duration::from_secs(timeouts.seed),
                        &log,
                    )
                    .await;
                    let ok = report.succeeded();
                    steps.push(report);
                    if !ok {
                        return Err(self.abort(&inner, steps, Step::Seed).await);
                    }
                    seed_env = read_env_out(&env_out);
                    if kind == ReproductionTarget::Remote {
                        seed_env = handed_on(seed_env);
                    }
                    register_sensitive(&seed_env);
                    if !seed_env.is_empty() {
                        log.push_stdout(format!(
                            "seed wrote {}\n",
                            seed_env.keys().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
                }
                None if !plan.start => steps.push(commands.nothing_to_run(Step::Seed)),
                None => {}
            }
        }

        for (wanted, step, command, timeout) in [
            (
                plan.test,
                Step::Test,
                commands.test.as_deref(),
                timeouts.test,
            ),
            (
                plan.stop,
                Step::Stop,
                commands.stop.as_deref(),
                timeouts.stop,
            ),
        ] {
            if !wanted {
                continue;
            }
            let Some(command) = command else {
                steps.push(commands.nothing_to_run(step));
                continue;
            };
            let mut env = inner.script_env();
            if step == Step::Test {
                env.push((
                    "PREPCOOK_CHANGED_FILES".into(),
                    changed_files(worktree, opts.target_branch.as_deref()).await,
                ));
            }
            let report = run_command(
                step,
                command,
                worktree,
                &env,
                Duration::from_secs(timeout),
                &log,
            )
            .await;
            let ok = report.succeeded();
            steps.push(report);
            if !ok {
                return Err(self.abort(&inner, steps, step).await);
            }
        }

        Ok(AppRun {
            run_id,
            target_kind: kind,
            port,
            url: inner.url(),
            target_ref: inner.target_ref(),
            env: seed_env,
            started_at: inner.started_at,
            log,
            worktree: worktree.to_path_buf(),
            steps,
            inner,
        })
    }

    async fn abort(
        &self,
        inner: &Arc<RunInner>,
        steps: Vec<StepReport>,
        step: Step,
    ) -> StandUpReport {
        self.abort_with(inner, steps, StandUpOutcome::failed_step(step), None)
            .await
    }

    async fn abort_with(
        &self,
        inner: &Arc<RunInner>,
        steps: Vec<StepReport>,
        outcome: StandUpOutcome,
        message: Option<String>,
    ) -> StandUpReport {
        inner.log.push(LogMsg::Stderr(format!(
            "stand-up stopped: {}\n",
            outcome.label()
        )));
        self.tear_down_inner(inner, true).await;
        StandUpReport::new(outcome, steps, message)
    }

    /// Spawns `start` in its own process group, waits for it to listen on its
    /// port within `timeouts.start`, then runs the ready check within its own
    /// timeout. `start` dying at any point is `StartFailed`; never listening,
    /// or never passing the check, is `NotReady`. Without a ready check, a
    /// listening port is ready — or, for a single-step test run
    /// (`wait_for_listen` false), a process still up when `timeouts.start`
    /// runs out.
    async fn start_and_wait(&self, inner: &Arc<RunInner>, wait_for_listen: bool) -> StartResult {
        let commands = &inner.commands;
        let timeouts = commands.timeouts;
        let command = commands.start.clone();
        let port = inner.port.expect("a local run has a port");
        let url = inner.url();
        let script_env = inner.script_env();
        inner
            .log
            .push_stdout(format!("$ {}    # start\n", secrets::redact(&command)));
        let spawned_at = Instant::now();
        let mut child =
            match shell_command(&command, &inner.worktree, &script_env).group_spawn_no_window() {
                Ok(child) => child,
                Err(e) => {
                    let report = StepReport {
                        step: Step::Start,
                        exit: None,
                        timed_out: false,
                        duration_ms: 0,
                        log_tail: format!("could not start the shell: {e}"),
                    };
                    return Err((vec![report], StandUpOutcome::StartFailed, None));
                }
            };
        if let Some(stdout) = child.inner().stdout.take() {
            pump(
                stdout,
                inner.log.clone(),
                inner.start_tail.clone(),
                false,
                None,
            );
        }
        if let Some(stderr) = child.inner().stderr.take() {
            pump(
                stderr,
                inner.log.clone(),
                inner.start_tail.clone(),
                true,
                None,
            );
        }
        *lock(&inner.child) = Some(child);

        let start_report = |exit: Option<i32>, timed_out: bool| StepReport {
            step: Step::Start,
            exit,
            timed_out,
            duration_ms: spawned_at.elapsed().as_millis() as u64,
            log_tail: tail_text(&inner.start_tail),
        };
        let exited = || -> Option<Option<i32>> {
            let mut guard = lock(&inner.child);
            let child = guard.as_mut()?;
            match child.try_wait() {
                Ok(Some(status)) => Some(status.code()),
                Ok(None) => None,
                Err(_) => Some(None),
            }
        };

        // 1. Listening.
        let listen_deadline = spawned_at + Duration::from_secs(timeouts.start);
        let ready_check = commands.ready.clone();
        let listen_required = wait_for_listen || ready_check.is_some();
        loop {
            if let Some(code) = exited() {
                return Err((
                    vec![start_report(code, false)],
                    StandUpOutcome::StartFailed,
                    None,
                ));
            }
            if port_is_listening(port).await {
                break;
            }
            if Instant::now() >= listen_deadline {
                if !listen_required {
                    // A single-step test run with no ready check: still up
                    // after the start timeout counts.
                    return Ok(vec![start_report(None, false)]);
                }
                inner.log.push(LogMsg::Stderr(format!(
                    "start did not listen on port {port} within {}s\n",
                    timeouts.start
                )));
                return Err((
                    vec![start_report(None, true)],
                    StandUpOutcome::NotReady,
                    None,
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        let started = start_report(None, false);

        // 2. Ready.
        let Some(ready) = ready_check else {
            return Ok(vec![started]);
        };
        let ready_started = Instant::now();
        let deadline = ready_started + Duration::from_secs(ready.timeout_secs);
        let client = probe_client().map_err(|e| {
            (
                vec![started.clone()],
                StandUpOutcome::Error(e.clone()),
                None,
            )
        })?;
        loop {
            if let Some(code) = exited() {
                return Err((
                    vec![start_report(code, false)],
                    StandUpOutcome::StartFailed,
                    None,
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (passed, last) = probe_ready_once(
                &client,
                &ready,
                &url,
                &inner.worktree,
                &script_env,
                remaining,
            )
            .await;
            if passed {
                inner.log.push_stdout("ready\n".to_string());
                return Ok(vec![
                    started,
                    StepReport {
                        step: Step::Ready,
                        exit: Some(0),
                        timed_out: false,
                        duration_ms: ready_started.elapsed().as_millis() as u64,
                        log_tail: String::new(),
                    },
                ]);
            }
            if Instant::now() >= deadline {
                let tail = secrets::redact(&last);
                inner.log.push(LogMsg::Stderr(format!(
                    "not ready after {}s: {tail}\n",
                    ready.timeout_secs
                )));
                let mut log_tail = tail_text(&inner.start_tail);
                if !tail.is_empty() {
                    log_tail.push_str(&format!("\nready check: {tail}"));
                }
                return Err((
                    vec![
                        started,
                        StepReport {
                            step: Step::Ready,
                            exit: None,
                            timed_out: true,
                            duration_ms: ready_started.elapsed().as_millis() as u64,
                            log_tail,
                        },
                    ],
                    StandUpOutcome::NotReady,
                    None,
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Runs a remote target's `start` (or `command`, for a redeploy) to
    /// completion within `timeouts.start`, takes the app's address from what
    /// it reported, then polls the ready check against that address. A
    /// deploy that fails is `StartFailed`; one that succeeds without saying
    /// where the app is breaks the script contract, which is the setup's
    /// problem (`Error`); never passing the check is `NotReady`.
    async fn deploy_and_wait(&self, inner: &Arc<RunInner>, command: Option<String>) -> StartResult {
        let commands = &inner.commands;
        let command = command.unwrap_or_else(|| commands.start.clone());
        inner.deployed.store(true, Ordering::SeqCst);
        let (report, printed_url) = run_command_reporting_url(
            Step::Start,
            &command,
            &inner.worktree,
            &inner.script_env(),
            Duration::from_secs(commands.timeouts.start),
            &inner.log,
        )
        .await;
        if !report.succeeded() {
            return Err((vec![report], StandUpOutcome::StartFailed, None));
        }

        let written = read_env_out(&inner.env_out);
        let reported = written
            .get(APP_URL_VAR)
            .cloned()
            .or(printed_url)
            .or_else(|| Some(inner.url()).filter(|url| !url.is_empty()));
        let Some(url) = reported.as_deref().and_then(base_url) else {
            let message = match reported {
                Some(bad) => format!(
                    "{} start reported {bad:?} as the app's address, which is not an http(s) URL",
                    commands.section
                ),
                None => format!(
                    "{} start exited 0 without reporting the app's address: write \
                     {APP_URL_VAR}=<url> to $PREPCOOK_ENV_OUT, or print the URL as the last \
                     http(s) line on stdout (docs/scripts.md#remote-targets)",
                    commands.section
                ),
            };
            inner
                .log
                .push(LogMsg::Stderr(format!("{}\n", secrets::redact(&message))));
            return Err((
                vec![report],
                StandUpOutcome::Error(message.clone()),
                Some(message),
            ));
        };
        *lock(&inner.url) = url.clone();
        inner.set_script_var(APP_URL_VAR, &url);
        if let Some(target_ref) = written.get(TARGET_REF_VAR).filter(|r| !r.is_empty()) {
            *lock(&inner.target_ref) = Some(target_ref.clone());
            inner.set_script_var(TARGET_REF_VAR, target_ref);
        }
        inner.log.push_stdout(format!("deployed at {url}\n"));
        let deployed = report;

        let Some(ready) = commands.ready.clone() else {
            return Ok(vec![deployed]);
        };
        let ready_started = Instant::now();
        let deadline = ready_started + Duration::from_secs(ready.timeout_secs);
        let client = probe_client().map_err(|e| {
            (
                vec![deployed.clone()],
                StandUpOutcome::Error(e.clone()),
                None,
            )
        })?;
        let script_env = inner.script_env();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (passed, last) = probe_ready_once(
                &client,
                &ready,
                &url,
                &inner.worktree,
                &script_env,
                remaining,
            )
            .await;
            if passed {
                inner.log.push_stdout("ready\n".to_string());
                return Ok(vec![
                    deployed,
                    StepReport {
                        step: Step::Ready,
                        exit: Some(0),
                        timed_out: false,
                        duration_ms: ready_started.elapsed().as_millis() as u64,
                        log_tail: String::new(),
                    },
                ]);
            }
            if Instant::now() >= deadline {
                let tail = secrets::redact(&last);
                inner.log.push(LogMsg::Stderr(format!(
                    "not ready after {}s: {tail}\n",
                    ready.timeout_secs
                )));
                return Err((
                    vec![
                        deployed,
                        StepReport {
                            step: Step::Ready,
                            exit: None,
                            timed_out: true,
                            duration_ms: ready_started.elapsed().as_millis() as u64,
                            log_tail: format!("ready check: {tail}"),
                        },
                    ],
                    StandUpOutcome::NotReady,
                    None,
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Whether `run`'s app answers: for a local app, its start process is up
    /// and listening; then, for either, one pass of its ready check.
    pub(crate) async fn probe(&self, run: &AppRun) -> RunStatus {
        let inner = &run.inner;
        let down = |message: String| RunStatus { up: false, message };
        if inner.torn_down.load(Ordering::SeqCst) {
            return down("the app has been torn down".to_string());
        }
        if inner.kind == ReproductionTarget::Local {
            let exited = {
                let mut guard = lock(&inner.child);
                match guard.as_mut().map(|child| child.try_wait()) {
                    Some(Ok(None)) => None,
                    Some(Ok(Some(status))) => Some(status.code()),
                    Some(Err(_)) | None => Some(None),
                }
            };
            if let Some(code) = exited {
                return down(match code {
                    Some(code) => format!("the start process exited {code}"),
                    None => "the start process is not running".to_string(),
                });
            }
            if let Some(port) = inner.port
                && !port_is_listening(port).await
            {
                return down(format!("nothing is listening on port {port}"));
            }
        }
        let url = inner.url();
        let Some(ready) = inner.commands.ready.clone() else {
            return RunStatus {
                up: true,
                message: match inner.kind {
                    ReproductionTarget::Local => format!("listening at {url}; no ready check"),
                    ReproductionTarget::Remote => {
                        format!("deployed at {url}; no ready check to ask")
                    }
                },
            };
        };
        let client = match probe_client() {
            Ok(client) => client,
            Err(e) => return down(e),
        };
        let (passed, said) = probe_ready_once(
            &client,
            &ready,
            &url,
            &inner.worktree,
            &inner.script_env(),
            STATUS_PROBE_TIMEOUT,
        )
        .await;
        RunStatus {
            up: passed,
            message: if passed {
                format!("ready at {url}")
            } else {
                format!("not ready: {}", secrets::redact(&said))
            },
        }
    }

    // -- Runs handed to workspaces -----------------------------------------

    /// Hands `run` to `workspace_id`: the next agent process that starts there
    /// gets its environment ([`Self::bind_process`]) and the run is torn down
    /// when that process finishes.
    pub fn adopt(&self, run: AppRun, workspace_id: Uuid) {
        lock(&self.inner.adopted).insert(
            run.run_id,
            Adoption {
                run,
                workspace_id,
                process_id: None,
            },
        );
    }

    /// [`Self::adopt`] for an agent process that is already running: the run
    /// is torn down when `process_id` finishes. The process's environment was
    /// fixed when it started, so the caller tells it where the app is.
    pub fn adopt_for_process(&self, run: AppRun, workspace_id: Uuid, process_id: Uuid) {
        lock(&self.inner.adopted).insert(
            run.run_id,
            Adoption {
                run,
                workspace_id,
                process_id: Some(process_id),
            },
        );
    }

    /// Binds the workspace's unbound run to `process_id` and returns the
    /// session environment for it, or `None` when there is none.
    pub fn bind_process(
        &self,
        workspace_id: Uuid,
        process_id: Uuid,
    ) -> Option<HashMap<String, String>> {
        let mut adopted = lock(&self.inner.adopted);
        let adoption = adopted
            .values_mut()
            .find(|a| a.workspace_id == workspace_id && a.process_id.is_none())?;
        adoption.process_id = Some(process_id);
        Some(adoption.run.session_env())
    }

    /// Tears down every run bound to `process_id`, and frees a port allocated
    /// for it (a dev server). Called when an execution process ends.
    pub async fn release_process(&self, process_id: Uuid) {
        self.inner.ports.release(process_id);
        let runs: Vec<AppRun> = {
            let mut adopted = lock(&self.inner.adopted);
            let ids: Vec<Uuid> = adopted
                .iter()
                .filter(|(_, a)| a.process_id == Some(process_id))
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| adopted.remove(&id))
                .map(|a| a.run)
                .collect()
        };
        for run in runs {
            info!(
                "app run {}: agent process {process_id} finished; tearing down",
                run.run_id
            );
            self.tear_down(&run).await;
            run.log.push_finished();
        }
    }

    /// Tears down every run handed to `workspace_id`.
    pub async fn tear_down_workspace(&self, workspace_id: Uuid) {
        let runs: Vec<AppRun> = {
            let mut adopted = lock(&self.inner.adopted);
            let ids: Vec<Uuid> = adopted
                .iter()
                .filter(|(_, a)| a.workspace_id == workspace_id)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| adopted.remove(&id))
                .map(|a| a.run)
                .collect()
        };
        for run in runs {
            self.tear_down(&run).await;
            run.log.push_finished();
        }
    }

    /// Tears down every run handed to a workspace whose worktree is `worktree`
    /// — before something else needs the worktree to itself.
    pub async fn tear_down_worktree(&self, worktree: &Path) {
        let runs: Vec<AppRun> = {
            let mut adopted = lock(&self.inner.adopted);
            let ids: Vec<Uuid> = adopted
                .iter()
                .filter(|(_, a)| a.run.worktree == worktree)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| adopted.remove(&id))
                .map(|a| a.run)
                .collect()
        };
        for run in runs {
            self.tear_down(&run).await;
            run.log.push_finished();
        }
    }

    /// Tears down everything, adopted or not. For shutdown.
    pub async fn tear_down_all(&self) {
        let adopted: Vec<AppRun> = lock(&self.inner.adopted)
            .drain()
            .map(|(_, a)| a.run)
            .collect();
        for run in &adopted {
            self.tear_down(run).await;
        }
        let live: Vec<Arc<RunInner>> = lock(&self.inner.live)
            .drain()
            .filter_map(|(_, weak)| weak.upgrade())
            .collect();
        for inner in &live {
            self.tear_down_inner(inner, true).await;
        }
    }

    /// The run serving `path` — a worktree, a directory inside one, or the
    /// workspace directory that holds it — and the workspace it was handed
    /// to, if any.
    pub fn current_run_for_worktree(&self, path: &Path) -> Option<(AppRun, Option<Uuid>)> {
        let matches = |worktree: &Path| {
            worktree == path || path.starts_with(worktree) || worktree.starts_with(path)
        };
        if let Some(adoption) = lock(&self.inner.adopted)
            .values()
            .find(|a| matches(&a.run.worktree))
        {
            return Some((adoption.run.clone(), Some(adoption.workspace_id)));
        }
        lock(&self.inner.live)
            .values()
            .filter_map(Weak::upgrade)
            .find(|inner| matches(&inner.worktree) && !inner.torn_down.load(Ordering::SeqCst))
            .map(|inner| (AppRun::from_inner(inner), None))
    }

    /// [`Self::current_run_for_worktree`] as the API shows it.
    pub fn current_for_worktree(&self, path: &Path) -> Option<AppRunStatus> {
        self.current_run_for_worktree(path)
            .map(|(run, workspace_id)| run.status(workspace_id))
    }

    /// The target an adopted run was stood up with.
    pub fn adopted_kind(&self, run_id: Uuid) -> Option<ReproductionTarget> {
        lock(&self.inner.adopted)
            .get(&run_id)
            .map(|adoption| adoption.run.target_kind)
    }

    /// Stands a workspace's run up again in the same worktree, keeping its
    /// binding to the agent process. For an agent testing a candidate
    /// change. A local app is torn down and comes back on the same port
    /// when it is still free; a remote one runs `[app.remote] restart` in
    /// place when the manifest has one, and is otherwise stopped and
    /// deployed again.
    pub(crate) async fn restart(
        &self,
        run_id: Uuid,
    ) -> Result<(AppRunStatus, StandUpReport), String> {
        let adoption = lock(&self.inner.adopted).remove(&run_id);
        let Some(adoption) = adoption else {
            return Err(if lock(&self.inner.live).contains_key(&run_id) {
                format!(
                    "app run {run_id} belongs to a verification in progress and cannot be restarted"
                )
            } else {
                format!("no app run {run_id}")
            });
        };
        let old = adoption.run;
        let kind = old.target_kind;
        let opts = StandUpOptions {
            ticket_key: old.inner.ticket_key.clone(),
            target_branch: old.inner.target_branch.clone(),
            reservation: None,
            log: Some(old.log.clone()),
            run_id: None,
            preferred_port: old.port,
        };
        let worktree = old.worktree.clone();
        let redeploy = match kind {
            ReproductionTarget::Remote => {
                old.inner.commands.restart.clone().map(|command| Redeploy {
                    command,
                    url: old.url.clone(),
                    target_ref: old.inner.target_ref(),
                })
            }
            ReproductionTarget::Local => None,
        };
        old.log.push_stdout("restarting\n".to_string());
        // A redeploy replaces what is there, so the old run gives back its
        // slot without destroying its environment.
        self.tear_down_inner(&old.inner, redeploy.is_none()).await;
        drop(old);
        match self
            .execute(&worktree, opts, Plan::stand_up(kind), kind, redeploy)
            .await
        {
            Ok(run) => {
                let report = StandUpReport::new(StandUpOutcome::Ok, run.steps.clone(), None);
                let status = run.status(Some(adoption.workspace_id));
                lock(&self.inner.adopted).insert(
                    run.run_id,
                    Adoption {
                        run,
                        workspace_id: adoption.workspace_id,
                        process_id: adoption.process_id,
                    },
                );
                Ok((status, report))
            }
            Err(report) => Err(format!(
                "the app did not come back up: {}\n\n{}",
                report.outcome.label(),
                report.evidence()
            )),
        }
    }

    // -- Script runs for the Settings UI -----------------------------------

    /// Starts `step` on `target` in `worktree` in the background and returns
    /// its id at once. Output streams from [`Self::script_run_log`]; the
    /// report lands in [`Self::script_run`] when it finishes.
    pub fn spawn_script_run(
        &self,
        target: ReproTargetKind,
        worktree: PathBuf,
        step: RunStep,
    ) -> Uuid {
        let run_id = Uuid::new_v4();
        let log = Arc::new(MsgStore::new());
        {
            let mut runs = lock(&self.inner.script_runs);
            while runs.len() >= SCRIPT_RUN_HISTORY {
                runs.pop_front();
            }
            runs.push_back(ScriptRunEntry {
                state: ScriptRunState {
                    run_id,
                    step,
                    target: target.target(),
                    worktree: worktree.to_string_lossy().into_owned(),
                    started_at: Utc::now(),
                    finished_at: None,
                    report: None,
                },
                log: log.clone(),
            });
        }
        let runner = self.clone();
        tokio::spawn(async move {
            let request = StandUpRequest {
                log: Some(log.clone()),
                run_id: Some(run_id),
                ..StandUpRequest::new(worktree)
            };
            let report = target.run_step(request, step).await;
            log.push_stdout(format!("result: {}\n", report.outcome.label()));
            {
                let mut runs = lock(&runner.inner.script_runs);
                if let Some(entry) = runs.iter_mut().find(|e| e.state.run_id == run_id) {
                    entry.state.finished_at = Some(Utc::now());
                    entry.state.report = Some(report);
                }
            }
            log.push_finished();
        });
        run_id
    }

    pub fn script_run(&self, run_id: Uuid) -> Option<ScriptRunState> {
        lock(&self.inner.script_runs)
            .iter()
            .find(|e| e.state.run_id == run_id)
            .map(|e| e.state.clone())
    }

    pub fn script_run_log(&self, run_id: Uuid) -> Option<Arc<MsgStore>> {
        lock(&self.inner.script_runs)
            .iter()
            .find(|e| e.state.run_id == run_id)
            .map(|e| e.log.clone())
    }

    // -- Environment for processes the container starts --------------------

    /// What a workspace hook (setup, cleanup, archive) receives: the worktree
    /// and every secret.
    pub fn hook_env(&self, worktree: &Path) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = Secrets::load().env_vars().into_iter().collect();
        env.insert(
            "PREPCOOK_WORKTREE".into(),
            worktree.to_string_lossy().into_owned(),
        );
        env
    }

    /// What the preview's dev server receives: a port from the runner's range
    /// held for `process_id` until [`Self::release_process`], and the same
    /// variables a local stand-up's scripts get.
    pub(crate) fn dev_server_env(
        &self,
        process_id: Uuid,
        worktree: &Path,
    ) -> Result<HashMap<String, String>, ports::PortError> {
        let range = self.settings().port_range;
        let port = self.inner.ports.allocate(&range, process_id)?;
        let mut env = self.hook_env(worktree);
        env.insert("PREPCOOK_PORT".into(), port.to_string());
        env.insert("PORT".into(), port.to_string());
        env.insert(
            "PREPCOOK_APP_URL".into(),
            format!("http://127.0.0.1:{port}"),
        );
        env.insert("PREPCOOK_RUN_ID".into(), process_id.to_string());
        Ok(env)
    }
}

/// What a start phase came to: its step reports, or those with the outcome
/// and an optional message for a person.
type StartResult = Result<Vec<StepReport>, (Vec<StepReport>, StandUpOutcome, Option<String>)>;

fn reserve(slots: &Arc<AtomicUsize>, max: usize) -> Option<SlotReservation> {
    slots
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
            (used < max).then_some(used + 1)
        })
        .ok()
        .map(|_| SlotReservation {
            slots: slots.clone(),
        })
}

/// The variables `$PREPCOOK_ENV_OUT` hands on to an agent session: all of
/// them but the address and reference a remote deploy reports, which the
/// run carries itself.
fn handed_on(mut env: BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.remove(APP_URL_VAR);
    env.remove(TARGET_REF_VAR);
    env
}

/// Registers the values of sensitive-looking variables for redaction.
fn register_sensitive(env: &BTreeMap<String, String>) {
    for (name, value) in env {
        if secrets::is_sensitive_name(name) {
            secrets::register_redaction(value);
        }
    }
}

/// `raw` as an app base URL: an absolute http(s) URL with no trailing slash,
/// so a ready path can be appended. `None` for anything else.
pub(crate) fn base_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let parsed = reqwest::Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(raw.trim_end_matches('/').to_string())
}

/// The URL a stdout line announces: the line, when it starts with `http://`
/// or `https://`, up to the first whitespace.
fn url_in_line(line: &str) -> Option<String> {
    let line = line.trim();
    (line.starts_with("http://") || line.starts_with("https://"))
        .then(|| line.split_whitespace().next().unwrap_or(line).to_string())
}

fn probe_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())
}

/// One pass of a ready check against `base_url`: a GET of the path that
/// passes on 2xx, or the command exiting 0 within `remaining` (at least a
/// second). Returns whether it passed and what it last said.
async fn probe_ready_once(
    client: &reqwest::Client,
    ready: &ReadyCheck,
    base_url: &str,
    worktree: &Path,
    env: &[(String, String)],
    remaining: Duration,
) -> (bool, String) {
    match (&ready.url, &ready.command) {
        (Some(path), _) => {
            let target = format!("{base_url}{path}");
            match client.get(&target).send().await {
                Ok(response) if response.status().is_success() => (true, String::new()),
                Ok(response) => (false, format!("GET {target} → {}", response.status())),
                Err(e) => (false, format!("GET {target} → {e}")),
            }
        }
        (None, Some(command)) => {
            let report = run_command_quiet(
                command,
                worktree,
                env,
                remaining.max(Duration::from_secs(1)),
            )
            .await;
            (report.exit == Some(0) && !report.timed_out, report.log_tail)
        }
        (None, None) => (true, String::new()),
    }
}

/// The worktree's checked-out branch, `$PREPCOOK_BRANCH` for remote scripts.
/// Empty when git cannot say or the worktree is on a detached HEAD.
async fn current_branch(worktree: &Path) -> String {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .stdin(Stdio::null())
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------

fn shell_command(command: &str, cwd: &Path, env: &[(String, String)]) -> tokio::process::Command {
    let (shell, arg) = get_shell_command();
    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg(arg)
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd
}

fn push_tail(tail: &Mutex<VecDeque<String>>, line: String) {
    let mut tail = lock(tail);
    if tail.len() == LOG_TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line);
}

fn tail_text(tail: &Mutex<VecDeque<String>>) -> String {
    lock(tail).iter().cloned().collect::<Vec<_>>().join("\n")
}

/// Copies a pipe into the log and the tail line by line, redacted. Invalid
/// UTF-8 is replaced rather than ending the copy. With `last_url`, the last
/// line that announces a URL (see [`url_in_line`]) is kept there, unredacted.
fn pump<R>(
    reader: R,
    log: Arc<MsgStore>,
    tail: Arc<Mutex<VecDeque<String>>>,
    stderr: bool,
    last_url: Option<Arc<Mutex<Option<String>>>>,
) -> tokio::task::JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let raw = String::from_utf8_lossy(&buf);
                    let raw = raw.trim_end_matches(['\n', '\r']);
                    if let Some(slot) = &last_url
                        && let Some(url) = url_in_line(raw)
                    {
                        *lock(slot) = Some(url);
                    }
                    let line = secrets::redact(raw);
                    push_tail(&tail, line.clone());
                    let text = format!("{line}\n");
                    log.push(if stderr {
                        LogMsg::Stderr(text)
                    } else {
                        LogMsg::Stdout(text)
                    });
                }
            }
        }
    })
}

/// Runs one command to completion within `timeout`, killing its process group
/// when it overruns.
async fn run_command(
    step: Step,
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
    log: &Arc<MsgStore>,
) -> StepReport {
    run_command_logged(step, command, cwd, env, timeout, log, None).await
}

/// [`run_command`], also returning the last URL the command printed on
/// stdout: a remote deploy's address when it does not write one to
/// `$PREPCOOK_ENV_OUT`.
async fn run_command_reporting_url(
    step: Step,
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
    log: &Arc<MsgStore>,
) -> (StepReport, Option<String>) {
    let url = Arc::new(Mutex::new(None));
    let report = run_command_logged(step, command, cwd, env, timeout, log, Some(url.clone())).await;
    let url = lock(&url).take();
    (report, url)
}

async fn run_command_logged(
    step: Step,
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
    log: &Arc<MsgStore>,
    last_url: Option<Arc<Mutex<Option<String>>>>,
) -> StepReport {
    log.push_stdout(format!(
        "$ {}    # {}\n",
        secrets::redact(command),
        step.label()
    ));
    let report = run_command_into(step, command, cwd, env, timeout, Some(log), last_url).await;
    if !report.succeeded() {
        log.push(LogMsg::Stderr(format!(
            "{} {}\n",
            step.label(),
            match (report.timed_out, report.exit) {
                (true, _) => format!("timed out after {}s", timeout.as_secs()),
                (false, Some(code)) => format!("exited {code}"),
                (false, None) => "was killed or could not start".to_string(),
            }
        )));
    }
    report
}

/// [`run_command`] without writing to the run's log: the ready command runs
/// every half second and would otherwise bury the start output.
async fn run_command_quiet(
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
) -> StepReport {
    run_command_into(Step::Ready, command, cwd, env, timeout, None, None).await
}

async fn run_command_into(
    step: Step,
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
    log: Option<&Arc<MsgStore>>,
    last_url: Option<Arc<Mutex<Option<String>>>>,
) -> StepReport {
    let started = Instant::now();
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let mut child = match shell_command(command, cwd, env).group_spawn_no_window() {
        Ok(child) => child,
        Err(e) => {
            return StepReport {
                step,
                exit: None,
                timed_out: false,
                duration_ms: 0,
                log_tail: format!("could not start the shell: {e}"),
            };
        }
    };
    let sink = log.cloned().unwrap_or_default();
    let mut pumps = Vec::new();
    if let Some(stdout) = child.inner().stdout.take() {
        pumps.push(pump(stdout, sink.clone(), tail.clone(), false, last_url));
    }
    if let Some(stderr) = child.inner().stderr.take() {
        pumps.push(pump(stderr, sink.clone(), tail.clone(), true, None));
    }
    let (exit, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => (status.code(), false),
        Ok(Err(e)) => {
            push_tail(&tail, format!("wait failed: {e}"));
            (None, false)
        }
        Err(_) => {
            let _ = utils::process::kill_process_group(&mut child).await;
            (None, true)
        }
    };
    for handle in pumps {
        let _ = tokio::time::timeout(PIPE_DRAIN_TIMEOUT, handle).await;
    }
    StepReport {
        step,
        exit,
        timed_out,
        duration_ms: started.elapsed().as_millis() as u64,
        log_tail: tail_text(&tail),
    }
}

async fn port_is_listening(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(1),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await,
        Ok(Ok(_))
    )
}

/// `$PREPCOOK_CHANGED_FILES`: the branch's changed paths against `target`, one per
/// line, relative to the worktree. Empty when there is no target or git
/// cannot say.
async fn changed_files(worktree: &Path, target: Option<&str>) -> String {
    let Some(target) = target.filter(|t| !t.is_empty()) else {
        return String::new();
    };
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["diff", "--name-only", &format!("{target}...HEAD")])
        .stdin(Stdio::null())
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
        Ok(output) => {
            warn!(
                "changed files against {target} in {}: {}",
                worktree.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            String::new()
        }
        Err(e) => {
            warn!("changed files: could not run git: {e}");
            String::new()
        }
    }
}

/// The `KEY=VALUE` lines a script appended to `$PREPCOOK_ENV_OUT`.
fn read_env_out(path: &Path) -> BTreeMap<String, String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            if !secrets::is_valid_name(name) {
                return None;
            }
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::reproduction::LocalTarget;

    /// The local target's operations, as the tests below drive them.
    impl ScriptRunner {
        async fn stand_up(
            &self,
            worktree: &Path,
            opts: StandUpOptions,
        ) -> Result<AppRun, StandUpReport> {
            self.stand_up_as(ReproductionTarget::Local, worktree, opts)
                .await
        }

        async fn verify(&self, worktree: &Path, opts: StandUpOptions) -> StandUpReport {
            self.verify_as(ReproductionTarget::Local, worktree, opts)
                .await
        }

        async fn run_one(
            &self,
            worktree: &Path,
            step: RunStep,
            opts: StandUpOptions,
        ) -> StandUpReport {
            self.run_one_as(ReproductionTarget::Local, worktree, step, opts)
                .await
        }
    }

    /// A worktree holding `manifest` as its `.prepcook/scripts.toml`.
    fn worktree(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".prepcook")).unwrap();
        std::fs::write(dir.path().join(".prepcook/scripts.toml"), manifest).unwrap();
        dir
    }

    fn runner(range: &str, max_parallel: u32) -> ScriptRunner {
        ScriptRunner::new(RunnerSettings::from_config(range, max_parallel))
    }

    /// A python HTTP server is the app; `test` exits with `code`.
    fn http_manifest(test_exit: i32) -> String {
        format!(
            r#"
[app]
setup = "echo installing; echo SETUP_RAN > setup.marker"
start = "exec python3 -m http.server $PREPCOOK_PORT --bind 127.0.0.1"
ready = {{ url = "/", timeout_secs = 30 }}
seed = "echo PREPCOOK_TEST_USER_EMAIL=user@example.com >> $PREPCOOK_ENV_OUT; echo PREPCOOK_TEST_USER_PASSWORD=pw-from-seed-123 >> $PREPCOOK_ENV_OUT"
test = "echo testing $PREPCOOK_TICKET_KEY; exit {test_exit}"
stop = "echo stopping"

[app.timeouts]
start = 30
"#
        )
    }

    #[test]
    fn outcomes_partition_into_branch_and_runner_faults() {
        let branch = [
            StandUpOutcome::SetupFailed,
            StandUpOutcome::BuildFailed,
            StandUpOutcome::StartFailed,
            StandUpOutcome::NotReady,
            StandUpOutcome::SeedFailed,
            StandUpOutcome::TestFailed,
        ];
        let runner = [
            StandUpOutcome::ManifestMissing,
            StandUpOutcome::ManifestInvalid,
            StandUpOutcome::TargetUnconfigured,
            StandUpOutcome::PortUnavailable,
            StandUpOutcome::SecretMissing("X".into()),
            StandUpOutcome::Error("boom".into()),
        ];
        for outcome in &branch {
            assert!(
                outcome.is_branch_fault() && !outcome.is_runner_fault(),
                "{outcome:?}"
            );
            assert!(!StandUpOutcome::is_runner_fault_label(outcome.label()));
        }
        for outcome in &runner {
            assert!(
                outcome.is_runner_fault() && !outcome.is_branch_fault(),
                "{outcome:?}"
            );
            assert!(StandUpOutcome::is_runner_fault_label(outcome.label()));
        }
        for outcome in [StandUpOutcome::Ok, StandUpOutcome::Busy] {
            assert!(!outcome.is_branch_fault() && !outcome.is_runner_fault());
        }
        assert!(StandUpOutcome::is_runner_fault_label("busy"));
        assert_eq!(StandUpOutcome::NotReady.label(), "not-ready");
        assert_eq!(
            StandUpOutcome::SecretMissing("A".into()).label(),
            "secret-missing"
        );
    }

    #[tokio::test]
    async fn a_missing_manifest_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let report = runner("4600-4609", 2)
            .stand_up(dir.path(), StandUpOptions::default())
            .await
            .unwrap_err();
        assert_eq!(report.outcome, StandUpOutcome::ManifestMissing);
        assert!(
            report
                .message
                .as_deref()
                .unwrap()
                .contains("see docs/scripts.md")
        );
    }

    #[tokio::test]
    async fn a_missing_secret_stops_the_run_before_any_script() {
        let dir = worktree(
            "[app]\nsetup = \"touch ran\"\nstart = \"sleep 30\"\n[secrets]\nrequires = [\"PREPCOOK_RUNNER_TEST_NEVER_SET\"]\n",
        );
        let report = runner("4610-4619", 2)
            .stand_up(dir.path(), StandUpOptions::default())
            .await
            .unwrap_err();
        assert_eq!(
            report.outcome,
            StandUpOutcome::SecretMissing("PREPCOOK_RUNNER_TEST_NEVER_SET".into())
        );
        assert!(!dir.path().join("ran").exists(), "setup must not run");
    }

    #[tokio::test]
    async fn an_app_stands_up_tests_green_and_tears_down() {
        let dir = worktree(&http_manifest(0));
        let runner = runner("4620-4639", 2);
        let run = runner
            .stand_up(
                dir.path(),
                StandUpOptions {
                    ticket_key: "BUG-1".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap_or_else(|report| panic!("stand-up failed: {report:?}"));
        let port = run.port.expect("a local run has a port");
        assert!((4620..=4639).contains(&port));
        assert_eq!(run.url, format!("http://127.0.0.1:{port}"));
        assert_eq!(run.target_kind, ReproductionTarget::Local);
        assert_eq!(
            run.env.get("PREPCOOK_TEST_USER_EMAIL").map(String::as_str),
            Some("user@example.com")
        );
        assert!(dir.path().join("setup.marker").exists());
        assert!(runner.has_completed_setup(dir.path()));
        assert_eq!(runner.slots_in_use(), 1);
        let session = run.session_env();
        assert_eq!(session.get("PREPCOOK_APP_URL"), Some(&run.url));
        assert_eq!(session.get("PREPCOOK_PORT"), Some(&port.to_string()));
        // The seeded password is registered for redaction.
        assert_eq!(
            secrets::redact("pw is pw-from-seed-123"),
            "pw is [redacted]"
        );
        assert!(port_is_listening(port).await);
        assert_eq!(
            runner.current_for_worktree(dir.path()).map(|s| s.run_id),
            Some(run.run_id)
        );

        let test = runner.test(&run).await;
        assert!(test.succeeded(), "{test:?}");
        assert!(test.log_tail.contains("testing BUG-1"), "{}", test.log_tail);

        runner.tear_down(&run).await;
        assert!(!port_is_listening(port).await, "the app is gone");
        assert_eq!(runner.slots_in_use(), 0);
        assert_eq!(runner.inner.ports.in_use(), 0);
        // A second teardown is a no-op.
        runner.tear_down(&run).await;
        let history: Vec<String> = run
            .log
            .get_history()
            .into_iter()
            .filter_map(|m| match m {
                LogMsg::Stdout(s) => Some(s),
                _ => None,
            })
            .collect();
        assert!(
            history.iter().any(|l| l.contains("stopping")),
            "{history:?}"
        );
    }

    #[tokio::test]
    async fn verify_routes_a_red_test_as_a_branch_fault() {
        let dir = worktree(&http_manifest(1));
        let runner = runner("4640-4659", 2);
        let report = runner.verify(dir.path(), StandUpOptions::default()).await;
        assert_eq!(report.outcome, StandUpOutcome::TestFailed);
        assert!(report.outcome.is_branch_fault());
        let last = report.last_step().unwrap();
        assert_eq!(last.step, Step::Test);
        assert_eq!(last.exit, Some(1));
        assert!(
            report.evidence().contains("(exit 1)"),
            "{}",
            report.evidence()
        );
        assert_eq!(runner.slots_in_use(), 0);

        let green = worktree(&http_manifest(0));
        let report = runner.verify(green.path(), StandUpOptions::default()).await;
        assert_eq!(report.outcome, StandUpOutcome::Ok, "{report:?}");
    }

    #[tokio::test]
    async fn a_start_that_dies_is_start_failed_and_a_silent_one_is_not_ready() {
        let runner = runner("4660-4669", 2);
        let dies = worktree("[app]\nstart = \"echo boom; exit 3\"\n[app.timeouts]\nstart = 10\n");
        let report = runner
            .stand_up(dies.path(), StandUpOptions::default())
            .await
            .unwrap_err();
        assert_eq!(report.outcome, StandUpOutcome::StartFailed);
        assert_eq!(report.last_step().unwrap().exit, Some(3));
        assert!(report.log_tail().contains("boom"));

        let silent = worktree("[app]\nstart = \"sleep 30\"\n[app.timeouts]\nstart = 1\n");
        let report = runner
            .stand_up(silent.path(), StandUpOptions::default())
            .await
            .unwrap_err();
        assert_eq!(report.outcome, StandUpOutcome::NotReady);
        assert_eq!(runner.slots_in_use(), 0, "a failed stand-up frees its slot");

        let setup_fails = worktree("[app]\nsetup = \"exit 2\"\nstart = \"sleep 30\"\n");
        let report = runner
            .stand_up(setup_fails.path(), StandUpOptions::default())
            .await
            .unwrap_err();
        assert_eq!(report.outcome, StandUpOutcome::SetupFailed);
    }

    #[tokio::test]
    async fn a_full_runner_answers_busy_and_a_dropped_run_frees_everything() {
        let dir = worktree(&http_manifest(0));
        let runner = runner("4670-4679", 1);
        let held = runner.try_reserve().expect("one slot");
        assert!(runner.try_reserve().is_none());
        let report = runner
            .stand_up(dir.path(), StandUpOptions::default())
            .await
            .unwrap_err();
        assert_eq!(report.outcome, StandUpOutcome::Busy);
        drop(held);

        let run = runner
            .stand_up(dir.path(), StandUpOptions::default())
            .await
            .unwrap();
        let port = run.port.unwrap();
        drop(run);
        // Drop kills the group and frees the port and the slot synchronously.
        assert_eq!(runner.slots_in_use(), 0);
        assert_eq!(runner.inner.ports.in_use(), 0);
        let mut gone = false;
        for _ in 0..20 {
            if !port_is_listening(port).await {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the dropped run's server is still listening");
    }

    #[tokio::test]
    async fn an_adopted_run_is_bound_to_a_process_and_released_with_it() {
        let dir = worktree(&http_manifest(0));
        let runner = runner("4680-4689", 2);
        let run = runner
            .stand_up(dir.path(), StandUpOptions::default())
            .await
            .unwrap();
        let (workspace, process) = (Uuid::new_v4(), Uuid::new_v4());
        let port = run.port.unwrap();
        runner.adopt(run, workspace);
        assert!(runner.bind_process(Uuid::new_v4(), process).is_none());
        let env = runner.bind_process(workspace, process).unwrap();
        assert_eq!(env.get("PREPCOOK_PORT"), Some(&port.to_string()));
        assert!(env.contains_key("PREPCOOK_TEST_USER_PASSWORD"));
        // Already bound: a second process gets nothing.
        assert!(runner.bind_process(workspace, Uuid::new_v4()).is_none());

        let restarted = runner
            .restart(runner.current_for_worktree(dir.path()).unwrap().run_id)
            .await
            .unwrap();
        assert_eq!(restarted.0.port, Some(port), "comes back on the same port");
        assert_eq!(restarted.1.outcome, StandUpOutcome::Ok);

        runner.release_process(process).await;
        assert!(runner.current_for_worktree(dir.path()).is_none());
        assert_eq!(runner.slots_in_use(), 0);
        assert!(!port_is_listening(port).await);
    }

    #[tokio::test]
    async fn single_steps_run_on_their_own() {
        let dir = worktree(&http_manifest(0));
        let runner = runner("4690-4699", 2);
        let report = runner
            .run_one(dir.path(), RunStep::Test, StandUpOptions::default())
            .await;
        assert_eq!(report.outcome, StandUpOutcome::Ok);
        assert_eq!(report.steps.len(), 1);
        let report = runner
            .run_one(dir.path(), RunStep::Build, StandUpOptions::default())
            .await;
        assert_eq!(report.outcome, StandUpOutcome::Ok);
        assert!(report.steps[0].log_tail.contains("no `build`"));
        let report = runner
            .run_one(dir.path(), RunStep::Start, StandUpOptions::default())
            .await;
        assert_eq!(report.outcome, StandUpOutcome::Ok, "{report:?}");
        assert_eq!(report.steps.last().unwrap().step, Step::Ready);
        assert_eq!(runner.slots_in_use(), 0);

        let id = runner.spawn_script_run(
            LocalTarget::new(runner.clone()).into(),
            dir.path().to_path_buf(),
            RunStep::Setup,
        );
        let mut state = runner.script_run(id).unwrap();
        for _ in 0..100 {
            if state.finished_at.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            state = runner.script_run(id).unwrap();
        }
        assert_eq!(state.target, ReproductionTarget::Local);
        assert_eq!(state.report.unwrap().outcome, StandUpOutcome::Ok);
        assert!(runner.script_run_log(id).is_some());
    }

    #[test]
    fn urls_are_read_from_stdout_lines_and_normalised() {
        assert_eq!(
            url_in_line("https://pr-12.preview.example.com/  deployed").as_deref(),
            Some("https://pr-12.preview.example.com/")
        );
        assert_eq!(
            url_in_line("  http://10.0.0.5:8080\r").as_deref(),
            Some("http://10.0.0.5:8080")
        );
        assert_eq!(url_in_line("deployed to https://x.example.com"), None);
        assert_eq!(url_in_line("ftp://x"), None);
        assert_eq!(
            base_url("https://pr-12.preview.example.com/").as_deref(),
            Some("https://pr-12.preview.example.com")
        );
        assert_eq!(
            base_url(" http://127.0.0.1:9000/app ").as_deref(),
            Some("http://127.0.0.1:9000/app")
        );
        for bad in ["", "preview.example.com", "ftp://x.example.com", "http://"] {
            assert_eq!(base_url(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn seed_output_parses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("env.out");
        std::fs::write(
            &path,
            "# comment\nA=1\nexport B=\"two words\"\nnot a line\n1BAD=x\n",
        )
        .unwrap();
        let env = read_env_out(&path);
        assert_eq!(env.len(), 2);
        assert_eq!(env["B"], "two words");
    }
}
