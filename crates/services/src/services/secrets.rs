//! The secrets store: values a target app's scripts need that must never be
//! committed, logged or posted — a test user's password, an API key for a
//! sandbox account.
//!
//! One file, `~/.config/prepcook/secrets.env` on every platform (`PREPCOOK_SECRETS_FILE`
//! overrides the path), holding `NAME=VALUE` lines and `#` comments. A process
//! environment variable `PREPCOOK_SECRET_<NAME>` overrides the file's entry, which
//! is how a hosted runner injects values without writing them to disk.
//!
//! Every value this module has seen, and every value registered through
//! [`register_redaction`], is replaced by `[redacted]` in [`redact`]. That is
//! the process-wide filter applied to script logs, card comments, merge request
//! notes and Slack text, so a value that reached a log line by accident is not
//! published with it.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
};

/// Environment variable that points the store at another file.
pub const SECRETS_FILE_ENV: &str = "PREPCOOK_SECRETS_FILE";
/// Prefix of the environment variables that override file entries, and of the
/// variables scripts receive each secret under.
pub const SECRET_ENV_PREFIX: &str = "PREPCOOK_SECRET_";
/// What a redacted value is replaced with.
pub const REDACTED: &str = "[redacted]";
/// Values shorter than this are not redacted: replacing every `true` or `1234`
/// in a log would make it unreadable and hides nothing worth hiding.
const MIN_REDACT_LEN: usize = 6;

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error(
        "invalid secret name {0:?}: use letters, digits and underscores, not starting with a digit"
    )]
    InvalidName(String),
    #[error("secret values cannot contain a line break")]
    InvalidValue,
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Where the secrets file lives: `PREPCOOK_SECRETS_FILE`, else
/// `~/.config/prepcook/secrets.env`. The same path on macOS, Linux and Windows on
/// purpose — the file is documented once and found in one place.
pub fn secrets_path() -> PathBuf {
    if let Some(path) = std::env::var_os(SECRETS_FILE_ENV).filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("prepcook")
        .join("secrets.env")
}

/// Whether `name` is usable as an environment variable name.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The secrets visible to this process: the file's entries with the
/// environment's `PREPCOOK_SECRET_*` overrides on top.
#[derive(Clone, Default)]
pub struct Secrets {
    path: PathBuf,
    file: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
}

// Written by hand so a `{:?}` can only ever print names.
impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets")
            .field("path", &self.path)
            .field("names", &self.names())
            .finish()
    }
}

/// Reads the store from its usual place; see [`Secrets::load`].
pub fn load() -> Secrets {
    Secrets::load()
}

impl Secrets {
    /// Reads the file (absent is empty) and the process environment, and
    /// registers every value for redaction.
    pub fn load() -> Self {
        Self::load_from(&secrets_path(), std::env::vars())
    }

    /// [`Self::load`] with the file and environment given explicitly.
    pub fn load_from(path: &Path, env: impl IntoIterator<Item = (String, String)>) -> Self {
        let file = std::fs::read_to_string(path)
            .map(|raw| parse(&raw))
            .unwrap_or_default();
        let env = env
            .into_iter()
            .filter_map(|(key, value)| {
                let name = key.strip_prefix(SECRET_ENV_PREFIX)?;
                is_valid_name(name).then(|| (name.to_string(), value))
            })
            .collect();
        let secrets = Self {
            path: path.to_path_buf(),
            file,
            env,
        };
        for value in secrets.file.values().chain(secrets.env.values()) {
            register_redaction(value);
        }
        secrets
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The value for `name`, the environment override first.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.env
            .get(name)
            .or_else(|| self.file.get(name))
            .map(String::as_str)
    }

