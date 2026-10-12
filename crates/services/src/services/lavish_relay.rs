//! Relays lavish-axi review feedback into agent sessions.
//!
//! Planning playbooks open the review gate headless (`npx lavish-axi <spec>
//! --no-open`) and END their turn — keeping an LLM session alive in a poll
//! loop is fragile (turns time out, and lavish then shows "your agent is not
//! listening"). Instead, this service long-polls the shared lavish server
//! for every session whose artifact lives inside a local workspace worktree
//! and forwards the human's feedback (annotations, the APPROVED prompt) to
//! the workspace's agent as a coding-agent follow-up, waking it only when
//! there is something to act on. The persistent long-poll also keeps
//! lavish's "agent is listening" indicator green.
//!
//! Lavish architecture (see the lavish-axi npm package): one shared local
//! server (default port 4387, `LAVISH_AXI_PORT`) with a session registry at
//! `$LAVISH_AXI_STATE_DIR/state.json` (default `~/.lavish-axi/`), keyed by
//! canonicalized artifact path. `GET /api/poll?file=…&timeoutMs=…` blocks
//! until feedback exists and consumes it atomically — so once this relay
//! takes a payload, delivering it to the agent is our responsibility.
//!
//! The registry outlives the server. The server is whichever `lavish-axi
//! <artifact>` invocation found none running, so it dies with the process
//! tree that started it — on a hosted runner, with the container a deploy
//! replaces — while `state.json` on the data volume still lists every
//! open session. This relay is the one thing that runs at boot and knows
//! the registry, so it is also what brings the server back: when the
//! server is unreachable and a live session sits inside a workspace
//! worktree, it starts `lavish-axi` on that artifact the way the reopen
//! route does. One start is enough; the server serves every registered
//! session, not only the one it was handed.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use db::{
    DBService,
    models::{
        coding_agent_turn::CodingAgentTurn,
        execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
        session::{CreateSession, Session},
        workspace::Workspace,
    },
};
use executors::actions::{
    ExecutorAction, ExecutorActionType, coding_agent_follow_up::CodingAgentFollowUpRequest,
    coding_agent_initial::CodingAgentInitialRequest,
};
use serde_json::Value;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::services::{
    approvals::decisions::{Decision, DecisionRequest, DecisionSource, Decisions},
    container::ContainerService,
};

/// How often the session registry is rescanned for new review sessions.
const SCAN_INTERVAL: Duration = Duration::from_secs(5);
/// Long-poll hold time; the server responds early the moment feedback lands.
const POLL_TIMEOUT_MS: u64 = 25_000;
/// How long a delivery waits for the agent to go idle before giving up.
const DELIVERY_IDLE_WAIT: Duration = Duration::from_secs(30 * 60);

/// How long an approval follow-up may run before the relay stops waiting
/// for confirmation and leaves the session open with a warning.
const APPROVAL_CONFIRM_WAIT: Duration = Duration::from_secs(10 * 60);

/// Minimum gap between two attempts to bring the shared server back. A
/// start that fails (lavish not installed, the artifact's checkout gone)
/// must not be retried every scan.
const REVIVE_BACKOFF: Duration = Duration::from_secs(60);
/// How long a health probe waits for the loopback server.
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_millis(800);

fn lavish_port() -> u16 {
    std::env::var("LAVISH_AXI_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4387)
}

fn lavish_state_file() -> Option<PathBuf> {
    let dir = std::env::var("LAVISH_AXI_STATE_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".lavish-axi"))
        })?;
    Some(dir.join("state.json"))
}

/// (session key, artifact file, status) triples from the registry.
async fn read_sessions() -> Vec<(String, String, String)> {
    let Some(state_file) = lavish_state_file() else {
        return Vec::new();
    };
    let Ok(raw) = tokio::fs::read_to_string(&state_file).await else {
        return Vec::new();
    };
    let Ok(state) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(sessions) = state.get("sessions").and_then(Value::as_object) else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter_map(|(key, session)| {
            let file = session.get("file")?.as_str()?.to_string();
            let status = session
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("open")
                .to_string();
            Some((key.clone(), file, status))
        })
        .collect()
}

/// Whether the shared lavish server answers on its loopback port.
pub async fn server_alive() -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(HEALTH_PROBE_TIMEOUT)
        .build()
    else {
        return true;
    };
    client
        .get(format!("http://127.0.0.1:{}/health", lavish_port()))
        .send()
        .await
        .is_ok()
}

