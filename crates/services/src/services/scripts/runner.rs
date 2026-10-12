//! Runs a repository's `.prepcook/scripts.toml` in a worktree: stands the app up
//! on a port this process allocates, tests it, and tears it down.
//!
//! A stood-up app is an [`AppRun`]. Its start command runs in its own process
//! group, so tearing it down kills everything the command spawned. Teardown is
//! explicit ([`ScriptRunner::tear_down`]) and also happens when the last handle
//! to a run is dropped, so a run that a caller forgets cannot hold a port, a
//! slot or a process past its owner.
//!
//! Runs are capped at `app_runs.max_parallel`. A caller that cannot get a slot
//! gets [`StandUpOutcome::Busy`] immediately rather than a queue: the
//! orchestrator leaves the card in its column and asks again on its next tick.
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
    manifest::{self, Manifest, ManifestError},
    ports::{self, PortAllocator},
};
use crate::services::secrets::{self, Secrets};

/// Lines kept for a step's `log_tail`.
const LOG_TAIL_LINES: usize = 60;
/// Finished script runs kept for `GET /api/script-runs/{id}`.
const SCRIPT_RUN_HISTORY: usize = 50;
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Bound on reading what a finished command left in its pipes. A command that
/// backgrounds a child holding stdout open would otherwise hold the step.
const PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

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
/// secret not set, no free port — and a person has to act.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum StandUpOutcome {
    Ok,
    ManifestMissing,
    ManifestInvalid,
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
    fn new(outcome: StandUpOutcome, steps: Vec<StepReport>, message: Option<String>) -> Self {
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
    /// A slot taken earlier with [`ScriptRunner::try_reserve`]; without one,
    /// stand-up takes its own or answers `Busy`.
    pub reservation: Option<SlotReservation>,
    /// Where the run's output goes; a fresh store when absent.
    pub log: Option<Arc<MsgStore>>,
    pub run_id: Option<Uuid>,
    /// A port to try before the range.
    pub preferred_port: Option<u16>,
}

/// The parts of a run teardown needs, shared by every handle to it.
struct RunInner {
    run_id: Uuid,
    port: u16,
    url: String,
    worktree: PathBuf,
    ticket_key: String,
    target_branch: Option<String>,
    manifest: Manifest,
    /// What every script receives on top of the process environment.
    script_env: Vec<(String, String)>,
    secret_env: Vec<(String, String)>,
    run_dir: PathBuf,
    log: Arc<MsgStore>,
    started_at: DateTime<Utc>,
    child: Mutex<Option<AsyncGroupChild>>,
    start_tail: Arc<Mutex<VecDeque<String>>>,
    ports: Arc<PortAllocator>,
    slot: Mutex<Option<SlotReservation>>,
    torn_down: AtomicBool,
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
        let _ = std::fs::remove_dir_all(&self.run_dir);
    }
}

/// A stood-up app. Cheap to clone; the app stays up until
/// [`ScriptRunner::tear_down`] or until the last clone is dropped.
#[derive(Clone)]
pub struct AppRun {
    pub run_id: Uuid,
    pub port: u16,
    pub url: String,
    /// What the seed script wrote to `$PREPCOOK_ENV_OUT`.
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
            .field("url", &self.url)
            .field("worktree", &self.worktree)
            .field("env", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AppRun {
    /// The variables an agent session working against this run receives: the
    /// app's address, the seed's output and every secret.
    pub fn session_env(&self) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = self.inner.secret_env.iter().cloned().collect();
        env.extend(self.env.clone());
        env.insert("PREPCOOK_APP_URL".into(), self.url.clone());
        env.insert("PREPCOOK_PORT".into(), self.port.to_string());
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
            worktree: self.worktree.to_string_lossy().into_owned(),
            url: self.url.clone(),
            port: self.port,
            seed_env_names: self.env.keys().cloned().collect(),
            started_at: self.started_at,
            workspace_id,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.inner.manifest
    }
}

/// A run as the API shows it. Names of the seed's variables, never values.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AppRunStatus {
    pub run_id: Uuid,
    pub worktree: String,
    pub url: String,
    pub port: u16,
    pub seed_env_names: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub workspace_id: Option<Uuid>,
}

/// A Settings "Test run": in progress until `finished_at` is set.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ScriptRunState {
    pub run_id: Uuid,
    pub step: RunStep,
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
    ports: Arc<PortAllocator>,
    slots: Arc<AtomicUsize>,
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
}

