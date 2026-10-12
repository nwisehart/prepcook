//! Claude seat health monitor.
//!
//! The runner authenticates Claude Code with a subscription **seat** (an OAuth
//! login, not an API key). Its refresh token can expire or be invalidated when
//! the same Claude account signs in elsewhere — and when that happens every
//! agent launch fails with "OAuth session expired and could not be refreshed",
//! silently, until someone notices tickets stalling.
//!
//! This service probes the seat every `CHECK_INTERVAL` by running the `claude`
//! CLI headlessly. On a healthy → unhealthy transition it posts to the Slack
//! webhook once (not every tick) and flips a process-wide flag the app surfaces
//! as a banner (`/api/info` → `claude_seat_degraded`). It clears both when the
//! seat recovers (someone re-runs `claude /login` on the box).
//!
//! Opt-in (`orchestrator.seat_monitor`): it is for a shared runner, and a
//! laptop install must not spawn `claude` on its own. The Settings "Re-check
//! now" button probes on demand whether or not it is on.
//!
//! Only meaningful with a subscription seat; harmless with an API key (the
//! probe just succeeds). No-op if the `claude` binary can't be found.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::services::{config::Config, slack};

/// Probe cadence.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Delay before the first probe, so a just-booted stack isn't flagged mid-startup.
const STARTUP_DELAY: Duration = Duration::from_secs(2 * 60);
/// While the monitor is switched off, how often to look whether it has been
/// switched on, so enabling it in the config takes effect without a restart.
const DISABLED_RECHECK: Duration = Duration::from_secs(60);
/// Hard cap on a single probe; a hung CLI must not wedge the loop.
const PROBE_TIMEOUT: Duration = Duration::from_secs(90);

/// Process-wide seat-degraded flag. Written only by this service; read by the
/// `/api/info` handler to drive the UI banner.
static SEAT_DEGRADED: AtomicBool = AtomicBool::new(false);

/// True when the last seat probe failed (auth expired / not logged in).
pub fn seat_degraded() -> bool {
    SEAT_DEGRADED.load(Ordering::Relaxed)
}

/// Runs the seat probe on demand (e.g. from the Settings "Re-check now" button)
/// and updates the process-wide flag. Returns `true` when the seat is healthy.
/// An inconclusive probe (no `claude` binary / spawn error) leaves the flag
/// unchanged and reports the last-known state, so a transient hiccup can't clear
/// a real outage.
pub async fn recheck_now() -> bool {
    match probe_seat().await {
        Some(healthy) => {
            SEAT_DEGRADED.store(!healthy, Ordering::Relaxed);
            healthy
        }
        None => !seat_degraded(),
    }
}

pub struct SeatMonitorService {
    config: Arc<RwLock<Config>>,
}

impl SeatMonitorService {
    pub async fn spawn(config: Arc<RwLock<Config>>) -> tokio::task::JoinHandle<()> {
        let service = Self { config };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        tokio::time::sleep(STARTUP_DELAY).await;
        let mut announced = false;
        loop {
            if !self.config.read().await.orchestrator.seat_monitor {
                announced = false;
                tokio::time::sleep(DISABLED_RECHECK).await;
                continue;
            }
            if !announced {
                info!("Claude seat monitor on (every {CHECK_INTERVAL:?})");
                announced = true;
            }
            self.check_once().await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    }

    async fn check_once(&self) {
        let healthy = match probe_seat().await {
            Some(healthy) => healthy,
            // Couldn't run the probe at all (no claude binary): don't flap the
            // flag or alert on an inconclusive result — leave state as-is.
            None => return,
        };

        let was_degraded = SEAT_DEGRADED.swap(!healthy, Ordering::Relaxed);

        if !healthy && !was_degraded {
            warn!("Claude seat probe failed — the subscription login needs a refresh");
            let webhook = self.config.read().await.slack_webhook_url.clone();
            slack::notify_if_configured(
                webhook.as_deref(),
                ":warning: *Prep Cook: Claude seat expired.* Agents can't run \
                 until it's re-authenticated. On the runner:\n\
                 `sudo docker exec -it prepcook claude /login` \
                 (sign in as the agent's Claude account, then paste the code).",
            )
            .await;
        } else if healthy && was_degraded {
            info!("Claude seat recovered");
            let webhook = self.config.read().await.slack_webhook_url.clone();
            slack::notify_if_configured(
                webhook.as_deref(),
                ":white_check_mark: *Prep Cook: Claude seat is healthy again.*",
            )
            .await;
        }
    }
}

/// Runs a trivial headless `claude` call and classifies the result.
/// `Some(true)` = seat works, `Some(false)` = auth failed, `None` = probe
/// couldn't run (binary missing / spawn error — inconclusive).
async fn probe_seat() -> Option<bool> {
    // Inherits the process environment, including CLAUDE_CONFIG_DIR (the runner
    // points it at the mounted volume where the seat lives).
    //
    // --strict-mcp-config with no --mcp-config loads ZERO MCP servers: the
    // probe only verifies auth, and should not pay the tool-schema context
    // cost of every server the user has configured for a one-word reply.
    let mut command = tokio::process::Command::new("claude");
    command
        .arg("-p")
        .arg("--strict-mcp-config")
        .arg("reply with the single word OK")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            warn!(
                ?error,
                "seat monitor: could not spawn `claude` — skipping probe"
            );
            return None;
        }
    };

    let output = match tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            warn!(?error, "seat monitor: `claude` probe errored");
            return None;
        }
        Err(_) => {
            warn!("seat monitor: `claude` probe timed out");
            // A hang is more likely environmental than an expired seat; stay
            // inconclusive rather than false-alarm.
            return None;
        }
    };

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();

    // Explicit auth-failure signals seen from Claude Code when the seat is
    // expired / logged out.
    let auth_failed = combined.contains("oauth")
        || combined.contains("not logged in")
        || combined.contains("please run /login")
        || combined.contains("could not be refreshed")
        || combined.contains("authentication");

    if auth_failed {
        return Some(false);
    }
    // Otherwise trust the exit status: success = seat works.
    Some(output.status.success())
}
