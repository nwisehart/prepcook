//! Cross-platform shell command utilities

use std::{
    collections::HashSet,
    env::{join_paths, split_paths},
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use crate::tokio::block_on;

/// Returns the appropriate shell command and argument for the current platform.
///
/// Returns (shell_program, shell_arg) where:
/// - Windows: ("cmd", "/C")
/// - Unix-like: ("sh", "-c") or ("bash", "-c") if available
pub fn get_shell_command() -> (String, &'static str) {
    if cfg!(windows) {
        ("cmd".into(), "/C")
    } else {
        UnixShell::current_shell().get_shell_command()
    }
}

/// Returns the path to an interactive shell for the current platform.
/// Used for spawning PTY sessions.
///
/// On Windows, prefers PowerShell if available, falling back to cmd.exe.
/// On Unix, returns the user's configured shell from $SHELL.
pub async fn get_interactive_shell() -> PathBuf {
    if cfg!(windows) {
        // Prefer PowerShell if available, fall back to cmd.exe
        if let Some(powershell) = resolve_executable_path("powershell.exe").await {
            powershell
        } else {
            PathBuf::from("cmd.exe")
        }
    } else {
        UnixShell::current_shell().path().to_path_buf()
    }
}

/// Resolve an executable by name, falling back to a refreshed PATH if needed.
///
/// The search order is:
/// 1. Explicit paths (absolute or containing a separator).
/// 2. The current process PATH via `which`.
/// 3. A platform-specific refresh of PATH (login shell on Unix, PowerShell on Windows),
///    after which we re-run the `which` lookup and update the process PATH for future calls.
pub async fn resolve_executable_path(executable: &str) -> Option<PathBuf> {
    if executable.trim().is_empty() {
        return None;
    }

    let path = Path::new(executable);
    if path.is_absolute() && path.is_file() {
        return Some(path.to_path_buf());
    }

    if let Some(found) = which(executable).await {
        return Some(found);
    }

    if refresh_path().await
        && let Some(found) = which(executable).await
    {
        return Some(found);
    }

    None
}

pub fn resolve_executable_path_blocking(executable: &str) -> Option<PathBuf> {
    block_on(resolve_executable_path(executable))
}

/// Merge two PATH strings into a single, de-duplicated PATH.
///
/// - Keeps the order of entries from `primary`.
/// - Appends only *unseen* entries from `secondary`.
/// - Ignores empty components.
/// - Returns a platform-correct PATH string (using the OS separator).
pub fn merge_paths(primary: impl AsRef<OsStr>, secondary: impl AsRef<OsStr>) -> OsString {
    let mut seen = HashSet::<PathBuf>::new();
    let mut merged = Vec::<PathBuf>::new();

    for p in split_paths(primary.as_ref()).chain(split_paths(secondary.as_ref())) {
        if !p.as_os_str().is_empty() && seen.insert(p.clone()) {
            merged.push(p);
        }
    }

    join_paths(merged).unwrap_or_default()
}

async fn refresh_path() -> bool {
    // One refresh per process: it spawns login shells, so repeating it on
    // every lookup miss made each failed agent spawn wait out the shell
    // timeouts all over again.
    static REFRESHED: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();
    if let Some(done) = REFRESHED.get() {
        return *done;
    }
    let outcome = refresh_path_once().await;
    let _ = REFRESHED.set(outcome);
    outcome
}

async fn refresh_path_once() -> bool {
    let Some(refreshed) = get_fresh_path().await else {
        return false;
    };
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let refreshed_os = OsString::from(&refreshed);
    let merged = merge_paths(&existing, refreshed_os);
    if merged == existing {
        return false;
    }
    tracing::debug!(?existing, ?refreshed, ?merged, "Refreshed PATH");
    unsafe {
        std::env::set_var("PATH", &merged);
    }
    true
}

async fn which(executable: &str) -> Option<PathBuf> {
    let executable = executable.to_string();
    tokio::task::spawn_blocking(move || which::which(executable))
        .await
        .ok()
        .and_then(|result| result.ok())
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnixShell {
    Zsh(PathBuf),
    Bash(PathBuf),
    Sh(PathBuf),
    Other(PathBuf),
}

impl UnixShell {
    pub fn path(&self) -> &Path {
        match self {
            UnixShell::Zsh(p) | UnixShell::Bash(p) | UnixShell::Sh(p) | UnixShell::Other(p) => p,
        }
    }
    pub fn login(&self) -> bool {
        matches!(self, UnixShell::Zsh(_) | UnixShell::Bash(_))
    }
    pub fn config_file(&self) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let config_file = match self {
            UnixShell::Zsh(_) => Some(home.join(".zshrc")),
            UnixShell::Bash(_) => Some(home.join(".bashrc")),
            UnixShell::Sh(_) | UnixShell::Other(_) => None,
        };
        config_file.filter(|p| p.is_file())
    }

    pub fn source_command(&self) -> Option<String> {
        if let Some(source_file) = self.config_file()
            && let Ok(escaped_source_file) =
                shlex::try_quote(source_file.to_string_lossy().as_ref())
        {
            Some(format!("source {escaped_source_file}"))
        } else {
            None
        }
    }
    pub fn current_shell() -> UnixShell {
        if let Ok(shell) = std::env::var("SHELL")
            && let Some(shell) = UnixShell::from_path(Path::new(&shell))
        {
            return shell;
        }
        UnixShell::Sh(PathBuf::from("/bin/sh"))
    }
    pub fn from_path(path: &Path) -> Option<UnixShell> {
        if path.is_absolute() && path.is_file() {
            let path_buf = path.to_path_buf();
            if path.file_name() == Some(OsStr::new("zsh")) {
                Some(UnixShell::Zsh(path_buf))
            } else if path.file_name() == Some(OsStr::new("bash")) {
                Some(UnixShell::Bash(path_buf))
            } else if path.file_name() == Some(OsStr::new("sh")) {
                Some(UnixShell::Sh(path_buf))
            } else {
                Some(UnixShell::Other(path_buf))
            }
        } else {
            None
        }
    }
    pub fn get_shell_command(&self) -> (String, &'static str) {
        (self.path().to_string_lossy().into_owned(), "-c")
    }
}

#[cfg(not(windows))]
async fn get_fresh_path() -> Option<String> {
    use std::{process::Stdio, time::Duration};

    use tokio::process::Command;

    async fn run(shell: &UnixShell) -> Option<String> {
        let mut cmd = Command::new(shell.path());
        if shell.login() {
            cmd.arg("-l");
        }
        if let Some(source_command) = shell.source_command() {
            cmd.arg("-c")
                .arg(format!("{source_command}; printf '%s' \"$PATH\""));
        } else {
            cmd.arg("-c").arg("printf '%s' \"$PATH\"");
        }
        cmd.env("TERM", "dumb")
            // stdin MUST be /dev/null: the shell sources the user's rc files,
            // and anything there that reads stdin (prompt frameworks, version
            // managers, `read` guards) blocks on an inherited stdin until the
            // timeout kills us. GUI launches hit exactly that — every refresh
            // burned the full timeout for zsh AND bash and recovered nothing,
            // so agents found neither `claude` nor `npx`.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Heavy rc files (nvm, pyenv, completions) genuinely need a few
        // seconds on a cold shell.
        const PATH_REFRESH_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

        let child = cmd.spawn().ok()?;
        let output = match tokio::time::timeout(
            PATH_REFRESH_COMMAND_TIMEOUT,
            child.wait_with_output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => {
                tracing::debug!(
                    shell = %shell.path().display(),
                    ?err,
                    "Failed to retrieve PATH from login shell"
                );
                return None;
            }
            Err(_) => {
                tracing::warn!(
                    shell = %shell.path().display(),
                    timeout_secs = PATH_REFRESH_COMMAND_TIMEOUT.as_secs(),
                    "Timed out retrieving PATH from login shell"
                );
                return None;
            }
        };

        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if path.is_empty() { None } else { Some(path) }
    }

    let mut paths = Vec::new();

    let current_shell = UnixShell::current_shell();
    if let Some(path) = run(&current_shell).await {
        paths.push(path);
    }

    let shells: Vec<UnixShell> = ["/bin/zsh", "/bin/bash", "/bin/sh"]
        .into_iter()
        .filter_map(|p| UnixShell::from_path(Path::new(p)))
        .collect();
    for shell in shells {
        if !(shell == current_shell)
            && let Some(path) = run(&shell).await
        {
            paths.push(path);
        }
    }

    // Shell-free fallback: conventional install locations for the CLIs
    // agents need. A GUI launch whose login shell cannot be interrogated
    // (or whose rc files hang) still finds a globally installed `claude`,
    // `npx`, `glab` or `treehouse`.
    if let Some(well_known) = well_known_bin_dirs() {
        paths.push(well_known);
    }

    if paths.is_empty() {
        return None;
    }

    paths
        .into_iter()
        .map(OsString::from)
        .reduce(|a, b| merge_paths(&a, &b))
        .map(|merged| merged.to_string_lossy().into_owned())
}

/// Conventional bin directories for user-installed CLIs, as a PATH string.
/// Existence-checked so the merged PATH stays meaningful.
#[cfg(not(windows))]
fn well_known_bin_dirs() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let mut dirs: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/local/bin"),
    ];
    for relative in [
        ".local/bin",
        ".npm-global/bin",
        ".volta/bin",
        ".bun/bin",
        ".cargo/bin",
    ] {
        dirs.push(PathBuf::from(&home).join(relative));
    }
    // Every installed nvm node version, sorted so newer paths come later.
    if let Ok(entries) = std::fs::read_dir(PathBuf::from(&home).join(".nvm/versions/node")) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path().join("bin")))
            .collect();
        versions.sort();
        dirs.extend(versions);
    }
    let existing: Vec<PathBuf> = dirs.into_iter().filter(|dir| dir.is_dir()).collect();
    if existing.is_empty() {
        return None;
    }
    Some(
        join_paths(existing)
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(windows)]
async fn get_fresh_path() -> Option<String> {
    tokio::task::spawn_blocking(get_fresh_path_blocking)
        .await
        .ok()
        .flatten()
}

