//! The `remote` target: `[app.remote]` run in the card's worktree to deploy
//! the branch somewhere reachable over the network, and the same stages run
//! against the URL the deploy reports.
//!
//! Everything about a remote target is in the repository's manifest; the
//! only setting outside it is which boards use it. The script contract (what
//! the scripts receive, how `start` reports the URL) is in
//! `docs/scripts.md#remote-targets`.

use async_trait::async_trait;
use uuid::Uuid;

use super::{Env, ReproError, ReproTarget, ReproductionTarget, RunStatus, StandUpRequest};
use crate::services::{
    config::AppRunsConfig,
    integrations::Descriptor,
    scripts::{
        AppRun, AppRunStatus, RunStep, ScriptRunner, SlotReservation, StandUpReport, StepReport,
    },
};

#[derive(Clone)]
pub struct RemoteTarget {
    runner: ScriptRunner,
    /// `[app.remote] max_parallel` of the manifest the target was built from.
    max_parallel: usize,
}

impl RemoteTarget {
    pub fn new(runner: ScriptRunner, max_parallel: usize) -> Self {
        Self {
            runner,
            max_parallel: max_parallel.max(1),
        }
    }

    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            ReproductionTarget::Remote.as_str(),
            "Remote deploy",
            "Runs [app.remote] from the repository's .prepcook/scripts.toml to deploy the \
             branch to a preview environment, dev tenant or staging slot, and reproduces and \
             verifies against the URL the deploy reports. Everything it runs is in the manifest; \
             choose it per board.",
            Vec::new(),
            "docs/scripts.md#remote-targets",
        )
    }

    /// Nothing global to check: each repository's `[app.remote]` is tried
    /// from the Scripts panel.
    pub(super) fn test(_app_runs: &AppRunsConfig) -> Result<String, String> {
        Ok(
            "Remote targets are described per repository in [app.remote]; \
            Settings → Repositories → Scripts deploys one with Test stand-up."
                .to_string(),
        )
    }
}

