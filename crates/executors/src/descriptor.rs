//! What each coding agent says about itself: what it needs installed, how it
//! signs in, how its sessions get MCP servers, the variants it ships with,
//! and the probe that checks it answers.
//!
//! Every [`BaseCodingAgent`] has one, built by its executor's `descriptor()`.
//! Settings renders a card per agent from them, and the configuration
//! reference carries a table generated from the same data.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use strum::VariantNames;
use ts_rs::TS;

#[cfg(feature = "qa-mode")]
use crate::executors::qa_mock::QaMockExecutor;
use crate::{
    executors::{
        AvailabilityInfo, BaseCodingAgent, CodingAgent, StandardCodingAgentExecutor, amp::Amp,
        claude::ClaudeCode, codex::Codex, copilot::Copilot, cursor::CursorAgent, droid::Droid,
        gemini::Gemini, opencode::Opencode, qwen::QwenCode,
    },
    profile::{ExecutorConfigs, ExecutorProfileId},
};

/// How an agent's sessions receive the MCP servers Prep Cook hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum McpWiring {
    /// Each session is pointed at the session MCP config on its own command
    /// line (Claude Code's `--mcp-config`, Codex's thread config), on top of
    /// the user's own servers and without editing their config.
    PerSession,
    /// The agent reads MCP servers only from its own config file, which
    /// Settings → MCP Servers edits.
    AgentConfig,
    /// The agent takes no MCP servers.
    Unsupported,
}

/// What a coding agent says about itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentDescriptor {
    pub executor: BaseCodingAgent,
    pub display_name: String,
    /// One sentence.
    pub summary: String,
    /// The program a session runs, looked up on PATH. Empty for an agent
    /// that runs inside Prep Cook.
    pub binary: String,
    /// The command line a session starts from, before its own arguments.
    /// The version probe runs it with `--version`.
    pub command: String,
    /// How to sign it in, e.g. "Run `claude` once and sign in".
    pub sign_in: String,
    pub supports_mcp: bool,
    pub mcp: McpWiring,
    /// The variants `default_profiles.json` ships for it, `DEFAULT` first.
    pub variants: Vec<String>,
    /// The cheaper variant a stage that carries no design judgment defaults
    /// to when the configured profile names no variant. `None` when the
    /// agent ships no such variant.
    pub economy_variant: Option<String>,
    pub docs_url: String,
}

impl AgentDescriptor {
    /// Builds a descriptor whose binary, MCP support and shipped variants
    /// are derived from `command`, `mcp` and `default_profiles.json`.
    pub fn new(
        executor: BaseCodingAgent,
        display_name: &str,
        summary: &str,
        command: &str,
        sign_in: &str,
        mcp: McpWiring,
        docs_url: &str,
    ) -> Self {
        Self {
            executor,
            display_name: display_name.to_string(),
            summary: summary.to_string(),
            binary: command.split_whitespace().next().unwrap_or("").to_string(),
            command: command.to_string(),
            sign_in: sign_in.to_string(),
            supports_mcp: mcp != McpWiring::Unsupported,
            mcp,
            variants: shipped_variants(executor),
            economy_variant: None,
            docs_url: docs_url.to_string(),
        }
    }

    /// Names the variant the cost-sensitive stages default to. It must be one
    /// of the shipped variants.
    pub fn with_economy_variant(mut self, variant: &str) -> Self {
        self.economy_variant = Some(variant.to_string());
        self
    }
}

/// The variant keys `default_profiles.json` ships for an agent, `DEFAULT`
/// first and the rest in name order.
fn shipped_variants(executor: BaseCodingAgent) -> Vec<String> {
    let defaults = ExecutorConfigs::from_defaults();
    let mut variants: Vec<String> = defaults
        .executors
        .get(&executor)
        .map(|profile| profile.configurations.keys().cloned().collect())
        .unwrap_or_default();
    variants.sort_by(|a, b| (a != "DEFAULT", a).cmp(&(b != "DEFAULT", b)));
    variants
}

