use std::env;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use secrecy::SecretString;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct RemoteServerConfig {
    pub database_url: String,
    pub listen_addr: String,
    pub server_public_base_url: Option<String>,
    pub auth: AuthConfig,
    pub refresh_token_overlap_secs: i64,
    pub electric_url: String,
    pub electric_secret: Option<SecretString>,
    pub electric_role_password: Option<SecretString>,
    pub electric_publication_names: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("environment variable `{0}` is not set")]
    MissingVar(&'static str),
    #[error("invalid value for environment variable `{0}`")]
    InvalidVar(&'static str),
}

impl RemoteServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = env::var("SERVER_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .map_err(|_| ConfigError::MissingVar("SERVER_DATABASE_URL"))?;

        let listen_addr =
            env::var("SERVER_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8081".to_string());

        let server_public_base_url = env::var("SERVER_PUBLIC_BASE_URL").ok();

        let auth = AuthConfig::from_env()?;

        let refresh_token_overlap_secs = env::var("REFRESH_TOKEN_OVERLAP_SECS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value >= 0 && *value <= 300)
            .unwrap_or(60);

        let electric_url =
            env::var("ELECTRIC_URL").map_err(|_| ConfigError::MissingVar("ELECTRIC_URL"))?;

        let electric_secret = env::var("ELECTRIC_SECRET")
            .map(|s| SecretString::new(s.into()))
            .ok();

        let electric_role_password = env::var("ELECTRIC_ROLE_PASSWORD")
            .ok()
            .map(|s| SecretString::new(s.into()));
        let electric_publication_names = match env::var("ELECTRIC_PUBLICATION_NAMES") {
            Ok(value) => parse_publication_names(&value)?,
            Err(_) => Vec::new(),
        };

        Ok(Self {
            database_url,
            listen_addr,
            server_public_base_url,
            auth,
            refresh_token_overlap_secs,
            electric_url,
            electric_secret,
            electric_role_password,
            electric_publication_names,
        })
    }
}

fn parse_publication_names(value: &str) -> Result<Vec<String>, ConfigError> {
    let mut names = Vec::new();

    for raw in value.split(',') {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        if !is_valid_identifier(name) {
            return Err(ConfigError::InvalidVar("ELECTRIC_PUBLICATION_NAMES"));
        }
        names.push(name.to_string());
    }

    Ok(names)
}

fn is_valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The board's token signing. `PREPCOOK_JWT_SECRET` signs agent sessions'
/// tokens and verifies owner tokens, which the local server signs with the
/// same value (its `PREPCOOK_BOARD_SECRET`).
#[derive(Debug, Clone)]
pub struct AuthConfig {
    jwt_secret: SecretString,
}

impl AuthConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let jwt_secret = env::var("PREPCOOK_JWT_SECRET")
            .map_err(|_| ConfigError::MissingVar("PREPCOOK_JWT_SECRET"))?;
        validate_jwt_secret(&jwt_secret)?;
        Ok(Self {
            jwt_secret: SecretString::new(jwt_secret.into()),
        })
    }

    pub fn jwt_secret(&self) -> &SecretString {
        &self.jwt_secret
    }
}

fn validate_jwt_secret(secret: &str) -> Result<(), ConfigError> {
    let decoded = BASE64_STANDARD
        .decode(secret.as_bytes())
        .map_err(|_| ConfigError::InvalidVar("PREPCOOK_JWT_SECRET"))?;

    if decoded.len() < 32 {
        return Err(ConfigError::InvalidVar("PREPCOOK_JWT_SECRET"));
    }

    Ok(())
}

#[cfg(test)]
impl RemoteServerConfig {
    /// A configuration for tests that build an `AppState` by hand.
    pub(crate) fn for_tests(database_url: &str, jwt_secret: &str) -> Self {
        Self {
            database_url: database_url.to_string(),
            listen_addr: "127.0.0.1:0".to_string(),
            server_public_base_url: None,
            auth: AuthConfig {
                jwt_secret: SecretString::new(jwt_secret.into()),
            },
            refresh_token_overlap_secs: 60,
            electric_url: "http://127.0.0.1:1".to_string(),
            electric_secret: None,
            electric_role_password: None,
            electric_publication_names: Vec::new(),
        }
    }
}