#[async_trait]
impl ReproTarget for RemoteTarget {
    fn kind(&self) -> &'static str {
        ReproductionTarget::Remote.as_str()
    }

    fn try_reserve(&self) -> Option<SlotReservation> {
        self.runner.try_reserve_remote(self.max_parallel)
    }

    async fn stand_up(&self, req: StandUpRequest) -> Result<AppRun, ReproError> {
        let (worktree, opts) = req.into_parts();
        self.runner
            .stand_up_as(ReproductionTarget::Remote, &worktree, opts)
            .await
            .map_err(ReproError::Run)
    }

    async fn seed(&self, run: &AppRun) -> StepReport {
        self.runner.seed(run).await
    }

    async fn test(&self, run: &AppRun) -> StepReport {
        self.runner.test(run).await
    }

    async fn verify(&self, req: StandUpRequest) -> StandUpReport {
        let (worktree, opts) = req.into_parts();
        self.runner
            .verify_as(ReproductionTarget::Remote, &worktree, opts)
            .await
    }

    async fn run_step(&self, req: StandUpRequest, step: RunStep) -> StandUpReport {
        let (worktree, opts) = req.into_parts();
        self.runner
            .run_one_as(ReproductionTarget::Remote, &worktree, step, opts)
            .await
    }

    async fn tear_down(&self, run: &AppRun) {
        self.runner.tear_down(run).await;
    }

    async fn restart(&self, run_id: Uuid) -> Result<(AppRunStatus, StandUpReport), String> {
        self.runner.restart(run_id).await
    }

    fn session_env(&self, run: &AppRun) -> Env {
        run.session_env()
    }

    /// The ready probe against the deployed URL.
    async fn status(&self, run: &AppRun) -> RunStatus {
        self.runner.probe(run).await
    }
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, path::Path, time::Duration};

    use super::*;
    use crate::services::scripts::{RunnerSettings, StandUpOutcome, Step};

    /// A port nothing is listening on, for the fake deploy's server.
    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// A git worktree on branch `preview-branch` holding `manifest`.
    fn worktree(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".prepcook")).unwrap();
        std::fs::write(dir.path().join(".prepcook/scripts.toml"), manifest).unwrap();
        let _ = std::process::Command::new("git")
            .args(["init", "-q", "-b", "preview-branch"])
            .current_dir(dir.path())
            .status();
        dir
    }

    /// A deploy that starts a server on `port` in the background (its pid in
    /// `server.pid`), records what it was told, and reports the URL the way
    /// `report` says; a stop that kills it and leaves `stopped`.
    fn deploy_manifest(port: u16, report: &str, extra: &str) -> String {
        format!(
            r#"
[app]
setup = "echo installing"
build = "touch built.marker"
start = "echo the local start must not run; exit 9"

[app.remote]
start = """
nohup python3 -m http.server {port} --bind 127.0.0.1 >/dev/null 2>&1 &
echo $! > server.pid
echo "$PREPCOOK_TICKET_KEY|$PREPCOOK_BRANCH|${{PREPCOOK_PORT:-no-port}}" > deploy.seen
{report}
"""
ready = {{ url = "/", timeout_secs = 30 }}
seed = "echo PREPCOOK_TEST_USER_EMAIL=remote@example.com >> $PREPCOOK_ENV_OUT"
test = "python3 -c \"import urllib.request as u, os; u.build_opener(u.ProxyHandler({{}})).open(os.environ['PREPCOOK_APP_URL'])\""
stop = "kill $(cat server.pid); echo \"$PREPCOOK_APP_URL|${{PREPCOOK_TARGET_REF:-}}\" > stopped"
{extra}
[app.remote.timeouts]
start = 30
"#
        )
    }

    async fn wait_until_down(port: u16) -> bool {
        for _ in 0..40 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err()
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        false
    }

    fn target(runner: &ScriptRunner, root: &Path) -> RemoteTarget {
        let manifest = runner.load_manifest(root).unwrap();
        RemoteTarget::new(
            runner.clone(),
            manifest.app.remote.unwrap().max_parallel as usize,
        )
    }

    #[tokio::test]
    async fn a_deploy_that_writes_its_url_to_env_out_stands_up_and_is_destroyed() {
        let port = free_port();
        let dir = worktree(&deploy_manifest(
            port,
            &format!(
                "echo PREPCOOK_APP_URL=http://127.0.0.1:{port}/ >> $PREPCOOK_ENV_OUT\n\
                 echo PREPCOOK_TARGET_REF=preview-42 >> $PREPCOOK_ENV_OUT\n\
                 echo PREVIEW_ADMIN_USER=admin >> $PREPCOOK_ENV_OUT"
            ),
            "",
        ));
        let runner = ScriptRunner::new(RunnerSettings::from_config("4720-4729", 2));
        let target = target(&runner, dir.path());
        assert_eq!(target.kind(), "remote");

        let run = target
            .stand_up(StandUpRequest {
                ticket_key: "BUG-7".to_string(),
                ..StandUpRequest::new(dir.path())
            })
            .await
            .unwrap_or_else(|e| panic!("{e}: {:?}", e.clone().into_report()));
        assert_eq!(run.target_kind, ReproductionTarget::Remote);
        assert_eq!(run.url, format!("http://127.0.0.1:{port}"));
        assert_eq!(run.port, None);
        assert_eq!(run.target_ref.as_deref(), Some("preview-42"));
        let steps: Vec<Step> = run.steps.iter().map(|s| s.step).collect();
        assert_eq!(
            steps,
            [Step::Setup, Step::Start, Step::Ready, Step::Seed],
            "no `[app] build` and no local start"
        );
        assert!(!dir.path().join("built.marker").exists());
        let seen = std::fs::read_to_string(dir.path().join("deploy.seen")).unwrap();
        assert_eq!(seen.trim(), "BUG-7|preview-branch|no-port");
        assert_eq!(runner.remote_slots_in_use(), 1);
        assert_eq!(runner.slots_in_use(), 0, "local slots are separate");

        let env = target.session_env(&run);
        assert_eq!(env.get("PREPCOOK_APP_URL"), Some(&run.url));
        assert!(!env.contains_key("PREPCOOK_PORT"));
        assert_eq!(
            env.get("PREPCOOK_TEST_USER_EMAIL").map(String::as_str),
            Some("remote@example.com")
        );
        assert_eq!(
            env.get("PREVIEW_ADMIN_USER").map(String::as_str),
            Some("admin")
        );
        assert!(!env.contains_key("PREPCOOK_TARGET_REF"));
        assert!(run.env.keys().all(|k| k != "PREPCOOK_APP_URL"));

        let status = target.status(&run).await;
        assert!(status.up, "{status:?}");
        let test = target.test(&run).await;
        assert!(test.succeeded(), "{test:?}");

        target.tear_down(&run).await;
        assert!(wait_until_down(port).await, "stop destroyed the deploy");
        let stopped = std::fs::read_to_string(dir.path().join("stopped")).unwrap();
        assert_eq!(
            stopped.trim(),
            format!("http://127.0.0.1:{port}|preview-42")
        );
        assert_eq!(runner.remote_slots_in_use(), 0);
        assert!(!target.status(&run).await.up);
    }

    #[tokio::test]
    async fn a_deploy_that_prints_its_url_on_stdout_is_verified() {
        let port = free_port();
        let dir = worktree(&deploy_manifest(
            port,
            &format!(
                "echo https://an-earlier-line.example.com\n\
                 echo http://127.0.0.1:{port}\n\
                 echo deploy finished\n\
                 echo http://stderr.example.com >&2"
            ),
            "",
        ));
        let runner = ScriptRunner::new(RunnerSettings::from_config("4730-4739", 2));
        let target = target(&runner, dir.path());

        let report = target.verify(StandUpRequest::new(dir.path())).await;
        assert_eq!(report.outcome, StandUpOutcome::Ok, "{report:?}");
        assert_eq!(report.last_step().unwrap().step, Step::Test);
        assert!(wait_until_down(port).await, "verify tears the deploy down");
        assert!(dir.path().join("stopped").exists());
        assert_eq!(runner.remote_slots_in_use(), 0);
    }

    #[tokio::test]
    async fn a_deploy_that_reports_no_url_is_a_runner_fault_and_is_still_destroyed() {
        let port = free_port();
        let dir = worktree(&deploy_manifest(port, "echo deployed somewhere", ""));
        let runner = ScriptRunner::new(RunnerSettings::from_config("4740-4749", 2));
        let error = target(&runner, dir.path())
            .stand_up(StandUpRequest::new(dir.path()))
            .await
            .unwrap_err();
        let report = error.into_report();
        assert!(report.outcome.is_runner_fault(), "{report:?}");
        assert!(
            report
                .message
                .as_deref()
                .unwrap()
                .contains("PREPCOOK_APP_URL"),
            "{report:?}"
        );
        assert!(
            dir.path().join("stopped").exists(),
            "a deploy that ran is always stopped"
        );
        assert!(wait_until_down(port).await);
        assert_eq!(runner.remote_slots_in_use(), 0);
    }

    #[tokio::test]
    async fn remote_runs_have_their_own_limit() {
        let port = free_port();
        let dir = worktree(&deploy_manifest(
            port,
            &format!("echo http://127.0.0.1:{port}"),
            "max_parallel = 1",
        ));
        let runner = ScriptRunner::new(RunnerSettings::from_config("4750-4759", 4));
        let target = target(&runner, dir.path());
        let held = target.try_reserve().expect("one remote slot");
        assert!(target.try_reserve().is_none());
        assert!(runner.try_reserve().is_some(), "local slots are untouched");

        let report = target
            .stand_up(StandUpRequest::new(dir.path()))
            .await
            .unwrap_err()
            .into_report();
        assert_eq!(report.outcome, StandUpOutcome::Busy);
        assert!(!dir.path().join("deploy.seen").exists(), "nothing ran");
        drop(held);
        assert_eq!(runner.remote_slots_in_use(), 0);
        assert!(target.try_reserve().is_some());
    }

    #[tokio::test]
    async fn a_board_set_to_remote_without_the_table_is_refused_before_anything_runs() {
        let dir = worktree("[app]\nsetup = \"touch ran\"\nstart = \"sleep 30\"\n");
        let runner = ScriptRunner::default();
        let report = RemoteTarget::new(runner.clone(), 1)
            .stand_up(StandUpRequest::new(dir.path()))
            .await
            .unwrap_err()
            .into_report();
        assert_eq!(report.outcome, StandUpOutcome::TargetUnconfigured);
        assert!(report.outcome.is_runner_fault());
        assert!(
            report.message.as_deref().unwrap().contains("[app.remote]"),
            "{report:?}"
        );
        assert!(!dir.path().join("ran").exists());
        assert_eq!(runner.remote_slots_in_use(), 0);
    }

    #[tokio::test]
    async fn a_restart_redeploys_in_place_when_the_manifest_says_how() {
        let port = free_port();
        let dir = worktree(&deploy_manifest(
            port,
            &format!(
                "echo PREPCOOK_APP_URL=http://127.0.0.1:{port} >> $PREPCOOK_ENV_OUT\n\
                 echo PREPCOOK_TARGET_REF=preview-9 >> $PREPCOOK_ENV_OUT"
            ),
            "restart = \"echo \\\"$PREPCOOK_APP_URL|$PREPCOOK_TARGET_REF\\\" > redeployed\"",
        ));
        let runner = ScriptRunner::new(RunnerSettings::from_config("4760-4769", 2));
        let target = target(&runner, dir.path());
        let run = target
            .stand_up(StandUpRequest::new(dir.path()))
            .await
            .unwrap();
        let (url, workspace) = (run.url.clone(), Uuid::new_v4());
        runner.adopt(run, workspace);
        let run_id = runner.current_for_worktree(dir.path()).unwrap().run_id;
        assert_eq!(
            runner.adopted_kind(run_id),
            Some(ReproductionTarget::Remote)
        );

        let (status, report) = target.restart(run_id).await.unwrap();
        assert_eq!(report.outcome, StandUpOutcome::Ok);
        assert_eq!(status.url, url, "the redeploy kept its address");
        assert_eq!(status.target_kind, ReproductionTarget::Remote);
        let redeployed = std::fs::read_to_string(dir.path().join("redeployed")).unwrap();
        assert_eq!(redeployed.trim(), format!("{url}|preview-9"));
        assert!(
            !dir.path().join("stopped").exists(),
            "a redeploy in place does not destroy first"
        );
        assert_eq!(runner.remote_slots_in_use(), 1);

        runner.tear_down_workspace(workspace).await;
        assert!(wait_until_down(port).await);
        assert_eq!(runner.remote_slots_in_use(), 0);
    }

    #[test]
    fn the_descriptor_has_nothing_global() {
        let descriptor = RemoteTarget::descriptor();
        assert_eq!(descriptor.kind, "remote");
        assert!(descriptor.fields.is_empty());
        assert!(RemoteTarget::test(&AppRunsConfig::default()).is_ok());
    }
}
