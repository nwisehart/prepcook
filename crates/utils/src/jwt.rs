use chrono::{DateTime, Utc};
use jsonwebtoken::dangerous::insecure_decode;
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TokenClaimsError {
    #[error("failed to decode JWT: {0}")]
    Decode(#[from] jsonwebtoken::errors::Error),
    #[error("missing `exp` claim in token")]
    MissingExpiration,
    #[error("invalid `exp` value `{0}`")]
    InvalidExpiration(i64),
    #[error("missing `sub` claim in token")]
    MissingSubject,
    #[error("invalid `sub` value: {0}")]
    InvalidSubject(String),
}

#[derive(Debug, Deserialize)]
struct ExpClaim {
    exp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SubClaim {
    sub: Option<String>,
}

/// Extract the expiration timestamp from a JWT without verifying its signature.
pub fn extract_expiration(token: &str) -> Result<DateTime<Utc>, TokenClaimsError> {
    let data = insecure_decode::<ExpClaim>(token)?;
    let exp = data.claims.exp.ok_or(TokenClaimsError::MissingExpiration)?;
    DateTime::from_timestamp(exp, 0).ok_or(TokenClaimsError::InvalidExpiration(exp))
}

/// Extract the subject (user ID) from a JWT without verifying its signature.
pub fn extract_subject(token: &str) -> Result<Uuid, TokenClaimsError> {
    let data = insecure_decode::<SubClaim>(token)?;
    let sub = data.claims.sub.ok_or(TokenClaimsError::MissingSubject)?;
    Uuid::parse_str(&sub).map_err(|_| TokenClaimsError::InvalidSubject(sub))
}

/// The `aud` claim of an owner token: a token the local server signs with the
/// board's own JWT secret to act as the board's single owner. It carries no
/// user or session; the board resolves both itself.
pub const OWNER_TOKEN_AUDIENCE: &str = "prepcook-owner";

/// The longest lifetime (`exp - iat`) the board accepts on an owner token.
pub const OWNER_TOKEN_MAX_TTL_SECONDS: i64 = 60 * 60;

/// The lifetime the local server gives the owner tokens it mints. Short, since
/// minting is a local HMAC and costs nothing to repeat.
pub const OWNER_TOKEN_TTL_SECONDS: i64 = 10 * 60;
const _: () = assert!(OWNER_TOKEN_TTL_SECONDS <= OWNER_TOKEN_MAX_TTL_SECONDS);

/// Clock skew tolerated on `exp` when an owner token is verified.
const OWNER_TOKEN_LEEWAY_SECONDS: u64 = 60;

#[derive(Debug, Error)]
pub enum OwnerTokenError {
    /// The secret is not standard base64, which is how the board reads its
    /// `PREPCOOK_JWT_SECRET`.
    #[error("the board secret is not valid base64")]
    InvalidSecret,
    #[error("owner token expired")]
    Expired,
    /// `exp - iat` is longer than [`OWNER_TOKEN_MAX_TTL_SECONDS`], or `iat`
    /// is in the future.
    #[error("owner token lifetime is out of range")]
    InvalidLifetime,
    #[error("invalid owner token: {0}")]
    Invalid(jsonwebtoken::errors::Error),
}

#[derive(Debug, serde::Serialize, Deserialize)]
struct OwnerTokenClaims {
    aud: String,
    iat: i64,
    exp: i64,
}

fn owner_validation() -> jsonwebtoken::Validation {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    validation.validate_nbf = false;
    validation.set_audience(&[OWNER_TOKEN_AUDIENCE]);
    validation.required_spec_claims = ["exp", "aud"].iter().map(|c| c.to_string()).collect();
    validation.leeway = OWNER_TOKEN_LEEWAY_SECONDS;
    validation
}

/// Mints an owner token valid from `issued_at` for `ttl_seconds`, signed
/// (HS256) with `secret`, the board's `PREPCOOK_JWT_SECRET` as written in its
/// `.env` (standard base64).
pub fn mint_owner_token(
    secret: &str,
    issued_at: DateTime<Utc>,
    ttl_seconds: i64,
) -> Result<String, OwnerTokenError> {
    let key = jsonwebtoken::EncodingKey::from_base64_secret(secret)
        .map_err(|_| OwnerTokenError::InvalidSecret)?;
    let claims = OwnerTokenClaims {
        aud: OWNER_TOKEN_AUDIENCE.to_string(),
        iat: issued_at.timestamp(),
        exp: issued_at.timestamp() + ttl_seconds,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &key,
    )
    .map_err(OwnerTokenError::Invalid)
}

/// Verifies `token` as an owner token signed with `secret`. Returns its
/// expiry, or `Ok(None)` when the token is validly signed but meant for
/// another audience (a session access token), so the caller can fall back to
/// its session path. A bad signature, an expired token or a lifetime over
/// [`OWNER_TOKEN_MAX_TTL_SECONDS`] is an error.
pub fn verify_owner_token(
    secret: &str,
    token: &str,
) -> Result<Option<DateTime<Utc>>, OwnerTokenError> {
    use jsonwebtoken::errors::ErrorKind;

    let key = jsonwebtoken::DecodingKey::from_base64_secret(secret)
        .map_err(|_| OwnerTokenError::InvalidSecret)?;
    let claims = match jsonwebtoken::decode::<OwnerTokenClaims>(token, &key, &owner_validation()) {
        Ok(data) => data.claims,
        Err(error) => {
            return match error.kind() {
                ErrorKind::InvalidAudience => Ok(None),
                ErrorKind::ExpiredSignature => Err(OwnerTokenError::Expired),
                _ => Err(OwnerTokenError::Invalid(error)),
            };
        }
    };

    let now = Utc::now().timestamp();
    let lifetime = claims.exp - claims.iat;
    if lifetime <= 0
        || lifetime > OWNER_TOKEN_MAX_TTL_SECONDS
        || claims.iat > now + OWNER_TOKEN_LEEWAY_SECONDS as i64
    {
        return Err(OwnerTokenError::InvalidLifetime);
    }

    DateTime::from_timestamp(claims.exp, 0)
        .map(Some)
        .ok_or(OwnerTokenError::InvalidLifetime)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    // Standard base64 of 32 bytes, the shape `prepcook up` generates.
    const SECRET: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const OTHER_SECRET: &str = "HxweHRwbGhkYFxYVFBMSERAPDg0MCwoJCAcGBQQDAgE=";

    #[test]
    fn a_minted_owner_token_verifies_with_the_same_secret() {
        let now = Utc::now();
        let token = mint_owner_token(SECRET, now, OWNER_TOKEN_TTL_SECONDS).unwrap();
        let expires_at = verify_owner_token(SECRET, &token).unwrap().unwrap();
        assert_eq!(
            expires_at.timestamp(),
            now.timestamp() + OWNER_TOKEN_TTL_SECONDS
        );
        assert_eq!(extract_expiration(&token).unwrap(), expires_at);
    }

    #[test]
    fn an_owner_token_signed_with_another_secret_is_rejected() {
        let token = mint_owner_token(OTHER_SECRET, Utc::now(), 60).unwrap();
        assert!(matches!(
            verify_owner_token(SECRET, &token),
            Err(OwnerTokenError::Invalid(_))
        ));
    }

    #[test]
    fn an_expired_owner_token_is_rejected() {
        let token = mint_owner_token(SECRET, Utc::now() - Duration::hours(2), 60).unwrap();
        assert!(matches!(
            verify_owner_token(SECRET, &token),
            Err(OwnerTokenError::Expired)
        ));
    }

    #[test]
    fn an_owner_token_living_longer_than_an_hour_is_rejected() {
        let token = mint_owner_token(SECRET, Utc::now(), OWNER_TOKEN_MAX_TTL_SECONDS + 60).unwrap();
        assert!(matches!(
            verify_owner_token(SECRET, &token),
            Err(OwnerTokenError::InvalidLifetime)
        ));
    }

    #[test]
    fn a_token_for_another_audience_is_not_an_owner_token() {
        let key = jsonwebtoken::EncodingKey::from_base64_secret(SECRET).unwrap();
        let now = Utc::now().timestamp();
        let claims = serde_json::json!({
            "sub": uuid::Uuid::new_v4(),
            "session_id": uuid::Uuid::new_v4(),
            "iat": now,
            "exp": now + 120,
            "aud": "access",
        });
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &key,
        )
        .unwrap();
        assert!(verify_owner_token(SECRET, &token).unwrap().is_none());
    }

    #[test]
    fn a_secret_that_is_not_base64_is_refused() {
        assert!(matches!(
            mint_owner_token("not base64!", Utc::now(), 60),
            Err(OwnerTokenError::InvalidSecret)
        ));
    }
}