/// Starts `lavish-axi <artifact> --no-open` from `cwd` — the headless open
/// the planning playbooks use. The CLI either registers the artifact with
/// the running shared server and exits, or finds none and becomes the
/// server, staying resident; so the child is not awaited. A fast non-zero
/// exit is a real failure and is logged, with the log file it wrote to.
///
/// Both the reopen route and the relay's revival go through here, so the
/// two ways a session comes back are the same command.
pub async fn open_session(artifact: &Path, cwd: &Path) -> anyhow::Result<()> {
    // Resolve through the login shell: the app's plain PATH usually lacks
    // the node bin dirs, which made a bare "npx" spawn die silently.
    let mut command = match utils::shell::resolve_executable_path("lavish-axi").await {
        Some(bin) => tokio::process::Command::new(bin),
        None => {
            let Some(npx) = utils::shell::resolve_executable_path("npx").await else {
                anyhow::bail!(
                    "Neither 'lavish-axi' nor 'npx' was found on PATH — install lavish-axi first"
                );
            };
            let mut command = tokio::process::Command::new(npx);
            command.arg("--yes").arg("lavish-axi");
            command
        }
    };

    // Output goes to a log file (never a pipe: the CLI stays resident when
    // it becomes the shared server, and an unread pipe could block it).
    let log_path = std::env::temp_dir().join("vk-lavish.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| anyhow::anyhow!("Failed to open lavish log: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| anyhow::anyhow!("Failed to open lavish log: {e}"))?;

    let mut child = command
        .arg(artifact)
        .arg("--no-open")
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(log_err))
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to start lavish-axi: {e}"))?;

    let artifact_display = artifact.display().to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => warn!(
                "lavish-axi for {artifact_display} exited with {status}; see {}",
                log_path.display()
            ),
            Ok(Some(_)) => {
                info!("lavish-axi for {artifact_display} registered with the shared server")
            }
            Ok(None) => info!("lavish-axi for {artifact_display} is running (server mode)"),
            Err(e) => warn!("lavish-axi for {artifact_display}: could not check child status: {e}"),
        }
    });

    Ok(())
}

/// The session to hand `lavish-axi` when the shared server has to be
/// brought back: the first live one whose artifact sits inside a workspace
/// worktree, with that worktree's checkout as the working directory.
/// Sessions outside our worktrees (personal lavish use) are not ours to
/// start, and an ended one has nothing to serve.
///
/// `roots` pairs each workspace with its canonical worktree roots (see
/// [`worktree_roots`]); `sessions` are registry triples as
/// `read_sessions` returns them.
fn revival_candidate<'a>(
    roots: &[(Uuid, Vec<PathBuf>)],
    sessions: &'a [(String, String, String)],
) -> Option<(Uuid, &'a str, PathBuf)> {
    sessions
        .iter()
        .filter(|(_, _, status)| status != "ended")
        .find_map(|(_, file, _)| {
            let file_path = Path::new(file);
            roots
                .iter()
                .flat_map(|(workspace_id, roots)| {
                    roots.iter().map(move |root| (*workspace_id, root))
                })
                .filter(|(_, root)| file_path.starts_with(root))
                .max_by_key(|(_, root)| root.components().count())
                .map(|(workspace_id, root)| (workspace_id, file.as_str(), root.clone()))
        })
}

/// Candidate canonical roots for a workspace worktree: the workspace dir
/// itself, plus each first-level entry with symlinks resolved. The second
/// part matters for pooled worktrees — `<workspace>/<repo>` is a SYMLINK to
/// `~/.treehouse/<pool>/<slot>/<repo>`, and lavish stores canonicalized
/// artifact paths (the pooled real path), so canonicalizing only the
/// workspace dir never prefix-matches a pooled session.
pub fn worktree_roots(container_ref: &str) -> Vec<PathBuf> {
    let mut roots =
        vec![std::fs::canonicalize(container_ref).unwrap_or_else(|_| PathBuf::from(container_ref))];
    if let Ok(entries) = std::fs::read_dir(container_ref) {
        for entry in entries.flatten() {
            if let Ok(real) = std::fs::canonicalize(entry.path())
                && real.is_dir()
                && !roots.contains(&real)
            {
                roots.push(real);
            }
        }
    }
    roots
}

