//! `.prepcook/scripts.toml`: how a target repository installs, builds, starts,
//! seeds and tests itself. The schema is documented in `docs/scripts.md`;
//! unknown keys are an error so a typo is reported rather than ignored.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::secrets;

/// Where the manifest lives, relative to the repository root.
pub const MANIFEST_PATH: &str = ".prepcook/scripts.toml";

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub app: AppScripts,
    #[serde(default)]
    pub workspace: WorkspaceScripts,
    #[serde(default)]
    pub secrets: SecretRequirements,
    /// Product profile: what the playbooks say about the product (see
    /// `docs/playbooks.md`). The runner itself does not read it.
    #[serde(default)]
    pub profile: Option<ProductProfile>,
}

/// `[app]`: the commands that stand the app up in a worktree. Every command
/// runs through the user's shell with the worktree root as its directory.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppScripts {
    /// Installs dependencies in a fresh worktree; exit 0 means ready.
    #[serde(default)]
    pub setup: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
    /// Starts the app in the foreground, listening on `$PREPCOOK_PORT`.
    pub start: String,
    #[serde(default)]
    pub ready: Option<ReadyCheck>,
    /// Loads test data; writes `KEY=VALUE` lines to `$PREPCOOK_ENV_OUT`.
    #[serde(default)]
    pub seed: Option<String>,
    /// The exit code is the verdict.
    #[serde(default)]
    pub test: Option<String>,
    /// Runs before the start process group is killed.
    #[serde(default)]
    pub stop: Option<String>,
    #[serde(default)]
    pub timeouts: StepTimeouts,
    /// How the app is deployed somewhere other than this machine, for boards
    /// whose reproduction target is `remote`.
    #[serde(default)]
    pub remote: Option<RemoteScripts>,
}

/// `[app.remote]`: commands that deploy the worktree's app to an environment
/// that is reached over the network (a preview environment, a dev tenant, a
/// per-branch staging slot) rather than served from this machine. They run
/// on this machine, in the worktree, like every other command; `start`
/// returns once the deploy is done instead of serving in the foreground.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoteScripts {
    /// Deploys the app and reports its base URL: a `PREPCOOK_APP_URL=<url>`
    /// line written to `$PREPCOOK_ENV_OUT`, or failing that the last line of
    /// stdout that starts with `http://` or `https://`.
    pub start: String,
    /// Polled against the reported URL.
    #[serde(default)]
    pub ready: Option<ReadyCheck>,
    /// Loads test data into the deployed app; writes `KEY=VALUE` lines to
    /// `$PREPCOOK_ENV_OUT`. `[app] seed` is not used for a remote app: a
    /// local seed may write to local storage the deployed app never reads.
    #[serde(default)]
    pub seed: Option<String>,
    /// Tests the deployed app; the exit code is the verdict.
    #[serde(default)]
    pub test: Option<String>,
    /// Destroys what `start` deployed.
    #[serde(default)]
    pub stop: Option<String>,
    /// Redeploys in place. Unset, a restart is `stop` then `start`.
    #[serde(default)]
    pub restart: Option<String>,
    /// Remote apps up at once, across every worktree. Each one is a deployed
    /// environment, so this is usually lower than the local limit.
    #[serde(default = "default_remote_max_parallel")]
    #[ts(type = "number")]
    pub max_parallel: u32,
    #[serde(default)]
    pub timeouts: RemoteTimeouts,
}

fn default_remote_max_parallel() -> u32 {
    1
}

/// `[app.remote.timeouts]`, in seconds. `setup` and `build` come from
/// `[app.timeouts]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct RemoteTimeouts {
    /// Time allowed for `start` (or `restart`) to deploy and exit.
    #[ts(type = "number")]
    pub start: u64,
    #[ts(type = "number")]
    pub seed: u64,
    #[ts(type = "number")]
    pub test: u64,
    #[ts(type = "number")]
    pub stop: u64,
}

impl Default for RemoteTimeouts {
    fn default() -> Self {
        Self {
            start: 1800,
            seed: 600,
            test: 1800,
            stop: 900,
        }
    }
}

/// `ready = { url = "/health" }` or `ready = { command = "..." }`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadyCheck {
    /// Polled until it answers 2xx; relative to `$PREPCOOK_APP_URL`.
    #[serde(default)]
    pub url: Option<String>,
    /// Run until it exits 0.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default = "default_ready_timeout")]
    #[ts(type = "number")]
    pub timeout_secs: u64,
}

fn default_ready_timeout() -> u64 {
    300
}