    /// Every name with a value, sorted, without duplicates.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.file.keys().chain(self.env.keys()).cloned().collect();
        names.sort();
        names.dedup();
        names
    }

    /// `Err` with the names in `required` that have no value, in order.
    pub fn check_required(&self, required: &[String]) -> Result<(), Vec<String>> {
        let missing: Vec<String> = required
            .iter()
            .filter(|name| self.get(name).is_none_or(str::is_empty))
            .cloned()
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// The variables a script receives: each secret as `PREPCOOK_SECRET_<NAME>`
    /// and as plain `<NAME>`.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        self.names()
            .into_iter()
            .filter_map(|name| {
                let value = self.get(&name)?.to_string();
                Some([
                    (format!("{SECRET_ENV_PREFIX}{name}"), value.clone()),
                    (name, value),
                ])
            })
            .flatten()
            .collect()
    }

    /// Stores `value` under `name` in the file, rewriting it atomically and
    /// keeping its comments and every other line.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), SecretsError> {
        if !is_valid_name(name) {
            return Err(SecretsError::InvalidName(name.to_string()));
        }
        if value.contains(['\n', '\r']) {
            return Err(SecretsError::InvalidValue);
        }
        let raw = std::fs::read_to_string(&self.path).unwrap_or_default();
        let mut replaced = false;
        let mut lines: Vec<String> = Vec::new();
        for line in raw.lines() {
            if entry_name(line) == Some(name) {
                if !replaced {
                    lines.push(format!("{name}={value}"));
                    replaced = true;
                }
            } else {
                lines.push(line.to_string());
            }
        }
        if !replaced {
            lines.push(format!("{name}={value}"));
        }
        write_atomically(&self.path, &lines)?;
        self.file.insert(name.to_string(), value.to_string());
        register_redaction(value);
        Ok(())
    }

    /// Removes `name` from the file. Returns whether it was there. An
    /// environment override is not touched — it is not the file's to remove.
    pub fn unset(&mut self, name: &str) -> Result<bool, SecretsError> {
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return Ok(false);
        };
        let before = raw.lines().count();
        let lines: Vec<String> = raw
            .lines()
            .filter(|line| entry_name(line) != Some(name))
            .map(str::to_string)
            .collect();
        if lines.len() == before {
            return Ok(false);
        }
        write_atomically(&self.path, &lines)?;
        self.file.remove(name);
        Ok(true)
    }
}

/// The name a `NAME=VALUE` line defines, or `None` for comments, blanks and
/// lines that define nothing.
fn entry_name(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (name, _) = line.split_once('=')?;
    let name = name.trim();
    is_valid_name(name).then_some(name)
}

fn parse(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let name = entry_name(line)?;
            let (_, value) = line.split_once('=')?;
            Some((name.to_string(), unquote(value.trim()).to_string()))
        })
        .collect()
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

/// Writes through a sibling temporary file and a rename, so a crash mid-write
/// leaves the old file rather than half of a new one. Mode 0600 on Unix: the
/// file holds credentials and nobody else on the machine needs to read it.
fn write_atomically(path: &Path, lines: &[String]) -> Result<(), SecretsError> {
    let wrap = |source| SecretsError::Write {
        path: path.to_path_buf(),
        source,
    };
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(wrap)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("secrets"),
        uuid::Uuid::new_v4().simple()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&tmp)?;
        let mut body = lines.join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(wrap)
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// Every value to hide, longest first so a value that contains another is
/// replaced whole. Seeded from the store on first use.
static REDACTIONS: LazyLock<RwLock<Vec<String>>> = LazyLock::new(|| {
    let store = Secrets::load_from(&secrets_path(), std::env::vars());
    let mut values: Vec<String> = store
        .file
        .values()
        .chain(store.env.values())
        .filter(|value| value.len() >= MIN_REDACT_LEN)
        .cloned()
        .collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();
    RwLock::new(values)
});

/// Adds `value` to what [`redact`] hides. Values shorter than six characters
/// are ignored.
pub fn register_redaction(value: &str) {
    if value.len() < MIN_REDACT_LEN {
        return;
    }
    let mut values = REDACTIONS.write().unwrap_or_else(|p| p.into_inner());
    if values.iter().any(|known| known == value) {
        return;
    }
    values.push(value.to_string());
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
}

