//! Reusable worktree pool backed by the `treehouse` CLI
//! (<https://github.com/kunchenguid/treehouse>).
//!
//! When the pool is enabled, workspace worktrees are leased from a warm,
//! per-repository pool (`~/.treehouse/<repo>-<hash>/<n>/<repo>`) instead of
//! being created from scratch under the temp workspace dir. Pooled trees keep
//! untracked-but-ignored state (node_modules, build caches) across tasks.
//!
//! Integration model:
//! - Acquire runs `treehouse get --lease`, which is non-interactive and
//!   prints only the pooled tree's absolute path to stdout. The lease is
//!   durable (persisted in treehouse state), so it survives app restarts.
//! - Callers dictate where the worktree must live (`<workspace_dir>/<repo>`),
//!   so the pooled tree is exposed there via a symlink. A workspace worktree
//!   path that is a symlink is, by construction, a pooled worktree — that is
//!   the pooled-vs-legacy discriminator used everywhere.
//! - Leased trees come back detached on the repo's default branch; the
//!   workspace branch (already created from its base in the main repo) is
//!   then checked out in place with `git checkout --force`.
//! - Release runs `treehouse return --force <path>` (terminates lingering
//!   processes, resets the tree, clears the lease) and removes the symlink.
//!   Pooled trees are never `git worktree remove`d or `remove_dir_all`ed by
//!   this app.
//!
//! Every treehouse failure (binary missing, pool full, old CLI without
//! `--lease`) degrades to the legacy temp-dir worktree path — acquisition
//! never blocks and never fails workspace creation on its own.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use git::GitCli;
use tracing::{debug, info, warn};
use utils::{command_ext::NoWindowExt, shell::resolve_executable_path_blocking};

use crate::worktree_manager::WorktreeError;

static POOL_CONFIG: OnceLock<PoolSettings> = OnceLock::new();

/// Marker line prefix identifying a `treehouse.toml` written by this app.
/// Files without it are user-managed and never touched.
#[cfg(unix)]
const MANAGED_HEADER: &str = "# Managed by prepcook";

#[derive(Debug, Clone)]
pub(crate) struct PoolSettings {
    pub enabled: bool,
    pub max_trees: u32,
    /// Pool root override (tilde-expanded). Treehouse nests pools under
    /// `<root>/.treehouse`; `None` means `~/.treehouse`.
    pub root: Option<PathBuf>,
}

/// Set once at startup from config (mirrors `set_workspace_dir_override`);
/// later calls are ignored, so config changes need an app restart.
pub(crate) fn set_config(enabled: bool, max_trees: u32, root: Option<PathBuf>) {
    let _ = POOL_CONFIG.set(PoolSettings {
        enabled,
        max_trees,
        root,
    });
}

fn config() -> Option<&'static PoolSettings> {
    POOL_CONFIG.get()
}

/// Pool is only usable on unix: pooled trees are surfaced through symlinks.
pub(crate) fn enabled() -> bool {
    cfg!(unix) && config().is_some_and(|c| c.enabled)
}