/// `[app.timeouts]`, in seconds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct StepTimeouts {
    #[ts(type = "number")]
    pub setup: u64,
    #[ts(type = "number")]
    pub build: u64,
    /// Time allowed for `start` to begin listening on `$PREPCOOK_PORT`.
    #[ts(type = "number")]
    pub start: u64,
    #[ts(type = "number")]
    pub seed: u64,
    #[ts(type = "number")]
    pub test: u64,
    #[ts(type = "number")]
    pub stop: u64,
}

impl Default for StepTimeouts {
    fn default() -> Self {
        Self {
            setup: 900,
            build: 1200,
            start: 120,
            seed: 600,
            test: 1800,
            stop: 120,
        }
    }
}

/// `[workspace]`: hooks run on workspace lifecycle events.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceScripts {
    /// Defaults to `app.setup`.
    #[serde(default)]
    pub setup: Option<String>,
    #[serde(default)]
    pub cleanup: Option<String>,
    #[serde(default)]
    pub archive: Option<String>,
}

/// `[secrets]`: names the scripts need, never values.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SecretRequirements {
    #[serde(default)]
    pub requires: Vec<String>,
}

/// `[profile]`: what the product is, for prompts that need to know.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductProfile {
    #[serde(default)]
    pub name: Option<String>,
    /// One paragraph on what the app is and who uses it.
    #[serde(default)]
    pub summary: Option<String>,
    /// The parts of the product a bug report or a review is likely to name.
    #[serde(default)]
    pub areas: Vec<String>,
    /// Errors that mean the seed is broken rather than the app; an agent that
    /// meets one reports it and stops.
    #[serde(default)]
    pub error_patterns: Vec<String>,
    /// Where an agent finds evidence beyond its own reproduction (log files,
    /// an error tracker it has a CLI for), in a sentence or two. Unset means
    /// the reproduction is the only evidence.
    #[serde(default)]
    pub evidence_tools: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ManifestError {
    #[error("no {MANIFEST_PATH} in {}; see docs/scripts.md", repo.display())]
    Missing { repo: PathBuf },
    #[error("{}: {message}", path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("could not read {}: {message}", path.display())]
    Unreadable { path: PathBuf, message: String },
}

impl Manifest {
    /// Parses and validates a manifest's text. `path` is only for messages.
    pub fn parse(raw: &str, path: &Path) -> Result<Self, ManifestError> {
        let invalid = |message: String| ManifestError::Invalid {
            path: path.to_path_buf(),
            message,
        };
        let manifest: Manifest = toml::from_str(raw).map_err(|e| invalid(e.to_string()))?;
        manifest.validate().map_err(invalid)?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), String> {
        let commands = [
            ("app.setup", self.app.setup.as_deref()),
            ("app.build", self.app.build.as_deref()),
            ("app.start", Some(self.app.start.as_str())),
            ("app.seed", self.app.seed.as_deref()),
            ("app.test", self.app.test.as_deref()),
            ("app.stop", self.app.stop.as_deref()),
            ("workspace.setup", self.workspace.setup.as_deref()),
            ("workspace.cleanup", self.workspace.cleanup.as_deref()),
            ("workspace.archive", self.workspace.archive.as_deref()),
        ];
        let remote = self.app.remote.as_ref();
        let remote_commands = [
            ("app.remote.start", remote.map(|r| r.start.as_str())),
            ("app.remote.seed", remote.and_then(|r| r.seed.as_deref())),
            ("app.remote.test", remote.and_then(|r| r.test.as_deref())),
            ("app.remote.stop", remote.and_then(|r| r.stop.as_deref())),
            (
                "app.remote.restart",
                remote.and_then(|r| r.restart.as_deref()),
            ),
        ];
        for (key, command) in commands.into_iter().chain(remote_commands) {
            if command.is_some_and(|c| c.trim().is_empty()) {
                return Err(format!(
                    "{key} is empty; remove the key or give it a command"
                ));
            }
        }
        if let Some(ready) = &self.app.ready {
            validate_ready("app.ready", ready)?;
        }
        if let Some(remote) = remote {
            if let Some(ready) = &remote.ready {
                validate_ready("app.remote.ready", ready)?;
            }
            if remote.max_parallel == 0 {
                return Err("app.remote.max_parallel must be above 0".to_string());
            }
        }
        if let Some(name) = self
            .secrets
            .requires
            .iter()
            .find(|name| !secrets::is_valid_name(name))
        {
            return Err(format!(
                "secrets.requires has {name:?}, which is not a valid variable name"
            ));
        }
        Ok(())
    }