/// Whether a variable named `name` holds something to redact: its name says
/// password, secret, token or key.
pub fn is_sensitive_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["PASSWORD", "SECRET", "TOKEN", "KEY"]
        .iter()
        .any(|marker| upper.contains(marker))
}

/// `text` with every known secret value replaced by `[redacted]`.
pub fn redact(text: &str) -> String {
    let values = REDACTIONS.read().unwrap_or_else(|p| p.into_inner());
    let mut out = text.to_string();
    for value in values.iter() {
        if out.contains(value.as_str()) {
            out = out.replace(value.as_str(), REDACTED);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_value_round_trips_through_the_file_and_comments_survive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("secrets.env");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# test logins\nOTHER=keep-me-please\n").unwrap();

        let mut store = Secrets::load_from(&path, env(&[]));
        store.set("TEST_USER_PASSWORD", "hunter2hunter2").unwrap();
        store.set("TEST_USER_PASSWORD", "correct-horse").unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("# test logins\n"), "{raw}");
        assert_eq!(raw.matches("TEST_USER_PASSWORD=").count(), 1, "{raw}");

        let reread = Secrets::load_from(&path, env(&[]));
        assert_eq!(reread.get("TEST_USER_PASSWORD"), Some("correct-horse"));
        assert_eq!(reread.get("OTHER"), Some("keep-me-please"));
        assert_eq!(reread.names(), vec!["OTHER", "TEST_USER_PASSWORD"]);

        let mut reread = reread;
        assert!(reread.unset("OTHER").unwrap());
        assert!(!reread.unset("OTHER").unwrap());
        assert_eq!(
            Secrets::load_from(&path, env(&[])).names(),
            vec!["TEST_USER_PASSWORD"]
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn the_environment_overrides_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.env");
        std::fs::write(&path, "API_KEY=\"from-the-file\"\nexport PLAIN=x1y2z3\n").unwrap();
        let store = Secrets::load_from(
            &path,
            env(&[
                ("PREPCOOK_SECRET_API_KEY", "from-the-env"),
                ("PREPCOOK_SECRET_ONLY_ENV", "env-only-value"),
                ("UNRELATED", "ignored"),
            ]),
        );
        assert_eq!(store.get("API_KEY"), Some("from-the-env"));
        assert_eq!(store.get("PLAIN"), Some("x1y2z3"));
        assert_eq!(store.names(), vec!["API_KEY", "ONLY_ENV", "PLAIN"]);
        assert_eq!(
            store.check_required(&["API_KEY".into(), "MISSING".into(), "ONLY_ENV".into()]),
            Err(vec!["MISSING".to_string()])
        );
        let vars = store.env_vars();
        assert!(vars.contains(&("PREPCOOK_SECRET_API_KEY".into(), "from-the-env".into())));
        assert!(vars.contains(&("API_KEY".into(), "from-the-env".into())));
    }

    #[test]
    fn known_values_are_redacted_and_short_ones_are_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.env");
        std::fs::write(&path, "LONG_ONE=s3cr3t-value-xyz\nSHORT=abc\n").unwrap();
        let _ = Secrets::load_from(&path, env(&[]));
        register_redaction("seeded-password-1");

        let text = "login with s3cr3t-value-xyz and seeded-password-1, flag abc";
        let out = redact(text);
        assert_eq!(
            out, "login with [redacted] and [redacted], flag abc",
            "short values stay readable"
        );
        assert!(is_sensitive_name("PREPCOOK_TEST_USER_PASSWORD"));
        assert!(is_sensitive_name("stripe_api_key"));
        assert!(!is_sensitive_name("PREPCOOK_TEST_USER_EMAIL"));
    }

    #[test]
    fn names_are_validated() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Secrets::load_from(&dir.path().join("s.env"), env(&[]));
        assert!(matches!(
            store.set("1BAD", "value"),
            Err(SecretsError::InvalidName(_))
        ));
        assert!(matches!(
            store.set("GOOD", "two\nlines"),
            Err(SecretsError::InvalidValue)
        ));
        assert!(is_valid_name("_OK_1"));
        assert!(!is_valid_name("has-dash"));
    }
}