impl BaseCodingAgent {
    /// This agent's descriptor. Every variant of [`CodingAgent`] has one;
    /// the match is exhaustive so a new agent cannot be added without it.
    pub fn descriptor(self) -> AgentDescriptor {
        match self {
            Self::ClaudeCode => ClaudeCode::descriptor(),
            Self::Amp => Amp::descriptor(),
            Self::Gemini => Gemini::descriptor(),
            Self::Codex => Codex::descriptor(),
            Self::Opencode => Opencode::descriptor(),
            Self::CursorAgent => CursorAgent::descriptor(),
            Self::QwenCode => QwenCode::descriptor(),
            Self::Copilot => Copilot::descriptor(),
            Self::Droid => Droid::descriptor(),
            #[cfg(feature = "qa-mode")]
            Self::QaMock => QaMockExecutor::descriptor(),
        }
    }

    /// Every agent, in the order the [`CodingAgent`] enum declares them.
    pub fn all() -> Vec<BaseCodingAgent> {
        CodingAgent::VARIANTS
            .iter()
            .filter_map(|name| BaseCodingAgent::from_str(name).ok())
            .collect()
    }
}

/// Every agent's descriptor, in declaration order.
pub fn descriptors() -> Vec<AgentDescriptor> {
    BaseCodingAgent::all()
        .into_iter()
        .map(BaseCodingAgent::descriptor)
        .collect()
}

/// `profile` with the agent's economy variant pinned on when it names no
/// variant. An explicit variant, or an agent without an economy variant,
/// passes through: configuration always wins over this default.
pub fn economy_variant_of(profile: &ExecutorProfileId) -> ExecutorProfileId {
    match (
        &profile.variant,
        profile.executor.descriptor().economy_variant,
    ) {
        (None, Some(variant)) => ExecutorProfileId::with_variant(profile.executor, variant),
        _ => profile.clone(),
    }
}

/// How long the version probe may take. One short command: an agent that
/// cannot print its version in this time is not ready to run a session.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// What the probe found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub ok: bool,
    pub message: String,
}

/// Runs the agent's version probe on this machine's PATH and reports whether
/// a sign-in was found. Nothing runs until this is called: there is no
/// probe at startup.
pub async fn probe(executor: BaseCodingAgent) -> ProbeOutcome {
    let availability = ExecutorConfigs::get_cached()
        .get_coding_agent(&ExecutorProfileId::new(executor))
        .map(|agent| agent.get_availability_info());
    probe_with(
        &executor.descriptor(),
        std::env::var_os("PATH"),
        PROBE_TIMEOUT,
        availability,
    )
    .await
}

/// [`probe`] against an explicit PATH and timeout.
pub async fn probe_with(
    descriptor: &AgentDescriptor,
    path: Option<OsString>,
    timeout: Duration,
    availability: Option<AvailabilityInfo>,
) -> ProbeOutcome {
    let Some(mut argv) = shlex::split(&descriptor.command) else {
        return ProbeOutcome {
            ok: false,
            message: format!("cannot parse the command `{}`", descriptor.command),
        };
    };
    if argv.is_empty() {
        return ProbeOutcome {
            ok: true,
            message: format!(
                "{} runs inside Prep Cook; there is nothing to install.",
                descriptor.display_name
            ),
        };
    }
    let program = argv.remove(0);
    let Some(resolved) = find_on_path(&program, path.as_deref()) else {
        return ProbeOutcome {
            ok: false,
            message: format!("`{program}` is not on PATH."),
        };
    };

    let mut command = tokio::process::Command::new(&resolved);
    command
        .args(&argv)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = &path {
        command.env("PATH", path);
    }
    let label = shown_command(&program, &argv);
    let output = match tokio::time::timeout(timeout, command.output()).await {
        Err(_) => {
            return ProbeOutcome {
                ok: false,
                message: format!("`{label}` did not answer within {} s.", timeout.as_secs()),
            };
        }
        Ok(Err(error)) => {
            return ProbeOutcome {
                ok: false,
                message: format!("`{label}` could not start: {error}"),
            };
        }
        Ok(Ok(output)) => output,
    };

    let stdout = first_line(&output.stdout);
    let stderr = first_line(&output.stderr);
    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "a signal".to_string());
        let detail = stderr.or(stdout).unwrap_or_default();
        return ProbeOutcome {
            ok: false,
            message: format!("`{label}` exited with {code}: {detail}")
                .trim_end_matches([' ', ':'])
                .to_string(),
        };
    }
    let version = stdout
        .or(stderr)
        .unwrap_or_else(|| "no version printed".to_string());
    let sign_in = match availability {
        Some(AvailabilityInfo::LoginDetected { .. }) => "sign-in found".to_string(),
        _ => format!("no sign-in found ({})", descriptor.sign_in),
    };
    ProbeOutcome {
        ok: true,
        message: format!("{version}; {sign_in}"),
    }
}