/// A workspace worktree is pooled iff its path is a symlink — this app only
/// ever materializes a workspace worktree as a symlink when it points at a
/// pooled tree. (No pool-root prefix check: it would not survive the user
/// changing the pool root between runs.)
pub(crate) fn is_pooled(worktree_path: &Path) -> bool {
    fs::symlink_metadata(worktree_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// For pooled (symlinked) worktrees, check the tree is still checked out on
/// the workspace branch. A pooled tree that was manually returned to the
/// pool (`treehouse return`) sits detached on the default branch and may be
/// handed to another agent — callers must treat that as "needs recreation".
/// Non-pooled paths trivially pass.
pub(crate) async fn is_on_branch(
    worktree_path: &Path,
    branch_name: &str,
) -> Result<bool, WorktreeError> {
    if !is_pooled(worktree_path) {
        return Ok(true);
    }
    let worktree_path = worktree_path.to_path_buf();
    let branch_name = branch_name.to_string();
    tokio::task::spawn_blocking(move || -> Result<bool, WorktreeError> {
        // Exits non-zero when HEAD is detached, which maps to Err -> false.
        let head = GitCli::new().git(
            &worktree_path,
            ["symbolic-ref", "--quiet", "--short", "HEAD"],
        );
        Ok(matches!(head, Ok(h) if h.trim() == branch_name))
    })
    .await
    .map_err(|e| WorktreeError::TaskJoin(format!("{e}")))?
}

/// Try to lease a pooled worktree for `branch_name` and surface it at
/// `worktree_path` (which must not exist yet). Returns `Ok(false)` whenever
/// the caller should fall back to the legacy temp-dir worktree.
pub(crate) async fn acquire(
    repo_path: &Path,
    branch_name: &str,
    worktree_path: &Path,
) -> Result<bool, WorktreeError> {
    let repo_path = repo_path.to_path_buf();
    let branch_name = branch_name.to_string();
    let worktree_path = worktree_path.to_path_buf();
    tokio::task::spawn_blocking(move || acquire_blocking(&repo_path, &branch_name, &worktree_path))
        .await
        .map_err(|e| WorktreeError::TaskJoin(format!("{e}")))?
}

/// Return a pooled worktree to the pool and remove the workspace symlink.
/// Best-effort by design: cleanup paths must keep working even when the
/// treehouse binary has gone missing, so failures degrade to warnings.
pub(crate) async fn release(worktree_path: &Path) -> Result<(), WorktreeError> {
    let worktree_path = worktree_path.to_path_buf();
    tokio::task::spawn_blocking(move || release_blocking(&worktree_path))
        .await
        .map_err(|e| WorktreeError::TaskJoin(format!("{e}")))?
}

#[cfg(not(unix))]
fn acquire_blocking(
    _repo_path: &Path,
    _branch_name: &str,
    _worktree_path: &Path,
) -> Result<bool, WorktreeError> {
    Ok(false)
}

#[cfg(unix)]
fn acquire_blocking(
    repo_path: &Path,
    branch_name: &str,
    worktree_path: &Path,
) -> Result<bool, WorktreeError> {
    let Some(cfg) = config().filter(|c| c.enabled) else {
        return Ok(false);
    };
    let Some(treehouse) = resolve_executable_path_blocking("treehouse") else {
        warn!(
            "worktree pool is enabled but the 'treehouse' CLI was not found on PATH; \
             falling back to a temp-dir worktree (install: \
             curl -fsSL https://kunchenguid.github.io/treehouse/install.sh | sh)"
        );
        return Ok(false);
    };

    if let Err(e) = write_repo_pool_config(repo_path, cfg) {
        debug!("could not write treehouse.toml (non-fatal): {e}");
    }

    // Non-interactive durable acquire: stdout is exactly the pooled path,
    // human chatter goes to stderr. Errors (pool full, CLI without --lease)
    // mean "fall back", never "fail the workspace".
    let output = Command::new(&treehouse)
        .args(["get", "--lease", "--lease-holder"])
        .arg(format!("prepcook:{branch_name}"))
        .current_dir(repo_path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .no_window()
        .output()
        .map_err(WorktreeError::Io)?;
    if !output.status.success() {
        warn!(
            "treehouse get --lease failed (pool full or CLI too old?); falling back to a \
             temp-dir worktree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return Ok(false);
    }
    let pooled = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if pooled.as_os_str().is_empty() || !pooled.join(".git").is_file() {
        warn!(
            "treehouse get --lease printed an unusable path '{}'; returning lease",
            pooled.display()
        );
        return_lease(&treehouse, &pooled);
        return Ok(false);
    }

    // Leased trees come back clean and detached on the default branch; put
    // this one on the workspace branch (already created from its base).
    if let Err(e) = GitCli::new().git(&pooled, ["checkout", "--force", branch_name]) {
        warn!(
            "failed to checkout '{}' in pooled worktree {}: {}; returning lease",
            branch_name,
            pooled.display(),
            e
        );
        return_lease(&treehouse, &pooled);
        return Ok(false);
    }

    // PORT_OFFSET convention for parallel dev stacks (the target repository's ./m):
    // pooled trees live at <pool>/<slot>/<repo>, and slot N's stack runs at
    // port offset N*10 — recorded in a .port-offset file at the checkout
    // root, which ./m reads (env var wins, missing file means 0). Only
    // written when the repo gitignores the file, so repos without the
    // convention never see untracked dirt.
    if let Err(e) = write_port_offset(&pooled) {
        debug!("could not write .port-offset (non-fatal): {e}");
    }

    // Surface the pooled tree where callers expect the worktree to live.
    if let Some(parent) = worktree_path.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::symlink_metadata(worktree_path) {
        // Stale pooled link (its release already ran or its target vanished).
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(worktree_path)?,
        Ok(_) => {
            // A real directory is a legacy worktree; don't shadow it.
            warn!(
                "cannot pool worktree: {} already exists; returning lease",
                worktree_path.display()
            );
            return_lease(&treehouse, &pooled);
            return Ok(false);
        }
        Err(_) => {}
    }
    std::os::unix::fs::symlink(&pooled, worktree_path)?;
    info!(
        "acquired pooled worktree {} -> {} (branch {})",
        worktree_path.display(),
        pooled.display(),
        branch_name
    );
    Ok(true)
}

fn release_blocking(worktree_path: &Path) -> Result<(), WorktreeError> {
    let target = match fs::read_link(worktree_path) {
        Ok(target) => target,
        Err(e) => {
            debug!(
                "pool release: {} is not a symlink ({}); nothing to do",
                worktree_path.display(),
                e
            );
            return Ok(());
        }
    };

    // Drop the link first so workspace-dir removal can always proceed, even
    // if returning the lease below fails.
    if let Err(e) = fs::remove_file(worktree_path) {
        warn!(
            "failed to remove pooled worktree link {}: {}",
            worktree_path.display(),
            e
        );
    }

    if !target.exists() {
        debug!(
            "pooled worktree {} is already gone; nothing to return",
            target.display()
        );
        return Ok(());
    }

    // A tree that is already detached and clean was returned behind our back
    // (e.g. a manual `treehouse return`) and may have been leased to someone
    // else since; returning it again would terminate their processes.
    let git = GitCli::new();
    let detached = git
        .git(&target, ["symbolic-ref", "--quiet", "HEAD"])
        .is_err();
    let clean = git
        .git(&target, ["status", "--porcelain"])
        .map(|out| out.trim().is_empty())
        .unwrap_or(false);
    if detached && clean {
        debug!(
            "pooled worktree {} is already detached and clean; skipping return",
            target.display()
        );
        return Ok(());
    }

    if let Some(treehouse) = resolve_executable_path_blocking("treehouse") {
        // `return --force`: terminates lingering processes in the tree,
        // resets it (detached on the default branch), and clears the lease.
        match Command::new(&treehouse)
            .args(["return", "--force"])
            .arg(&target)
            .env("GIT_TERMINAL_PROMPT", "0")
            .no_window()
            .output()
        {
            Ok(output) if output.status.success() => {
                info!("returned pooled worktree {} to pool", target.display());
                return Ok(());
            }
            Ok(output) => warn!(
                "treehouse return --force {} failed: {}",
                target.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(e) => warn!("failed to run treehouse return: {e}"),
        }
    } else {
        warn!(
            "'treehouse' CLI not found; cannot return pooled worktree {} to the pool",
            target.display()
        );
    }

    // Degraded release: reset the tree in place so no stale work lingers.
    // The lease stays recorded, which is safe (the slot won't be reused) but
    // leaks it until the user runs `treehouse return --force <path>`.
    for args in [
        ["checkout", "--detach"],
        ["reset", "--hard"],
        ["clean", "-fd"],
    ] {
        if let Err(e) = git.git(&target, args) {
            debug!(
                "pool release: git {:?} in {} failed (non-fatal): {}",
                args,
                target.display(),
                e
            );
        }
    }
    warn!(
        "pooled worktree {} was reset but its lease is still held; run \
         `treehouse return --force {}` to reclaim the pool slot",
        target.display(),
        target.display()
    );
    Ok(())
}

/// Writes the slot-derived `.port-offset` for repos that opt into the
/// parallel-stack convention (the file is in their .gitignore). Slot number
/// comes from the pooled path's parent directory (`<pool>/<slot>/<repo>`).
#[cfg(unix)]
fn write_port_offset(pooled: &Path) -> std::io::Result<()> {
    let Some(slot) = pooled
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .and_then(|name| name.parse::<u32>().ok())
    else {
        return Ok(());
    };
    // ./m accepts offsets 0-90 only (main checkout owns 0). max_trees is
    // clamped to 9, but a user-managed treehouse.toml can size the pool
    // larger — never write an offset the stack tooling would reject.
    if slot > 9 {
        warn!(
            "pooled worktree {} is in slot {slot} (port offset {} exceeds 90); \
             skipping .port-offset — its dev stack would collide or be rejected",
            pooled.display(),
            slot * 10
        );
        return Ok(());
    }
    // check-ignore exits zero only when the path is ignored.
    if GitCli::new()
        .git(pooled, ["check-ignore", "-q", ".port-offset"])
        .is_err()
    {
        return Ok(());
    }
    fs::write(pooled.join(".port-offset"), format!("{}\n", slot * 10))
}

/// Best-effort lease return used when acquisition fails halfway.
#[cfg(unix)]
fn return_lease(treehouse: &Path, pooled: &Path) {
    if pooled.as_os_str().is_empty() {
        return;
    }
    match Command::new(treehouse)
        .args(["return", "--force"])
        .arg(pooled)
        .env("GIT_TERMINAL_PROMPT", "0")
        .no_window()
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => warn!(
            "treehouse return --force {} failed: {}",
            pooled.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(e) => warn!("failed to run treehouse return: {e}"),
    }
}

/// Treehouse reads `max_trees`/`root` from a repo-level `treehouse.toml`
/// (there is no CLI flag or env var), so pool sizing from app settings is
/// materialized there on acquire. Files not carrying the managed header are
/// user-owned and left untouched — the user's own sizing then wins.
#[cfg(unix)]
fn write_repo_pool_config(repo_path: &Path, cfg: &PoolSettings) -> std::io::Result<()> {
    let path = repo_path.join("treehouse.toml");
    let mut content = format!(
        "{MANAGED_HEADER} — pool sizing comes from Settings > Worktree pool; edits are overwritten.\nmax_trees = {}\n",
        cfg.max_trees
    );
    if let Some(root) = &cfg.root {
        let escaped = root
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        content.push_str(&format!("root = \"{escaped}\"\n"));
    }
    match fs::read_to_string(&path) {
        Ok(existing) if !existing.starts_with(MANAGED_HEADER) => {
            debug!(
                "treehouse.toml at {} is user-managed; leaving it alone",
                path.display()
            );
            return Ok(());
        }
        Ok(existing) if existing == content => return Ok(()),
        _ => {}
    }
    fs::write(&path, content)
}
