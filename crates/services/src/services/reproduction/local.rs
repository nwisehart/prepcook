//! The `local` target: the app built and started from `[app]` in the card's
//! worktree, on a port from `app_runs.port_range`, at most
//! `app_runs.max_parallel` at once.

use std::path::Path;

use async_trait::async_trait;
use uuid::Uuid;

use super::{Env, ReproError, ReproTarget, ReproductionTarget, RunStatus, StandUpRequest};
use crate::services::{
    config::AppRunsConfig,
    integrations::{Descriptor, Field, FieldKind},
    scripts::{
        AppRun, AppRunStatus, RunStep, ScriptRunner, SlotReservation, StandUpReport, StepReport,
        ports,
    },
};

#[derive(Clone)]
pub struct LocalTarget {
    runner: ScriptRunner,
}

impl LocalTarget {
    pub fn new(runner: ScriptRunner) -> Self {
        Self { runner }
    }

    pub fn descriptor() -> Descriptor {
        Descriptor::new(
            ReproductionTarget::Local.as_str(),
            "This machine",
            "Builds and starts the app from [app] in the card's worktree and serves it on a \
             port of this machine.",
            vec![
                Field::new(
                    "port_range",
                    "Port range",
                    FieldKind::Text,
                    true,
                    "Ports handed to app runs, `<first>-<last>` inclusive; each run gets one as \
                     $PREPCOOK_PORT.",
                ),
                Field::new(
                    "max_parallel",
                    "Apps at once",
                    FieldKind::Number,
                    true,
                    "Local apps up at the same time. Each is a full app stack, so this is \
                     bounded by the machine's memory; a stand-up past it waits for the next scan.",
                ),
            ],
            "docs/scripts.md",
        )
    }

    /// Parses the range and counts the ports in it nothing is bound to.
    pub(super) fn test(app_runs: &AppRunsConfig) -> Result<String, String> {
        let range = ports::parse_range(&app_runs.port_range).map_err(|e| e.to_string())?;
        if app_runs.max_parallel == 0 {
            return Err("max_parallel must be at least 1".to_string());
        }
        let free = range
            .clone()
            .filter(|port| std::net::TcpListener::bind(("127.0.0.1", *port)).is_ok())
            .count();
        if free == 0 {
            return Err(format!(
                "every port in {} is in use on this machine",
                app_runs.port_range
            ));
        }
        Ok(format!(
            "{}: {free} of {} ports free; up to {} apps at once",
            app_runs.port_range,
            range.count(),
            app_runs.max_parallel
        ))
    }

    /// What the preview's dev server receives: a port from the range held
    /// for `process_id`, and the variables a stand-up's scripts get. The
    /// preview always serves from this machine, whatever the board's target.
    pub fn dev_server_env(
        &self,
        process_id: Uuid,
        worktree: &Path,
    ) -> Result<Env, ports::PortError> {
        self.runner.dev_server_env(process_id, worktree)
    }
}

#[async_trait]
impl ReproTarget for LocalTarget {
    fn kind(&self) -> &'static str {
        ReproductionTarget::Local.as_str()
    }

    fn try_reserve(&self) -> Option<SlotReservation> {
        self.runner.try_reserve()
    }

    async fn stand_up(&self, req: StandUpRequest) -> Result<AppRun, ReproError> {
        let (worktree, opts) = req.into_parts();
        self.runner
            .stand_up_as(ReproductionTarget::Local, &worktree, opts)
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
            .verify_as(ReproductionTarget::Local, &worktree, opts)
            .await
    }

    async fn run_step(&self, req: StandUpRequest, step: RunStep) -> StandUpReport {
        let (worktree, opts) = req.into_parts();
        self.runner
            .run_one_as(ReproductionTarget::Local, &worktree, step, opts)
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

    async fn status(&self, run: &AppRun) -> RunStatus {
        self.runner.probe(run).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::scripts::{RunnerSettings, StandUpOutcome};

    #[tokio::test]
    async fn the_trait_drives_the_local_runner() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".prepcook")).unwrap();
        std::fs::write(
            dir.path().join(".prepcook/scripts.toml"),
            r#"
[app]
start = "exec python3 -m http.server $PREPCOOK_PORT --bind 127.0.0.1"
ready = { url = "/", timeout_secs = 30 }
seed = "echo PREPCOOK_TEST_USER_EMAIL=local@example.com >> $PREPCOOK_ENV_OUT"
test = "test -n \"$PREPCOOK_PORT\""

[app.timeouts]
start = 30
"#,
        )
        .unwrap();
        let runner = ScriptRunner::new(RunnerSettings::from_config("4700-4709", 1));
        let target = LocalTarget::new(runner.clone());
        assert_eq!(target.kind(), "local");

        let reservation = target.try_reserve().expect("one slot");
        assert!(target.try_reserve().is_none(), "max_parallel is 1");
        let run = target
            .stand_up(StandUpRequest {
                reservation: Some(reservation),
                ..StandUpRequest::new(dir.path())
            })
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(run.target_kind, ReproductionTarget::Local);
        let port = run.port.unwrap();
        let env = target.session_env(&run);
        assert_eq!(env.get("PREPCOOK_PORT"), Some(&port.to_string()));
        assert_eq!(
            env.get("PREPCOOK_TEST_USER_EMAIL").map(String::as_str),
            Some("local@example.com")
        );
        let status = target.status(&run).await;
        assert!(status.up, "{status:?}");
        assert!(target.test(&run).await.succeeded());
        assert!(target.seed(&run).await.succeeded());

        target.tear_down(&run).await;
        assert!(!target.status(&run).await.up);
        assert_eq!(runner.slots_in_use(), 0);

        let report = target.verify(StandUpRequest::new(dir.path())).await;
        assert_eq!(report.outcome, StandUpOutcome::Ok, "{report:?}");
        let report = target
            .run_step(StandUpRequest::new(dir.path()), RunStep::StandUp)
            .await;
        assert_eq!(report.outcome, StandUpOutcome::Ok, "{report:?}");
        assert_eq!(runner.slots_in_use(), 0);
    }

    #[test]
    fn the_connection_test_checks_the_range() {
        let ok = LocalTarget::test(&AppRunsConfig {
            port_range: "4710-4712".to_string(),
            ..Default::default()
        })
        .unwrap();
        assert!(ok.contains("of 3 ports free"), "{ok}");
        assert!(
            LocalTarget::test(&AppRunsConfig {
                port_range: "nope".to_string(),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            LocalTarget::test(&AppRunsConfig {
                max_parallel: 0,
                ..Default::default()
            })
            .is_err()
        );
    }
}