/// Maps an artifact path to the local workspace whose worktree contains it.
/// Paths are canonicalized (macOS `/var` → `/private/var`, pool symlinks
/// resolved) to match lavish's canonicalized session paths; deepest match
/// wins.
fn match_workspace(workspaces: &[Workspace], file: &str) -> Option<Uuid> {
    let file_path = Path::new(file);
    workspaces
        .iter()
        .filter(|workspace| !workspace.archived)
        .filter_map(|workspace| {
            let container_ref = workspace.container_ref.as_deref()?;
            worktree_roots(container_ref)
                .into_iter()
                .filter(|root| file_path.starts_with(root))
                .map(|root| root.components().count())
                .max()
                .map(|depth| (workspace.id, depth))
        })
        .max_by_key(|(_, depth)| *depth)
        .map(|(workspace_id, _)| workspace_id)
}

/// Posts a status line into the reviewer's browser chat (best-effort).
/// These are the "what's happening" indicators the reviewer sees while
/// waiting: relay connected, feedback in flight, delivery failed.
async fn post_agent_reply(client: &reqwest::Client, reply_url: &str, text: &str) {
    let result = client
        .post(reply_url)
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await;
    if let Err(e) = result {
        debug!("Lavish relay: failed to post chat status: {e}");
    }
}

/// Renders a consumed feedback payload as the follow-up prompt. Includes the
/// exact command for the agent to reply into the reviewer's chat (a plain
/// HTTP POST — deliberately NOT `lavish-axi poll --agent-reply`, which would
/// consume feedback and race this relay's long-poll).
fn format_feedback(file: &str, reply_url: &str, body: &Value) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(prompts) = body.get("prompts").and_then(Value::as_array) {
        for prompt in prompts {
            if let Some(text) = prompt.get("prompt").and_then(Value::as_str)
                && !text.trim().is_empty()
            {
                lines.push(format!("- {}", text.trim()));
            }
        }
    }
    let feedback = if lines.is_empty() {
        "- (annotation-only feedback; open the artifact source for context)".to_string()
    } else {
        lines.join("\n")
    };
    format!(
        "Lavish review update for {file}:\n\n{feedback}\n\nIf the feedback above includes \"APPROVED\", record the approval on the linked prepcook issue via the MCP `update_issue` tool (spec_approved = true) — recording it advances the ticket to Executing automatically — and finish. Otherwise, address the feedback in the artifact (updating the planning verdict if it changes), then end your turn — further review feedback will arrive the same way.\n\nEither way, before ending your turn, post a one-line status reply into the reviewer's chat by running:\n\ncurl -s -X POST {reply_url} -H 'Content-Type: application/json' -d '{{\"text\":\"<your one-line reply>\"}}'\n\nDo NOT run `lavish-axi poll` — feedback delivery is handled for you."
    )
}

/// Service that watches lavish review sessions tied to local workspaces and
/// forwards consumed feedback as agent follow-ups (see module docs).
#[derive(Clone)]
pub struct LavishRelayService<C: ContainerService> {
    db: DBService,
    container: C,
    decisions: Decisions,
}