#[cfg(windows)]
fn get_fresh_path_blocking() -> Option<String> {
    use std::{
        ffi::{OsStr, OsString},
        os::windows::ffi::{OsStrExt, OsStringExt},
    };

    use winreg::{HKEY, RegKey, enums::*};

    // Expand %VARS% for registry PATH entries
    fn expand_env_vars(input: &OsStr) -> OsString {
        use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;

        let wide: Vec<u16> = input.encode_wide().chain(Some(0)).collect();
        unsafe {
            let needed = ExpandEnvironmentStringsW(wide.as_ptr(), std::ptr::null_mut(), 0);
            if needed == 0 {
                return input.to_os_string();
            }
            let mut buf = vec![0u16; needed as usize];
            let written = ExpandEnvironmentStringsW(wide.as_ptr(), buf.as_mut_ptr(), needed);
            if written == 0 {
                return input.to_os_string();
            }
            // written includes the trailing NUL when it fits
            OsString::from_wide(&buf[..(written as usize).saturating_sub(1)])
        }
    }

    fn read_registry_path(root: HKEY, subkey: &str) -> Option<OsString> {
        let key = RegKey::predef(root)
            .open_subkey_with_flags(subkey, KEY_READ)
            .ok()?;
        key.get_value::<String, _>("Path").ok().map(OsString::from)
    }

    let mut paths: Vec<OsString> = Vec::new();

    if let Some(user_path) = read_registry_path(HKEY_CURRENT_USER, "Environment") {
        paths.push(expand_env_vars(&user_path));
    }

    if let Some(machine_path) = read_registry_path(
        HKEY_LOCAL_MACHINE,
        r"System\CurrentControlSet\Control\Session Manager\Environment",
    ) {
        paths.push(expand_env_vars(&machine_path));
    }

    if paths.is_empty() {
        return None;
    }

    paths
        .into_iter()
        .map(OsString::from)
        .reduce(|a, b| merge_paths(&a, &b))
        .map(|merged| merged.to_string_lossy().into_owned())
}