fn shown_command(program: &str, args: &[String]) -> String {
    let mut parts = vec![program.to_string()];
    parts.extend(args.iter().cloned());
    parts.push("--version".to_string());
    parts.join(" ")
}

/// The first non-blank line of a process's output, capped so a stack trace
/// cannot fill the Settings card.
fn first_line(bytes: &[u8]) -> Option<String> {
    const MAX: usize = 200;
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(match line.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    })
}

/// Looks `program` up on `path` (the process PATH when `None`). A program
/// given as a path is used as it is.
fn find_on_path(program: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let direct = Path::new(program);
    if direct.components().count() > 1 {
        return is_executable(direct).then(|| direct.to_path_buf());
    }
    let path = match path {
        Some(path) => path.to_os_string(),
        None => std::env::var_os("PATH")?,
    };
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path).find_map(|dir| {
        extensions.iter().find_map(|extension| {
            let candidate = dir.join(format!("{program}{extension}"));
            is_executable(&candidate).then_some(candidate)
        })
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_has_a_descriptor_that_matches_its_profiles() {
        let all = BaseCodingAgent::all();
        assert!(all.contains(&BaseCodingAgent::ClaudeCode));
        assert!(all.contains(&BaseCodingAgent::Droid));
        for executor in all {
            let descriptor = executor.descriptor();
            assert_eq!(descriptor.executor, executor);
            assert!(!descriptor.display_name.is_empty(), "{executor}");
            assert!(!descriptor.summary.is_empty(), "{executor}");
            assert!(!descriptor.sign_in.is_empty(), "{executor}");
            assert!(descriptor.docs_url.starts_with("https://"), "{executor}");
            assert_eq!(
                descriptor.binary,
                descriptor.command.split_whitespace().next().unwrap_or(""),
                "{executor}"
            );
            assert_eq!(
                descriptor.supports_mcp,
                descriptor.mcp != McpWiring::Unsupported
            );
            if let Some(economy) = &descriptor.economy_variant {
                assert!(descriptor.variants.contains(economy), "{executor}");
            }
            // The QA mock ships no profile and runs nothing on PATH.
            if executor.to_string() == "QA_MOCK" {
                continue;
            }
            assert_eq!(
                descriptor.variants.first().map(String::as_str),
                Some("DEFAULT"),
                "{executor}"
            );
            assert!(!descriptor.binary.is_empty(), "{executor}");
        }
    }

    #[test]
    fn claude_and_codex_take_mcp_per_session_and_the_rest_from_their_config() {
        for descriptor in descriptors() {
            let expected = match descriptor.executor {
                BaseCodingAgent::ClaudeCode | BaseCodingAgent::Codex => McpWiring::PerSession,
                #[cfg(feature = "qa-mode")]
                BaseCodingAgent::QaMock => McpWiring::Unsupported,
                _ => McpWiring::AgentConfig,
            };
            assert_eq!(descriptor.mcp, expected, "{}", descriptor.executor);
        }
    }

    #[test]
    fn claude_names_its_sign_in_and_economy_variant() {
        let claude = BaseCodingAgent::ClaudeCode.descriptor();
        assert_eq!(claude.binary, "claude");
        assert!(claude.sign_in.contains("`claude`"));
        assert_eq!(claude.variants, vec!["DEFAULT", "SONNET"]);
        assert_eq!(claude.economy_variant.as_deref(), Some("SONNET"));
        let codex = BaseCodingAgent::Codex.descriptor();
        assert!(codex.sign_in.contains("CODEX_API_KEY"));
        assert_eq!(codex.economy_variant, None);
    }

    #[test]
    fn the_economy_variant_fills_only_an_unset_variant() {
        let bare = ExecutorProfileId::new(BaseCodingAgent::ClaudeCode);
        assert_eq!(
            economy_variant_of(&bare),
            ExecutorProfileId::with_variant(BaseCodingAgent::ClaudeCode, "SONNET".to_string())
        );
        let chosen =
            ExecutorProfileId::with_variant(BaseCodingAgent::ClaudeCode, "DEFAULT".to_string());
        assert_eq!(economy_variant_of(&chosen), chosen);
        let codex = ExecutorProfileId::new(BaseCodingAgent::Codex);
        assert_eq!(economy_variant_of(&codex), codex);
    }

    #[cfg(unix)]
    mod probe {
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        /// A directory holding one executable script named `name`.
        fn fake_bin(name: &str, script: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("prepcook-probe-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            dir
        }

        fn descriptor(command: &str) -> AgentDescriptor {
            AgentDescriptor {
                command: command.to_string(),
                binary: command.split_whitespace().next().unwrap().to_string(),
                ..BaseCodingAgent::ClaudeCode.descriptor()
            }
        }

        fn path_of(dir: &Path) -> Option<OsString> {
            // The fake binary's directory, then the system's for `sh` itself.
            let system = std::env::var_os("PATH").unwrap_or_default();
            let mut dirs = vec![dir.to_path_buf()];
            dirs.extend(std::env::split_paths(&system));
            Some(std::env::join_paths(dirs).unwrap())
        }

        #[tokio::test]
        async fn a_fake_binary_on_path_answers_with_its_version_and_sign_in() {
            let dir = fake_bin(
                "fakeagent",
                r#"[ "$1" = "--version" ] && echo "1.2.3 (Fake Agent)" && exit 0; exit 9"#,
            );
            let outcome = probe_with(
                &descriptor("fakeagent"),
                path_of(&dir),
                PROBE_TIMEOUT,
                Some(AvailabilityInfo::LoginDetected {
                    last_auth_timestamp: 0,
                }),
            )
            .await;
            assert!(outcome.ok, "{}", outcome.message);
            assert_eq!(outcome.message, "1.2.3 (Fake Agent); sign-in found");

            let unsigned = probe_with(
                &descriptor("fakeagent"),
                path_of(&dir),
                PROBE_TIMEOUT,
                Some(AvailabilityInfo::NotFound),
            )
            .await;
            assert!(unsigned.ok);
            assert!(
                unsigned
                    .message
                    .starts_with("1.2.3 (Fake Agent); no sign-in found (")
            );
        }

        #[tokio::test]
        async fn arguments_in_the_command_come_before_the_version_flag() {
            let dir = fake_bin(
                "fakenpx",
                r#"[ "$1 $2 $3" = "-y @acme/agent@1.0.0 --version" ] && echo "agent 1.0.0" && exit 0; echo "got: $*" >&2; exit 3"#,
            );
            let outcome = probe_with(
                &descriptor("fakenpx -y @acme/agent@1.0.0"),
                path_of(&dir),
                PROBE_TIMEOUT,
                None,
            )
            .await;
            assert!(outcome.ok, "{}", outcome.message);
            assert!(outcome.message.starts_with("agent 1.0.0; "));
        }

        #[tokio::test]
        async fn a_failing_binary_reports_its_exit_code_and_stderr() {
            let dir = fake_bin("brokenagent", r#"echo "not signed in" >&2; exit 4"#);
            let outcome = probe_with(
                &descriptor("brokenagent"),
                path_of(&dir),
                PROBE_TIMEOUT,
                None,
            )
            .await;
            assert!(!outcome.ok);
            assert_eq!(
                outcome.message,
                "`brokenagent --version` exited with 4: not signed in"
            );
        }

        #[tokio::test]
        async fn a_binary_that_hangs_is_cut_off_at_the_timeout() {
            let dir = fake_bin("slowagent", "sleep 30");
            let started = std::time::Instant::now();
            let outcome = probe_with(
                &descriptor("slowagent"),
                path_of(&dir),
                Duration::from_millis(300),
                None,
            )
            .await;
            assert!(!outcome.ok);
            assert!(
                outcome.message.contains("did not answer"),
                "{}",
                outcome.message
            );
            assert!(started.elapsed() < Duration::from_secs(10));
        }

        #[tokio::test]
        async fn a_missing_binary_is_named() {
            let dir = fake_bin("someotheragent", "exit 0");
            let outcome = probe_with(
                &descriptor("prepcook-no-such-agent"),
                Some(dir.into_os_string()),
                PROBE_TIMEOUT,
                None,
            )
            .await;
            assert!(!outcome.ok);
            assert_eq!(outcome.message, "`prepcook-no-such-agent` is not on PATH.");
        }
    }
}
