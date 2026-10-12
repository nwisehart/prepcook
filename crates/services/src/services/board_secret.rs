//! The board secret: the board's `PREPCOOK_JWT_SECRET`, held by this server
//! as `PREPCOOK_BOARD_SECRET`. The server signs a short-lived owner token with
//! it for every request to the board (`utils::jwt::mint_owner_token`), so
//! there is no sign-in and nothing to refresh or store.
//!
//! Where it comes from, first match wins:
//! 1. the board `.env` `prepcook up` started the board with (see
//!    [`use_board_env_file`]);
//! 2. the `PREPCOOK_BOARD_SECRET` environment variable;
//! 3. `PREPCOOK_BOARD_SECRET` in the secrets store;
//! 4. `PREPCOOK_JWT_SECRET` in `<app data dir>/board/.env`, when the board is
//!    the one `prepcook up` runs on this machine.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use chrono::Utc;
use utils::{assets::asset_dir, jwt::mint_owner_token};

use super::secrets;

/// The environment variable, and the secrets-store name, of the board secret.
pub const BOARD_SECRET_NAME: &str = "PREPCOOK_BOARD_SECRET";

/// The key holding the secret in a board `.env`.
pub const BOARD_ENV_SECRET_KEY: &str = "PREPCOOK_JWT_SECRET";

/// The board `prepcook up` runs on this machine (`REMOTE_SERVER_PORTS` in its
/// compose file).
pub const LOCAL_BOARD_URL: &str = "http://127.0.0.1:4110";

/// The board `.env` `prepcook up` started, read before any other source.
static UP_BOARD_ENV_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Makes `path` (the `.env` `prepcook up` started the board with) the first
/// source of the board secret. Call before the deployment is created.
pub fn use_board_env_file(path: PathBuf) {
    let _ = UP_BOARD_ENV_FILE.set(path);
}

/// Where `prepcook up` keeps the board's `.env`.
pub fn board_env_file() -> PathBuf {
    asset_dir().join("board").join(".env")
}

/// Whether `base` (a `PREPCOOK_SHARED_API_BASE` value) is the board
/// `prepcook up` runs on this machine.
pub fn is_local_board(base: &str) -> bool {
    let base = base.trim().trim_end_matches('/');
    base == LOCAL_BOARD_URL || base == "http://localhost:4110"
}

/// The value of `key` in the contents of a `.env`. Accepts the forms Compose
/// reads: `KEY=value`, optionally `export `-prefixed and quoted; ignores
/// comments, blank lines and empty values.
pub fn env_file_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim();
        let unquoted = ['"', '\'']
            .iter()
            .find_map(|q| value.strip_prefix(*q)?.strip_suffix(*q))
            .unwrap_or(value);
        Some(unquoted.to_string()).filter(|v| !v.is_empty())
    })
}

/// A board secret: standard base64, as the board reads it. Its `Debug` never
/// shows the value.
#[derive(Clone)]
pub struct BoardSecret(Arc<str>);

impl std::fmt::Debug for BoardSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoardSecret(<redacted>)")
    }
}

impl BoardSecret {
    /// `None` when `value` is not one the board could have signed with (not
    /// standard base64).
    pub fn new(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || mint_owner_token(value, Utc::now(), 60).is_err() {
            return None;
        }
        secrets::register_redaction(value);
        Some(Self(Arc::from(value)))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Where a board secret was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardSecretSource {
    BoardEnvFile(PathBuf),
    Environment,
    SecretsStore,
}

impl std::fmt::Display for BoardSecretSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BoardEnvFile(path) => write!(f, "{}", path.display()),
            Self::Environment => write!(f, "the {BOARD_SECRET_NAME} environment variable"),
            Self::SecretsStore => write!(f, "{BOARD_SECRET_NAME} in the secrets store"),
        }
    }
}

/// The board secret for a server whose board is `board_base`, and where it
/// was found (see the module docs for the order).
pub fn resolve(board_base: Option<&str>) -> Option<(BoardSecret, BoardSecretSource)> {
    resolve_from(
        UP_BOARD_ENV_FILE.get().map(PathBuf::as_path),
        std::env::var(BOARD_SECRET_NAME).ok().as_deref(),
        secrets::load().get(BOARD_SECRET_NAME),
        board_base
            .filter(|base| is_local_board(base))
            .map(|_| board_env_file())
            .as_deref(),
    )
}