    /// The workspace setup hook: `[workspace] setup`, else `[app] setup`.
    pub fn workspace_setup(&self) -> Option<&str> {
        self.workspace
            .setup
            .as_deref()
            .or(self.app.setup.as_deref())
    }

    pub fn workspace_cleanup(&self) -> Option<&str> {
        self.workspace.cleanup.as_deref()
    }

    pub fn workspace_archive(&self) -> Option<&str> {
        self.workspace.archive.as_deref()
    }
}

fn validate_ready(key: &str, ready: &ReadyCheck) -> Result<(), String> {
    match (&ready.url, &ready.command) {
        (Some(url), None) => {
            if !url.starts_with('/') {
                return Err(format!(
                    "{key}.url {url:?} must be a path starting with '/'; it is \
                     appended to $PREPCOOK_APP_URL"
                ));
            }
        }
        (None, Some(command)) if !command.trim().is_empty() => {}
        (None, Some(_)) => return Err(format!("{key}.command is empty")),
        _ => {
            return Err(format!("{key} needs exactly one of `url` or `command`"));
        }
    }
    if ready.timeout_secs == 0 {
        return Err(format!("{key}.timeout_secs must be above 0"));
    }
    Ok(())
}

/// The manifest path for a worktree or repository root.
pub fn manifest_path(root: &Path) -> PathBuf {
    root.join(MANIFEST_PATH)
}

/// Whether `root` carries a manifest, without parsing it.
pub fn has_manifest(root: &Path) -> bool {
    manifest_path(root).is_file()
}