impl<C: ContainerService + Clone + Send + Sync + 'static> LavishRelayService<C> {
    pub async fn spawn(db: DBService, container: C, decisions: Decisions) -> JoinHandle<()> {
        let service = Self {
            db,
            container,
            decisions,
        };
        tokio::spawn(async move {
            service.start().await;
        })
    }

    async fn start(&self) {
        info!("Starting lavish relay service");
        let mut pollers: HashMap<String, JoinHandle<()>> = HashMap::new();
        let mut last_revive: Option<tokio::time::Instant> = None;

        loop {
            pollers.retain(|_, handle| !handle.is_finished());

            let sessions = read_sessions().await;
            if !sessions.is_empty() {
                let workspaces = Workspace::fetch_all(&self.db.pool)
                    .await
                    .unwrap_or_default();

                // A registry with live sessions and no server behind it is
                // the state a deploy leaves behind (and a crash, and a
                // reviewer's Ctrl-C on a desktop). Start it again before
                // the pollers go looking for it.
                let due = last_revive.is_none_or(|at| at.elapsed() >= REVIVE_BACKOFF);
                if due && !server_alive().await {
                    last_revive = Some(tokio::time::Instant::now());
                    self.revive_server(&workspaces, &sessions).await;
                }

                for (key, file, status) in sessions {
                    if status == "ended" || pollers.contains_key(&key) {
                        continue;
                    }
                    // Sessions outside VK worktrees (personal lavish use)
                    // are never touched — consuming their feedback would
                    // steal it from whatever agent is handling them.
                    let Some(workspace_id) = match_workspace(&workspaces, &file) else {
                        continue;
                    };
                    info!("Lavish relay: watching {file} for workspace {workspace_id}");
                    let service = self.clone();
                    let session_key = key.clone();
                    pollers.insert(
                        key,
                        tokio::spawn(async move {
                            service.poll_session(workspace_id, session_key, file).await;
                        }),
                    );
                }
            }

            tokio::time::sleep(SCAN_INTERVAL).await;
        }
    }

    /// Brings the shared server back for the first live session inside a
    /// workspace worktree whose artifact still exists. Best-effort: a
    /// failure is logged and the next scan after `REVIVE_BACKOFF` tries
    /// again.
    async fn revive_server(&self, workspaces: &[Workspace], sessions: &[(String, String, String)]) {
        let roots: Vec<(Uuid, Vec<PathBuf>)> = workspaces
            .iter()
            .filter(|workspace| !workspace.archived)
            .filter_map(|workspace| {
                let container_ref = workspace.container_ref.as_deref()?;
                Some((workspace.id, worktree_roots(container_ref)))
            })
            .collect();
        let Some((workspace_id, file, checkout)) = revival_candidate(&roots, sessions) else {
            debug!("Lavish relay: server down, but no live session inside a workspace worktree");
            return;
        };
        let artifact = Path::new(file);
        if !tokio::fs::metadata(artifact)
            .await
            .map(|meta| meta.is_file())
            .unwrap_or(false)
        {
            warn!("Lavish relay: server down and {file} is registered but no longer on disk");
            return;
        }
        info!(
            "Lavish relay: shared server is down with {file} registered; starting it for workspace {workspace_id}"
        );
        if let Err(e) = open_session(artifact, &checkout).await {
            warn!("Lavish relay: could not start the shared server: {e}");
        }
    }

    /// Long-polls one session until it ends, forwarding each feedback batch.
    async fn poll_session(&self, workspace_id: Uuid, key: String, file: String) {
        let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_millis(POLL_TIMEOUT_MS + 10_000))
            .build()
        else {
            return;
        };
        let base = format!("http://127.0.0.1:{}", lavish_port());
        let poll_url = format!("{base}/api/poll");
        let reply_url = format!("{base}/api/{key}/agent-reply");
        let timeout = POLL_TIMEOUT_MS.to_string();
        let mut consecutive_errors: u32 = 0;

        // Chat indicator: tell the reviewer the pipeline is live.
        post_agent_reply(
            &client,
            &reply_url,
            "🔗 Connected to the prepcook agent. Feedback and Approve are delivered automatically — I'll acknowledge here when the agent picks them up.",
        )
        .await;

        loop {
            let response = client
                .get(&poll_url)
                .query(&[("file", file.as_str()), ("timeoutMs", timeout.as_str())])
                .send()
                .await;

            let body = match response {
                Ok(response) if response.status().is_success() => {
                    match response.json::<Value>().await {
                        Ok(body) => {
                            consecutive_errors = 0;
                            body
                        }
                        Err(_) => continue,
                    }
                }
                _ => {
                    consecutive_errors += 1;
                    if consecutive_errors > 5 {
                        // Server gone (or session unreachable); the scan
                        // loop re-spawns us if the session is still live.
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };

            match body.get("status").and_then(Value::as_str) {
                Some("waiting") => continue,
                Some("feedback") => {
                    // Approval is terminal for the review: after handing it
                    // to the agent, the session is ended so the reviewer
                    // lands back on their prepcook tab instead of an
                    // idle review page.
                    let approved =
                        body.get("prompts")
                            .and_then(Value::as_array)
                            .is_some_and(|prompts| {
                                prompts.iter().any(|prompt| {
                                    prompt.get("prompt").and_then(Value::as_str).is_some_and(
                                        |text| text.trim().eq_ignore_ascii_case("APPROVED"),
                                    )
                                })
                            });

                    // Chat indicator: feedback is in flight to the agent.
                    post_agent_reply(
                        &client,
                        &reply_url,
                        if approved {
                            "🕐 Approval received — handing it to the agent and waiting for it to land on the ticket…"
                        } else {
                            "📨 Feedback received — waking the agent. It will reply here when the update is ready."
                        },
                    )
                    .await;

                    // The approval is a decision like any other and is
                    // recorded as one; the agent still receives the raw
                    // feedback below and records the spec approval itself.
                    if approved {
                        self.record_approval(workspace_id).await;
                    }

                    let prompt = format_feedback(&file, &reply_url, &body);
                    let delivered = self.deliver(workspace_id, prompt).await;
                    if delivered.is_none() {
                        post_agent_reply(
                            &client,
                            &reply_url,
                            "⚠️ Could not hand this feedback to the agent (it stayed busy or is unavailable). Please repost your feedback, or check the workspace in prepcook.",
                        )
                        .await;
                    }

                    if let Some(process_id) = delivered
                        && approved
                    {
                        // The approval "lands" when the follow-up turn we
                        // just started completes — that turn is where the
                        // agent records spec_approved (which auto-advances
                        // the ticket). Track that exact process, so an
                        // Executing agent starting right after doesn't
                        // confuse the wait.
                        if self.wait_for_process(process_id).await {
                            post_agent_reply(
                                &client,
                                &reply_url,
                                "✅ Approval landed — the ticket is moving to Execute. This review session is now closed; you can return to prepcook.",
                            )
                            .await;
                            let end_url = format!("{base}/api/{key}/end");
                            if let Err(e) = client.post(&end_url).send().await {
                                debug!("Lavish relay: failed to end session after approval: {e}");
                            }
                            break;
                        }
                        post_agent_reply(
                            &client,
                            &reply_url,
                            "⚠️ The agent hasn't finished recording the approval yet. Leaving this session open — check the ticket in prepcook; if it doesn't advance, send APPROVED again.",
                        )
                        .await;
                    }

                    if body
                        .get("session_ended")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        break;
                    }
                }
                Some("ended") | Some("missing") => break,
                _ => continue,
            }
        }
        debug!("Lavish relay: stopped watching {file}");
    }

    /// Records a Lavish approval on the card the workspace was launched for.
    /// Best effort: the agent's own recording is what advances the card, so a
    /// failure here costs the audit record, not the approval.
    async fn record_approval(&self, workspace_id: Uuid) {
        let Some(issue_id) = self.decisions.issue_for_workspace(workspace_id).await else {
            debug!("Lavish relay: workspace {workspace_id} has no card to record the approval on");
            return;
        };
        let request = DecisionRequest {
            issue_id,
            decision: Decision::Approve,
            comment: String::new(),
            by: "Lavish reviewer".to_string(),
            source: DecisionSource::Lavish,
        };
        if let Err(e) = self.decisions.apply_decision(request).await {
            warn!("Lavish relay: recording the approval on {issue_id} failed: {e}");
        }
    }

    /// Waits (bounded) for an execution process to leave Running. True means
    /// the turn finished (any terminal status — the ticket's own state is
    /// the source of truth for whether the approval recorded successfully).
    async fn wait_for_process(&self, process_id: Uuid) -> bool {
        let deadline = tokio::time::Instant::now() + APPROVAL_CONFIRM_WAIT;
        loop {
            match ExecutionProcess::find_by_id(&self.db.pool, process_id).await {
                Ok(Some(process)) if process.status == ExecutionProcessStatus::Running => {
                    if tokio::time::Instant::now() >= deadline {
                        return false;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Ok(Some(_)) => return true,
                Ok(None) => return false,
                Err(e) => {
                    warn!("Lavish relay: cannot check approval turn {process_id}: {e}");
                    return false;
                }
            }
        }
    }

    /// Starts a coding-agent follow-up carrying the feedback. The payload is
    /// already consumed from lavish, so this waits (bounded) for the agent
    /// to go idle rather than dropping it. Returns the started execution
    /// process id, so the caller can flag failures in the review chat and
    /// track the turn to completion (approval confirmation).
    async fn deliver(&self, workspace_id: Uuid, prompt: String) -> Option<Uuid> {
        let pool = &self.db.pool;

        let deadline = tokio::time::Instant::now() + DELIVERY_IDLE_WAIT;
        loop {
            match ExecutionProcess::has_running_non_dev_server_processes_for_workspace(
                pool,
                workspace_id,
            )
            .await
            {
                Ok(false) => break,
                Ok(true) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Ok(true) => {
                    warn!(
                        "Lavish relay: workspace {workspace_id} never went idle; dropping feedback"
                    );
                    return None;
                }
                Err(e) => {
                    warn!("Lavish relay: cannot check workspace {workspace_id} state: {e}");
                    return None;
                }
            }
        }

        let workspace = match Workspace::find_by_id(pool, workspace_id).await {
            Ok(Some(workspace)) if !workspace.archived => workspace,
            _ => return None,
        };

        let session = match Session::find_latest_by_workspace_id(pool, workspace.id).await {
            Ok(Some(session)) => session,
            Ok(None) => {
                match Session::create(
                    pool,
                    &CreateSession {
                        executor: None,
                        name: None,
                    },
                    Uuid::new_v4(),
                    workspace.id,
                )
                .await
                {
                    Ok(session) => session,
                    Err(e) => {
                        warn!("Lavish relay: failed to create session: {e}");
                        return None;
                    }
                }
            }
            Err(e) => {
                warn!("Lavish relay: failed to load session: {e}");
                return None;
            }
        };

        let executor_profile_id =
            match ExecutionProcess::latest_executor_profile_for_session(pool, session.id).await {
                Ok(Some(profile)) => profile,
                _ => {
                    warn!(
                        "Lavish relay: no executor profile for session {}; dropping feedback",
                        session.id
                    );
                    return None;
                }
            };

        let latest_session_info = CodingAgentTurn::find_latest_session_info(pool, session.id)
            .await
            .ok()
            .flatten();
        let working_dir = session
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        let action_type = if let Some(info) = latest_session_info {
            ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
                prompt,
                session_id: info.session_id,
                reset_to_message_id: None,
                executor_config: executors::profile::ExecutorConfig::from(
                    executor_profile_id.clone(),
                ),
                working_dir: working_dir.clone(),
            })
        } else {
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt,
                executor_config: executors::profile::ExecutorConfig::from(executor_profile_id),
                working_dir,
            })
        };
        let action = ExecutorAction::new(action_type, None);

        // Same guard as the orchestrator's follow-ups: the janitor may have
        // removed this worktree while the plan sat in review.
        if let Err(e) = self.container.ensure_container_exists(&workspace).await {
            warn!(
                "Lavish relay: could not restore the worktree for workspace {}: {e}",
                workspace.id
            );
            return None;
        }

        match self
            .container
            .start_execution(
                &workspace,
                &session,
                &action,
                &ExecutionProcessRunReason::CodingAgent,
            )
            .await
        {
            Ok(process) => {
                info!(
                    "Lavish relay: delivered review feedback to workspace {}",
                    workspace.id
                );
                Some(process.id)
            }
            Err(e) => {
                warn!(
                    "Lavish relay: failed to start follow-up on workspace {}: {e}",
                    workspace.id
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(file: &str, status: &str) -> (String, String, String) {
        (format!("key-{file}"), file.to_string(), status.to_string())
    }

    #[test]
    fn revival_takes_the_first_live_session_in_a_worktree() {
        let ours = Uuid::new_v4();
        let roots = vec![(
            ours,
            vec![
                PathBuf::from("/data/workspaces/ws-1"),
                PathBuf::from("/data/pool/slot-3/app"),
            ],
        )];
        let sessions = vec![
            // Ended: nothing to serve.
            session("/data/pool/slot-3/app/.plan/spec.html", "ended"),
            // Personal lavish use outside any worktree: not ours to start.
            session("/home/nick/notes/plan.html", "open"),
            // Ours, live, in the pooled checkout the workspace links to.
            session("/data/pool/slot-3/app/.plan/rca.html", "open"),
            session("/data/workspaces/ws-1/app/.plan/other.html", "open"),
        ];
        let (workspace_id, file, checkout) =
            revival_candidate(&roots, &sessions).expect("a live session in a worktree");
        assert_eq!(workspace_id, ours);
        assert_eq!(file, "/data/pool/slot-3/app/.plan/rca.html");
        // The deepest root containing the artifact is the checkout to run from.
        assert_eq!(checkout, PathBuf::from("/data/pool/slot-3/app"));
    }

    #[test]
    fn nothing_to_revive_without_a_live_session_of_ours() {
        let roots = vec![(Uuid::new_v4(), vec![PathBuf::from("/data/workspaces/ws-1")])];
        let sessions = vec![
            session("/data/workspaces/ws-1/app/.plan/spec.html", "ended"),
            session("/elsewhere/plan.html", "open"),
        ];
        assert!(revival_candidate(&roots, &sessions).is_none());
        assert!(revival_candidate(&roots, &[]).is_none());
        assert!(revival_candidate(&[], &sessions).is_none());
    }
}