fn resolve_from(
    up_board_env: Option<&Path>,
    environment: Option<&str>,
    store: Option<&str>,
    local_board_env: Option<&Path>,
) -> Option<(BoardSecret, BoardSecretSource)> {
    let from_env_file = |path: &Path| {
        let contents = std::fs::read_to_string(path).ok()?;
        let value = env_file_value(&contents, BOARD_ENV_SECRET_KEY)?;
        let secret = valid(&value, path.display())?;
        Some((secret, BoardSecretSource::BoardEnvFile(path.to_path_buf())))
    };

    up_board_env
        .and_then(from_env_file)
        .or_else(|| {
            let secret = valid(environment?, BOARD_SECRET_NAME)?;
            Some((secret, BoardSecretSource::Environment))
        })
        .or_else(|| {
            let secret = valid(store?, "the secrets store")?;
            Some((secret, BoardSecretSource::SecretsStore))
        })
        .or_else(|| local_board_env.and_then(from_env_file))
}

/// The secret in `value`, or `None` with a warning naming `origin` when it is
/// set but unusable.
fn valid(value: &str, origin: impl std::fmt::Display) -> Option<BoardSecret> {
    if value.trim().is_empty() {
        return None;
    }
    let secret = BoardSecret::new(value);
    if secret.is_none() {
        tracing::warn!("ignoring the board secret from {origin}: it is not standard base64");
    }
    secret
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const OTHER: &str = "HxweHRwbGhkYFxYVFBMSERAPDg0MCwoJCAcGBQQDAgE=";

    fn env_file(dir: &Path, name: &str, secret: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!("DB_PASSWORD=x\n{BOARD_ENV_SECRET_KEY}={secret}\n"),
        )
        .unwrap();
        path
    }

    #[test]
    fn the_up_board_env_comes_first_then_env_store_and_local_board() {
        let dir = tempfile::tempdir().unwrap();
        let up = env_file(dir.path(), "up.env", SECRET);
        let local = env_file(dir.path(), "local.env", OTHER);

        let (secret, source) =
            resolve_from(Some(&up), Some(OTHER), Some(OTHER), Some(&local)).unwrap();
        assert_eq!(
            (secret.expose(), source),
            (SECRET, BoardSecretSource::BoardEnvFile(up.clone()))
        );

        let (secret, source) = resolve_from(None, Some(SECRET), Some(OTHER), Some(&local)).unwrap();
        assert_eq!(
            (secret.expose(), source),
            (SECRET, BoardSecretSource::Environment)
        );

        let (secret, source) = resolve_from(None, None, Some(SECRET), Some(&local)).unwrap();
        assert_eq!(
            (secret.expose(), source),
            (SECRET, BoardSecretSource::SecretsStore)
        );

        let (secret, source) = resolve_from(None, None, None, Some(&local)).unwrap();
        assert_eq!(
            (secret.expose(), source),
            (OTHER, BoardSecretSource::BoardEnvFile(local))
        );

        assert!(resolve_from(None, None, None, None).is_none());
        assert!(resolve_from(Some(&dir.path().join("absent.env")), None, None, None).is_none());
    }

    #[test]
    fn a_value_that_is_not_base64_is_skipped() {
        let (secret, source) = resolve_from(None, Some("not base64!"), Some(SECRET), None).unwrap();
        assert_eq!(
            (secret.expose(), source),
            (SECRET, BoardSecretSource::SecretsStore)
        );
    }

    #[test]
    fn a_board_env_with_other_keys_still_yields_its_secret() {
        // Keys the board no longer reads (such as sign-in lines) are ignored.
        let contents = format!(
            "# comment\nDB_PASSWORD=a\nexport {BOARD_ENV_SECRET_KEY} = \"{SECRET}\"\n\
             SELF_HOST_LOCAL_AUTH_EMAIL=you@localhost\nSELF_HOST_LOCAL_AUTH_PASSWORD=p\n"
        );
        assert_eq!(
            env_file_value(&contents, BOARD_ENV_SECRET_KEY).as_deref(),
            Some(SECRET)
        );
        assert_eq!(env_file_value("KEY=\n", "KEY"), None);
        assert_eq!(env_file_value("KEY='p=w'\n", "KEY").as_deref(), Some("p=w"));
    }

    #[test]
    fn only_the_local_board_reads_the_local_board_env() {
        assert!(is_local_board("http://127.0.0.1:4110"));
        assert!(is_local_board("http://localhost:4110/"));
        assert!(!is_local_board("https://board.example.com"));
        assert!(!is_local_board(""));
    }

    #[test]
    fn a_board_secret_never_debug_prints_its_value() {
        let secret = BoardSecret::new(SECRET).unwrap();
        assert!(!format!("{secret:?}").contains(SECRET));
    }
}