/// Reads and validates `root/.prepcook/scripts.toml`.
pub fn load_manifest(root: &Path) -> Result<Manifest, ManifestError> {
    let path = manifest_path(root);
    match std::fs::read_to_string(&path) {
        Ok(raw) => Manifest::parse(&raw, &path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ManifestError::Missing {
            repo: root.to_path_buf(),
        }),
        Err(e) => Err(ManifestError::Unreadable {
            path,
            message: e.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
[app]
setup = "pnpm install"
build = "pnpm build"
start = "pnpm start"
ready = { url = "/health", timeout_secs = 60 }
seed = "node scripts/seed.js"
test = "pnpm test"
stop = "pnpm stop"

[app.timeouts]
setup = 100
test = 200

[workspace]
cleanup = "rm -rf .cache"

[secrets]
requires = ["TEST_USER_PASSWORD"]

[profile]
name = "Example App"
summary = "One paragraph."
areas = ["Sign-in", "Dashboard"]
error_patterns = ["SeedDataMissing"]
evidence_tools = "The app logs to logs/app.log."
"#;

    fn parse(raw: &str) -> Result<Manifest, ManifestError> {
        Manifest::parse(raw, Path::new("/repo/.prepcook/scripts.toml"))
    }

    #[test]
    fn a_full_manifest_parses() {
        let manifest = parse(FULL).unwrap();
        assert_eq!(manifest.app.start, "pnpm start");
        let ready = manifest.app.ready.as_ref().unwrap();
        assert_eq!(ready.url.as_deref(), Some("/health"));
        assert_eq!(ready.timeout_secs, 60);
        assert_eq!(manifest.app.timeouts.setup, 100);
        assert_eq!(manifest.app.timeouts.test, 200);
        // Unset timeouts keep their defaults.
        assert_eq!(manifest.app.timeouts.build, 1200);
        assert_eq!(manifest.app.timeouts.start, 120);
        assert_eq!(manifest.secrets.requires, vec!["TEST_USER_PASSWORD"]);
        let profile = manifest.profile.as_ref().unwrap();
        assert_eq!(profile.areas.len(), 2);
        assert_eq!(
            profile.evidence_tools.as_deref(),
            Some("The app logs to logs/app.log.")
        );
        // workspace.setup falls back to app.setup.
        assert_eq!(manifest.workspace_setup(), Some("pnpm install"));
        assert_eq!(manifest.workspace_cleanup(), Some("rm -rf .cache"));
        assert_eq!(manifest.workspace_archive(), None);
    }

    #[test]
    fn the_documented_example_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/scripts.toml");
        let raw = std::fs::read_to_string(&path).unwrap();
        let manifest = Manifest::parse(&raw, &path).unwrap();
        let profile = manifest.profile.unwrap();
        assert_eq!(profile.name.as_deref(), Some("Pantry"));
        assert_eq!(profile.areas, ["Sign-in", "Lists", "Items", "Sharing"]);
        assert_eq!(profile.error_patterns, ["SeedMissing"]);
        assert!(profile.evidence_tools.unwrap().contains("logs/app.log"));
    }

    #[test]
    fn a_minimal_manifest_takes_the_defaults() {
        let manifest = parse("[app]\nstart = \"python3 -m http.server $PREPCOOK_PORT\"\n").unwrap();
        assert_eq!(manifest.app.timeouts, StepTimeouts::default());
        assert!(manifest.app.ready.is_none());
        assert!(manifest.secrets.requires.is_empty());
        assert!(manifest.profile.is_none());
        assert_eq!(manifest.workspace_setup(), None);
        let ready = parse("[app]\nstart = \"x\"\nready = { command = \"true\" }\n")
            .unwrap()
            .app
            .ready
            .unwrap();
        assert_eq!(ready.timeout_secs, 300);
    }

    #[test]
    fn unknown_keys_are_errors_that_name_the_key() {
        let error = parse("[app]\nstart = \"x\"\nstrat = \"y\"\n").unwrap_err();
        let message = error.to_string();
        assert!(matches!(error, ManifestError::Invalid { .. }));
        assert!(message.contains("strat"), "{message}");
        assert!(
            message.contains("/repo/.prepcook/scripts.toml"),
            "{message}"
        );

        let error = parse("[app]\nstart = \"x\"\n[app.timeouts]\nbuidl = 3\n").unwrap_err();
        assert!(error.to_string().contains("buidl"), "{error}");

        let error = parse("[app]\nstart = \"x\"\n[extras]\nx = 1\n").unwrap_err();
        assert!(error.to_string().contains("extras"), "{error}");
    }

    #[test]
    fn invalid_values_are_refused() {
        assert!(
            parse("[app]\nbuild = \"x\"\n").is_err(),
            "start is required"
        );
        assert!(parse("[app]\nstart = \"  \"\n").is_err());
        assert!(parse("[app]\nstart = \"x\"\nready = { timeout_secs = 3 }\n").is_err());
        assert!(
            parse("[app]\nstart = \"x\"\nready = { url = \"/\", command = \"true\" }\n").is_err()
        );
        assert!(parse("[app]\nstart = \"x\"\nready = { url = \"health\" }\n").is_err());
        assert!(parse("[app]\nstart = \"x\"\n[secrets]\nrequires = [\"NOT-OK\"]\n").is_err());
    }

    #[test]
    fn a_remote_table_parses_with_its_own_defaults_and_rules() {
        let manifest = parse(
            r#"
[app]
start = "pnpm start"

[app.remote]
start = "./scripts/deploy-preview.sh"
ready = { url = "/health", timeout_secs = 600 }
stop = "./scripts/destroy-preview.sh"

[app.remote.timeouts]
start = 900
"#,
        )
        .unwrap();
        let remote = manifest.app.remote.unwrap();
        assert_eq!(remote.start, "./scripts/deploy-preview.sh");
        assert_eq!(remote.ready.unwrap().timeout_secs, 600);
        assert_eq!(remote.max_parallel, 1);
        assert_eq!(remote.timeouts.start, 900);
        assert_eq!(remote.timeouts.stop, RemoteTimeouts::default().stop);
        assert!(remote.seed.is_none() && remote.restart.is_none());
        assert!(
            parse("[app]\nstart = \"x\"\n")
                .unwrap()
                .app
                .remote
                .is_none()
        );

        let bad = |table: &str| parse(&format!("[app]\nstart = \"x\"\n[app.remote]\n{table}"));
        assert!(
            bad("ready = { url = \"/\" }\n").is_err(),
            "start is required"
        );
        assert!(bad("start = \" \"\n").is_err());
        assert!(bad("start = \"d\"\nmax_parallel = 0\n").is_err());
        assert!(bad("start = \"d\"\nready = { url = \"health\" }\n").is_err());
        let error = bad("start = \"d\"\nstrat = \"y\"\n").unwrap_err();
        assert!(error.to_string().contains("strat"), "{error}");
        let error = bad("start = \"d\"\nrestart = \"\"\n").unwrap_err();
        assert!(error.to_string().contains("app.remote.restart"), "{error}");
    }

    #[test]
    fn a_missing_file_is_its_own_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = load_manifest(dir.path()).unwrap_err();
        assert!(matches!(error, ManifestError::Missing { .. }));
        let message = error.to_string();
        assert!(
            message.starts_with("no .prepcook/scripts.toml in ")
                && message.ends_with("see docs/scripts.md"),
            "{message}"
        );
        assert!(!has_manifest(dir.path()));

        std::fs::create_dir_all(dir.path().join(".prepcook")).unwrap();
        std::fs::write(manifest_path(dir.path()), FULL).unwrap();
        assert!(has_manifest(dir.path()));
        assert!(load_manifest(dir.path()).is_ok());
    }
}