/// Runs `.prepcook/scripts.toml`. Held by the deployment; clones share state.
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
                ports: Arc::new(PortAllocator::new()),
                slots: Arc::new(AtomicUsize::new(0)),
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

    fn settings(&self) -> RunnerSettings {
        self.inner
            .settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn load_manifest(&self, worktree: &Path) -> Result<Manifest, ManifestError> {
        manifest::load_manifest(worktree)
    }

    /// Takes a slot, or `None` when all `max_parallel` are in use.
    pub fn try_reserve(&self) -> Option<SlotReservation> {
        let max = self.settings().max_parallel;
        self.inner
            .slots
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                (used < max).then_some(used + 1)
            })
            .ok()
            .map(|_| SlotReservation {
                slots: self.inner.slots.clone(),
            })
    }

    pub fn slots_in_use(&self) -> usize {
        self.inner.slots.load(Ordering::SeqCst)
    }

    /// Whether a stand-up in this process already ran `setup` successfully in
    /// `worktree`, so the workspace setup hook need not run it again.
    pub fn has_completed_setup(&self, worktree: &Path) -> bool {
        lock(&self.inner.setup_done).contains(worktree)
    }

    // -- Stand-up, test, teardown ------------------------------------------

    /// setup → build → start → ready → seed, in `worktree`. On success the app
    /// is up and the returned run owns it.
    pub async fn stand_up(
        &self,
        worktree: &Path,
        opts: StandUpOptions,
    ) -> Result<AppRun, StandUpReport> {
        self.execute(worktree, opts, Plan::STAND_UP).await
    }

    /// Runs `test` against a stood-up app, with `$PREPCOOK_CHANGED_FILES` set.
    pub async fn test(&self, run: &AppRun) -> StepReport {
        let inner = &run.inner;
        let Some(command) = inner.manifest.app.test.clone() else {
            return StepReport::note(Step::Test, "no `test` in [app]; nothing to run");
        };
        let mut env = inner.script_env.clone();
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
            Duration::from_secs(inner.manifest.app.timeouts.test),
            &inner.log,
        )
        .await
    }

    /// Stand-up, then `test`, then teardown: the pre-merge verification.
    pub async fn verify(&self, worktree: &Path, opts: StandUpOptions) -> StandUpReport {
        let run = match self.stand_up(worktree, opts).await {
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

    /// Runs `stop` (bounded by its timeout), kills the start process group and
    /// gives back the port and the slot. Always completes, whatever `stop`
    /// does; a second call is a no-op.
    pub async fn tear_down(&self, run: &AppRun) {
        lock(&self.inner.adopted).remove(&run.run_id);
        self.tear_down_inner(&run.inner).await;
    }

    async fn tear_down_inner(&self, inner: &Arc<RunInner>) {
        if inner.torn_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let child = lock(&inner.child).take();
        if child.is_some()
            && let Some(stop) = inner.manifest.app.stop.clone()
        {
            let report = run_command(
                Step::Stop,
                &stop,
                &inner.worktree,
                &inner.script_env,
                Duration::from_secs(inner.manifest.app.timeouts.stop),
                &inner.log,
            )
            .await;
            if !report.succeeded() {
                warn!(
                    "app run {}: stop {} — killing the process group regardless",
                    inner.run_id,
                    if report.timed_out {
                        "timed out".to_string()
                    } else {
                        format!("exited {:?}", report.exit)
                    }
                );
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
    /// once it is ready (or once `start`'s timeout passes with the process
    /// still up, when there is no ready check).
    pub async fn run_one(
        &self,
        worktree: &Path,
        step: RunStep,
        opts: StandUpOptions,
    ) -> StandUpReport {
        let plan = match step {
            RunStep::StandUp => Plan::STAND_UP,
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
        };
        match self.execute(worktree, opts, plan).await {
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
    ) -> Result<AppRun, StandUpReport> {
        let log = opts.log.clone().unwrap_or_default();
        let fail = |outcome: StandUpOutcome, message: String| -> Result<AppRun, StandUpReport> {
            log.push(LogMsg::Stderr(format!("{}\n", secrets::redact(&message))));
            Err(StandUpReport::new(outcome, Vec::new(), Some(message)))
        };

        let reservation = if plan.start {
            match opts.reservation {
                Some(reservation) => Some(reservation),
                None => match self.try_reserve() {
                    Some(reservation) => Some(reservation),
                    None => {
                        return fail(
                            StandUpOutcome::Busy,
                            format!(
                                "all {} app-run slots are in use",
                                self.settings().max_parallel
                            ),
                        );
                    }
                },
            }
        } else {
            opts.reservation
        };

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
        let range = self.settings().port_range;
        let port = match self
            .inner
            .ports
            .allocate_preferring(&range, run_id, opts.preferred_port)
        {
            Ok(port) => port,
            Err(e) => return fail(StandUpOutcome::PortUnavailable, e.to_string()),
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

        let url = format!("http://127.0.0.1:{port}");
        let secret_env = secrets.env_vars();
        let mut script_env: Vec<(String, String)> = vec![
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
        script_env.extend(secret_env.iter().cloned());

        let inner = Arc::new(RunInner {
            run_id,
            port,
            url: url.clone(),
            worktree: worktree.to_path_buf(),
            ticket_key: opts.ticket_key.clone(),
            target_branch: opts.target_branch.clone(),
            manifest: manifest.clone(),
            script_env: script_env.clone(),
            secret_env,
            run_dir,
            log: log.clone(),
            started_at: Utc::now(),
            child: Mutex::new(None),
            start_tail: Arc::new(Mutex::new(VecDeque::new())),
            ports: self.inner.ports.clone(),
            slot: Mutex::new(reservation),
            torn_down: AtomicBool::new(false),
        });
        {
            let mut live = lock(&self.inner.live);
            live.retain(|_, weak| weak.strong_count() > 0);
            live.insert(run_id, Arc::downgrade(&inner));
        }
        info!("app run {run_id}: {} on port {port}", worktree.display());

        let mut steps: Vec<StepReport> = Vec::new();
        let timeouts = manifest.app.timeouts;
        let simple = [
            (
                plan.setup,
                Step::Setup,
                manifest.app.setup.as_deref(),
                timeouts.setup,
            ),
            (
                plan.build,
                Step::Build,
                manifest.app.build.as_deref(),
                timeouts.build,
            ),
        ];
        for (wanted, step, command, timeout) in simple {
            if !wanted {
                continue;
            }
            let Some(command) = command else {
                if !plan.start {
                    steps.push(StepReport::note(
                        step,
                        format!("no `{}` in [app]; nothing to run", step.label()),
                    ));
                }
                continue;
            };
            let report = run_command(
                step,
                command,
                worktree,
                &script_env,
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

        if plan.start {
            match self.start_and_wait(&inner, plan.seed).await {
                Ok(reports) => steps.extend(reports),
                Err((reports, outcome)) => {
                    steps.extend(reports);
                    let report = self.abort_with(&inner, steps, outcome).await;
                    return Err(report);
                }
            }
        }

        let mut seed_env = BTreeMap::new();
        if plan.seed {
            match manifest.app.seed.as_deref() {
                Some(command) => {
                    let report = run_command(
                        Step::Seed,
                        command,
                        worktree,
                        &script_env,
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
                    for (name, value) in &seed_env {
                        if secrets::is_sensitive_name(name) {
                            secrets::register_redaction(value);
                        }
                    }
                    if !seed_env.is_empty() {
                        log.push_stdout(format!(
                            "seed wrote {}\n",
                            seed_env.keys().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
                }
                None if !plan.start => steps.push(StepReport::note(
                    Step::Seed,
                    "no `seed` in [app]; nothing to run",
                )),
                None => {}
            }
        }

        for (wanted, step, command, timeout) in [
            (
                plan.test,
                Step::Test,
                manifest.app.test.as_deref(),
                timeouts.test,
            ),
            (
                plan.stop,
                Step::Stop,
                manifest.app.stop.as_deref(),
                timeouts.stop,
            ),
        ] {
            if !wanted {
                continue;
            }
            let Some(command) = command else {
                steps.push(StepReport::note(
                    step,
                    format!("no `{}` in [app]; nothing to run", step.label()),
                ));
                continue;
            };
            let mut env = script_env.clone();
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
            port,
            url,
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
        self.abort_with(inner, steps, StandUpOutcome::failed_step(step))
            .await
    }

    async fn abort_with(
        &self,
        inner: &Arc<RunInner>,
        steps: Vec<StepReport>,
        outcome: StandUpOutcome,
    ) -> StandUpReport {
        inner.log.push(LogMsg::Stderr(format!(
            "stand-up stopped: {}\n",
            outcome.label()
        )));
        self.tear_down_inner(inner).await;
        StandUpReport::new(outcome, steps, None)
    }

    /// Spawns `start` in its own process group, waits for it to listen on its
    /// port within `timeouts.start`, then runs the ready check within its own
    /// timeout. `start` dying at any point is `StartFailed`; never listening,
    /// or never passing the check, is `NotReady`. Without a ready check, a
    /// listening port is ready — or, for a single-step test run
    /// (`wait_for_listen` false), a process still up when `timeouts.start`
    /// runs out.
    async fn start_and_wait(
        &self,
        inner: &Arc<RunInner>,
        wait_for_listen: bool,
    ) -> Result<Vec<StepReport>, (Vec<StepReport>, StandUpOutcome)> {
        let manifest = &inner.manifest;
        let timeouts = manifest.app.timeouts;
        let command = manifest.app.start.clone();
        inner
            .log
            .push_stdout(format!("$ {}    # start\n", secrets::redact(&command)));
        let spawned_at = Instant::now();
        let mut child = match shell_command(&command, &inner.worktree, &inner.script_env)
            .group_spawn_no_window()
        {
            Ok(child) => child,
            Err(e) => {
                let report = StepReport {
                    step: Step::Start,
                    exit: None,
                    timed_out: false,
                    duration_ms: 0,
                    log_tail: format!("could not start the shell: {e}"),
                };
                return Err((vec![report], StandUpOutcome::StartFailed));
            }
        };
        if let Some(stdout) = child.inner().stdout.take() {
            pump(stdout, inner.log.clone(), inner.start_tail.clone(), false);
        }
        if let Some(stderr) = child.inner().stderr.take() {
            pump(stderr, inner.log.clone(), inner.start_tail.clone(), true);
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
        let ready_check = manifest.app.ready.clone();
        let listen_required = wait_for_listen || ready_check.is_some();
        loop {
            if let Some(code) = exited() {
                return Err((vec![start_report(code, false)], StandUpOutcome::StartFailed));
            }
            if port_is_listening(inner.port).await {
                break;
            }
            if Instant::now() >= listen_deadline {
                if !listen_required {
                    // A single-step test run with no ready check: still up
                    // after the start timeout counts.
                    return Ok(vec![start_report(None, false)]);
                }
                inner.log.push(LogMsg::Stderr(format!(
                    "start did not listen on port {} within {}s\n",
                    inner.port, timeouts.start
                )));
                return Err((vec![start_report(None, true)], StandUpOutcome::NotReady));
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
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| (vec![started.clone()], StandUpOutcome::Error(e.to_string())))?;
        let mut last = String::new();
        loop {
            if let Some(code) = exited() {
                return Err((vec![start_report(code, false)], StandUpOutcome::StartFailed));
            }
            let passed = match (&ready.url, &ready.command) {
                (Some(path), _) => {
                    let target = format!("{}{path}", inner.url);
                    match client.get(&target).send().await {
                        Ok(response) if response.status().is_success() => true,
                        Ok(response) => {
                            last = format!("GET {target} → {}", response.status());
                            false
                        }
                        Err(e) => {
                            last = format!("GET {target} → {e}");
                            false
                        }
                    }
                }
                (None, Some(command)) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let report = run_command_quiet(
                        command,
                        &inner.worktree,
                        &inner.script_env,
                        remaining.max(Duration::from_secs(1)),
                    )
                    .await;
                    last = report.log_tail;
                    report.exit == Some(0) && !report.timed_out
                }
                (None, None) => true,
            };
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
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
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
            self.tear_down_inner(inner).await;
        }
    }

    /// The run serving `path` — a worktree, a directory inside one, or the
    /// workspace directory that holds it.
    pub fn current_for_worktree(&self, path: &Path) -> Option<AppRunStatus> {
        let matches = |worktree: &Path| {
            worktree == path || path.starts_with(worktree) || worktree.starts_with(path)
        };
        if let Some(adoption) = lock(&self.inner.adopted)
            .values()
            .find(|a| matches(&a.run.worktree))
        {
            return Some(adoption.run.status(Some(adoption.workspace_id)));
        }
        lock(&self.inner.live)
            .values()
            .filter_map(Weak::upgrade)
            .find(|inner| matches(&inner.worktree) && !inner.torn_down.load(Ordering::SeqCst))
            .map(|inner| AppRunStatus {
                run_id: inner.run_id,
                worktree: inner.worktree.to_string_lossy().into_owned(),
                url: inner.url.clone(),
                port: inner.port,
                seed_env_names: Vec::new(),
                started_at: inner.started_at,
                workspace_id: None,
            })
    }

    /// Tears a workspace's run down and stands it up again in the same
    /// worktree, on the same port when it is still free, keeping its binding
    /// to the agent process. For an agent testing a candidate change.
    pub async fn restart(&self, run_id: Uuid) -> Result<(AppRunStatus, StandUpReport), String> {
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
        let opts = StandUpOptions {
            ticket_key: old.inner.ticket_key.clone(),
            target_branch: old.inner.target_branch.clone(),
            reservation: None,
            log: Some(old.log.clone()),
            run_id: None,
            preferred_port: Some(old.port),
        };
        let worktree = old.worktree.clone();
        old.log.push_stdout("restarting\n".to_string());
        self.tear_down(&old).await;
        drop(old);
        match self.stand_up(&worktree, opts).await {
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

    /// Starts `step` in `worktree` in the background and returns its id at
    /// once. Output streams from [`Self::script_run_log`]; the report lands in
    /// [`Self::script_run`] when it finishes.
    pub fn spawn_script_run(&self, worktree: PathBuf, step: RunStep) -> Uuid {
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
            let opts = StandUpOptions {
                log: Some(log.clone()),
                run_id: Some(run_id),
                ..Default::default()
            };
            let report = runner.run_one(&worktree, step, opts).await;
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
    /// variables a stand-up's scripts get.
    pub fn dev_server_env(
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
/// UTF-8 is replaced rather than ending the copy.
fn pump<R>(
    reader: R,
    log: Arc<MsgStore>,
    tail: Arc<Mutex<VecDeque<String>>>,
    stderr: bool,
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
                    let line = secrets::redact(raw.trim_end_matches(['\n', '\r']));
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
    log.push_stdout(format!(
        "$ {}    # {}\n",
        secrets::redact(command),
        step.label()
    ));
    let report = run_command_into(step, command, cwd, env, timeout, Some(log)).await;
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
    run_command_into(Step::Ready, command, cwd, env, timeout, None).await
}

async fn run_command_into(
    step: Step,
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
    log: Option<&Arc<MsgStore>>,
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
        pumps.push(pump(stdout, sink.clone(), tail.clone(), false));
    }
    if let Some(stderr) = child.inner().stderr.take() {
        pumps.push(pump(stderr, sink.clone(), tail.clone(), true));
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

/// The `KEY=VALUE` lines a seed script appended to `$PREPCOOK_ENV_OUT`.
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
        assert!((4620..=4639).contains(&run.port));
        assert_eq!(run.url, format!("http://127.0.0.1:{}", run.port));
        assert_eq!(
            run.env.get("PREPCOOK_TEST_USER_EMAIL").map(String::as_str),
            Some("user@example.com")
        );
        assert!(dir.path().join("setup.marker").exists());
        assert!(runner.has_completed_setup(dir.path()));
        assert_eq!(runner.slots_in_use(), 1);
        let session = run.session_env();
        assert_eq!(session.get("PREPCOOK_APP_URL"), Some(&run.url));
        assert_eq!(session.get("PREPCOOK_PORT"), Some(&run.port.to_string()));
        // The seeded password is registered for redaction.
        assert_eq!(
            secrets::redact("pw is pw-from-seed-123"),
            "pw is [redacted]"
        );
        assert!(port_is_listening(run.port).await);
        assert_eq!(
            runner.current_for_worktree(dir.path()).map(|s| s.run_id),
            Some(run.run_id)
        );

        let test = runner.test(&run).await;
        assert!(test.succeeded(), "{test:?}");
        assert!(test.log_tail.contains("testing BUG-1"), "{}", test.log_tail);

        let port = run.port;
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
        let port = run.port;
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
        let port = run.port;
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
        assert_eq!(restarted.0.port, port, "comes back on the same port");
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

        let id = runner.spawn_script_run(dir.path().to_path_buf(), RunStep::Setup);
        let mut state = runner.script_run(id).unwrap();
        for _ in 0..100 {
            if state.finished_at.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            state = runner.script_run(id).unwrap();
        }
        assert_eq!(state.report.unwrap().outcome, StandUpOutcome::Ok);
        assert!(runner.script_run_log(id).is_some());
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
