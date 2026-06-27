//! JWT authentication signed with an x509 key pair, plus sliding-window rate limiting.
//!
//! The backend holds the RSA **private** key and signs JWTs (RS256); it verifies
//! incoming tokens with the **public** key extracted from the x509 certificate.
//! Tokens carry a [`Claims`] payload (`sub`, `iss`, `iat`, `exp`, `permissions`).
//! The private key and minted tokens are never logged.

use crate::config::{DEFAULT_ISSUER, DEFAULT_TOKEN_TTL_SECS};
use crate::error::ApiError;
use crate::models::Permission;
use dashmap::DashMap;
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode,
    errors::ErrorKind as JwtErrorKind,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Default JWT signing algorithm (RSA + SHA-256, x509 key pair).
const JWT_ALGORITHM: Algorithm = Algorithm::RS256;

/// Clock-skew leeway in seconds applied to `exp` / `iat` validation.
const CLOCK_SKEW_LEEWAY_SECS: u64 = 60;

/// Sliding rate-limit window length in milliseconds (60s, per issue #48).
const RATE_LIMIT_WINDOW_MS: u64 = 60_000;

/// Hard upper bound on the number of distinct keys the [`RateLimiter`] tracks at
/// once (issue #48: bound the window map against memory-exhaustion DoS from many
/// distinct subjects / peer IPs). When the map is full, a sweep of fully-expired
/// entries is attempted before a brand-new key is admitted; if it is still full,
/// the new key is rejected (fail-closed) rather than growing the map unbounded.
const MAX_TRACKED_KEYS: usize = 100_000;

/// Built-in, clearly-labeled DEV/TEST private key (RSA, PKCS#8 PEM).
///
/// Used only by [`JwtAuth::dev`] for local `cargo run`, unit tests, and the
/// no-config fallback. Production keys come from the configured PEM paths and
/// override this at startup. NOT a secret for production use.
const DEV_PRIVATE_KEY_PEM: &[u8] = include_bytes!("../tests/fixtures/dev-private-key.pem");

/// Built-in, clearly-labeled DEV/TEST x509 certificate (holds the public key).
const DEV_CERT_PEM: &[u8] = include_bytes!("../tests/fixtures/dev-cert.pem");

/// Returns the current Unix time in whole seconds.
#[inline]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Returns the current Unix time in milliseconds.
#[inline]
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Compares a `candidate` against the server-configured `secret` so that the
/// running time depends only on the (fixed) length of `secret` — never on the
/// candidate's length or contents.
///
/// A length mismatch is folded into the difference accumulator instead of
/// short-circuiting, and the loop always iterates over every byte of `secret`
/// (reading `candidate.get(i)`, treating out-of-range bytes as `0`). Returns
/// true only when the two slices are byte-for-byte equal, including equal length.
#[must_use]
pub fn constant_time_eq(secret: &[u8], candidate: &[u8]) -> bool {
    // Non-zero when the lengths differ, folded in so a wrong length can never
    // cause an early return (and so equal-but-shorter candidates still fail).
    let mut diff: u8 = if secret.len() == candidate.len() {
        0
    } else {
        1
    };
    for (i, expected) in secret.iter().enumerate() {
        let actual = candidate.get(i).copied().unwrap_or(0);
        diff |= expected ^ actual;
    }
    diff == 0
}

/// JWT claims carried by every authenticated request.
///
/// `iat` / `exp` are seconds since the Unix epoch; `permissions` map directly to
/// the [`Permission`] enum (Admin implies all — see [`Claims::has_permission`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the token identity, used as the rate-limit key.
    pub sub: String,
    /// Issuer (the `iss` claim).
    pub iss: String,
    /// Issued-at timestamp (seconds since the Unix epoch).
    pub iat: u64,
    /// Expiration timestamp (seconds since the Unix epoch).
    pub exp: u64,
    /// Permissions granted to this token.
    pub permissions: Vec<Permission>,
}

impl Claims {
    /// Returns true if the claims grant `permission` (Admin implies all).
    #[must_use]
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission) || self.permissions.contains(&Permission::Admin)
    }
}

/// Rate limiter using a sliding-window algorithm, keyed by an arbitrary string
/// (the JWT `sub`, or a synthetic IP-based key for the unauthenticated token
/// endpoint).
#[derive(Debug, Default)]
pub struct RateLimiter {
    /// Request timestamps (ms) per key.
    windows: DashMap<String, VecDeque<u64>>,
}

impl RateLimiter {
    /// Creates a new rate limiter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
        }
    }

    /// Records a request for `key` and returns true if it is within `limit`
    /// requests over the sliding 60-second window.
    ///
    /// Existing keys take a lock-free fast path. A brand-new key is admitted only
    /// while the map is under [`MAX_TRACKED_KEYS`]; at the cap a sweep of
    /// fully-expired entries is attempted first, and if the map is still full the
    /// request is rejected so the map can never grow without bound.
    pub fn check_and_record(&self, key: &str, limit: u32) -> bool {
        let now = now_millis();
        let window_start = now.saturating_sub(RATE_LIMIT_WINDOW_MS);

        // Fast path: the key already has a bucket. `get_mut` holds the shard lock
        // only for the duration of this block (no `.await` inside).
        if let Some(mut entry) = self.windows.get_mut(key) {
            return Self::prune_and_record(entry.value_mut(), now, window_start, limit);
        }

        // New key: enforce the cap before allocating a bucket.
        if self.windows.len() >= MAX_TRACKED_KEYS {
            let evicted = self.sweep_expired();
            if self.windows.len() >= MAX_TRACKED_KEYS {
                tracing::warn!(
                    tracked = self.windows.len(),
                    evicted,
                    "rate-limit window map at capacity; rejecting request for a new key"
                );
                return false;
            }
        }

        let mut entry = self.windows.entry(key.to_string()).or_default();
        Self::prune_and_record(entry.value_mut(), now, window_start, limit)
    }

    /// Drops timestamps older than `window_start` from `window`, then records
    /// `now` if doing so keeps the count within `limit`.
    fn prune_and_record(
        window: &mut VecDeque<u64>,
        now: u64,
        window_start: u64,
        limit: u32,
    ) -> bool {
        while let Some(&front) = window.front() {
            if front < window_start {
                window.pop_front();
            } else {
                break;
            }
        }

        if window.len() < limit as usize {
            window.push_back(now);
            true
        } else {
            false
        }
    }

    /// Reaps entries whose entire window has expired, returning the number of
    /// keys removed.
    ///
    /// Lazy pruning alone never removes a key (a recorded request always leaves
    /// the bucket non-empty), so the map would otherwise retain one entry per
    /// distinct key seen. This sweep — driven periodically by a background task
    /// and on-demand when the cap is hit — reaps idle buckets back toward
    /// baseline. It is lock-free across shards (`DashMap::retain`) and does not
    /// hold any guard across an `.await`.
    pub fn sweep_expired(&self) -> usize {
        let now = now_millis();
        let window_start = now.saturating_sub(RATE_LIMIT_WINDOW_MS);
        let before = self.windows.len();
        self.windows.retain(|_key, window| {
            while let Some(&front) = window.front() {
                if front < window_start {
                    window.pop_front();
                } else {
                    break;
                }
            }
            !window.is_empty()
        });
        before.saturating_sub(self.windows.len())
    }

    /// Returns the number of keys currently tracked (for tests / observability).
    #[must_use]
    pub fn tracked_keys(&self) -> usize {
        self.windows.len()
    }

    /// Clears rate-limit data for `key`.
    pub fn clear(&self, key: &str) {
        self.windows.remove(key);
    }
}

/// JWT authentication core: signing/verification keys + the rate limiter.
///
/// Holds the RSA private signing key (never logged) and the public verification
/// key, plus the configured issuer and default token TTL.
pub struct JwtAuth {
    /// RSA private key used to sign tokens.
    encoding_key: EncodingKey,
    /// Public key (from the x509 cert) used to verify tokens.
    decoding_key: DecodingKey,
    /// Signing algorithm.
    algorithm: Algorithm,
    /// Token issuer (`iss` claim).
    issuer: String,
    /// Default token lifetime in seconds.
    default_ttl_secs: u64,
    /// Verification rules (algorithm, issuer, leeway, `exp`).
    validation: Validation,
    /// Sliding-window rate limiter keyed by `sub`.
    rate_limiter: RateLimiter,
}

impl JwtAuth {
    /// Builds a JWT auth core from in-memory PEM material.
    ///
    /// `private_key_pem` is a PEM-encoded RSA private key; `cert_pem` is a
    /// PEM-encoded x509 certificate (the public key is extracted from it).
    ///
    /// # Errors
    /// Returns [`ApiError::Internal`] if either PEM cannot be parsed. The
    /// underlying parse error is logged at `error` level but never returned to
    /// the client, and the key material itself is never logged.
    pub fn from_rsa_pem(
        private_key_pem: &[u8],
        cert_pem: &[u8],
        issuer: String,
        default_ttl_secs: u64,
    ) -> Result<Self, ApiError> {
        let encoding_key = EncodingKey::from_rsa_pem(private_key_pem).map_err(|e| {
            tracing::error!(error = %e, "failed to load JWT private signing key");
            ApiError::Internal("failed to load auth private key".to_string())
        })?;
        let decoding_key = DecodingKey::from_rsa_pem(cert_pem).map_err(|e| {
            tracing::error!(error = %e, "failed to load JWT public verification key");
            ApiError::Internal("failed to load auth certificate".to_string())
        })?;
        Ok(Self::assemble(
            encoding_key,
            decoding_key,
            JWT_ALGORITHM,
            issuer,
            default_ttl_secs,
        ))
    }

    /// Builds a JWT auth core from PEM files on disk.
    ///
    /// # Errors
    /// Returns [`ApiError::Internal`] if a file cannot be read or parsed. Paths
    /// are logged; key contents are not.
    pub fn from_paths(
        private_key_path: &Path,
        cert_path: &Path,
        issuer: String,
        default_ttl_secs: u64,
    ) -> Result<Self, ApiError> {
        let private_key_pem = std::fs::read(private_key_path).map_err(|e| {
            tracing::error!(path = %private_key_path.display(), error = %e, "failed to read auth private key file");
            ApiError::Internal("failed to read auth private key file".to_string())
        })?;
        let cert_pem = std::fs::read(cert_path).map_err(|e| {
            tracing::error!(path = %cert_path.display(), error = %e, "failed to read auth certificate file");
            ApiError::Internal("failed to read auth certificate file".to_string())
        })?;
        Self::from_rsa_pem(&private_key_pem, &cert_pem, issuer, default_ttl_secs)
    }

    /// Builds a DEV/TEST auth core from the built-in dev key pair.
    ///
    /// Used for local `cargo run`, unit tests, and the no-config fallback. The
    /// embedded dev key is an RSA build-time fixture; parsing it cannot fail at
    /// runtime. Never use for production signing — production keys come from the
    /// configured paths, and startup hard-fails when the dev key is used in
    /// production without an explicit override.
    ///
    /// # Panics
    /// Panics only if the compiled-in dev key pair cannot be parsed, which is a
    /// build-fixture bug rather than a runtime condition.
    #[must_use]
    pub fn dev() -> Self {
        Self::from_rsa_pem(
            DEV_PRIVATE_KEY_PEM,
            DEV_CERT_PEM,
            DEFAULT_ISSUER.to_string(),
            DEFAULT_TOKEN_TTL_SECS,
        )
        .expect("embedded dev key pair must parse")
    }

    /// Assembles a [`JwtAuth`] from prepared keys and configuration.
    fn assemble(
        encoding_key: EncodingKey,
        decoding_key: DecodingKey,
        algorithm: Algorithm,
        issuer: String,
        default_ttl_secs: u64,
    ) -> Self {
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(std::slice::from_ref(&issuer));
        validation.leeway = CLOCK_SKEW_LEEWAY_SECS;
        validation.validate_exp = true;
        Self {
            encoding_key,
            decoding_key,
            algorithm,
            issuer,
            default_ttl_secs,
            validation,
            rate_limiter: RateLimiter::new(),
        }
    }

    /// Returns the configured default token TTL in seconds.
    #[must_use]
    pub fn default_ttl_secs(&self) -> u64 {
        self.default_ttl_secs
    }

    /// Mints and signs a JWT for `permissions` valid for `ttl_secs` seconds.
    ///
    /// Returns the signed token and its expiration (`exp`, Unix seconds). A fresh
    /// random `sub` (UUID) is assigned to each token.
    ///
    /// # Errors
    /// Returns [`ApiError::InvalidRequest`] if `ttl_secs` overflows the clock, or
    /// [`ApiError::Internal`] if signing fails. The token is never logged.
    pub fn mint_token(
        &self,
        permissions: Vec<Permission>,
        ttl_secs: u64,
    ) -> Result<(String, u64), ApiError> {
        let now = now_secs();
        let exp = now
            .checked_add(ttl_secs)
            .ok_or_else(|| ApiError::InvalidRequest("token ttl overflow".to_string()))?;

        let claims = Claims {
            sub: Uuid::new_v4().to_string(),
            iss: self.issuer.clone(),
            iat: now,
            exp,
            permissions,
        };

        let header = Header::new(self.algorithm);
        let token = encode(&header, &claims, &self.encoding_key).map_err(|e| {
            tracing::error!(error = %e, "failed to sign JWT");
            ApiError::Internal("failed to sign token".to_string())
        })?;

        Ok((token, exp))
    }

    /// Verifies a JWT against the public key, issuer, and expiry, returning its
    /// claims.
    ///
    /// # Errors
    /// Returns [`ApiError::Unauthorized`] if the token is malformed, signed by
    /// the wrong key, issued by an unexpected issuer, or expired. The token is
    /// never logged; only the (non-secret) failure kind is recorded at `debug`.
    pub fn verify_token(&self, token: &str) -> Result<Claims, ApiError> {
        match decode::<Claims>(token, &self.decoding_key, &self.validation) {
            Ok(data) => Ok(data.claims),
            Err(e) => {
                let reason = match e.kind() {
                    JwtErrorKind::ExpiredSignature => "expired token",
                    JwtErrorKind::InvalidIssuer => "invalid token issuer",
                    JwtErrorKind::InvalidSignature => "invalid token signature",
                    _ => "invalid token",
                };
                tracing::debug!(kind = ?e.kind(), "JWT verification failed");
                Err(ApiError::Unauthorized(reason.to_string()))
            }
        }
    }

    /// Records a request for `key` (the JWT `sub`) and returns true if it is
    /// within `limit` requests over the sliding window.
    pub fn check_rate_limit(&self, key: &str, limit: u32) -> bool {
        self.rate_limiter.check_and_record(key, limit)
    }

    /// Reaps fully-expired rate-limit buckets, returning the number removed.
    ///
    /// Intended to be called periodically by a background sweep task so idle
    /// buckets do not accumulate (issue #48).
    pub fn sweep_rate_limit_windows(&self) -> usize {
        self.rate_limiter.sweep_expired()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_auth() -> JwtAuth {
        JwtAuth::from_rsa_pem(
            DEV_PRIVATE_KEY_PEM,
            DEV_CERT_PEM,
            "test-issuer".to_string(),
            3600,
        )
        .expect("dev key pair must load")
    }

    #[test]
    fn test_sign_verify_round_trip() {
        let auth = dev_auth();
        let (token, exp) = auth
            .mint_token(vec![Permission::Read, Permission::Trade], 3600)
            .expect("mint");
        assert!(exp > now_secs());

        let claims = auth.verify_token(&token).expect("verify");
        assert_eq!(claims.iss, "test-issuer");
        assert!(!claims.sub.is_empty());
        assert!(claims.has_permission(Permission::Read));
        assert!(claims.has_permission(Permission::Trade));
        assert!(!claims.has_permission(Permission::Admin));
    }

    #[test]
    fn test_expired_token_rejected() {
        let auth = dev_auth();
        // TTL of 0 puts `exp` at `now`; with the leeway it would still pass, so
        // craft claims well in the past and sign them directly.
        let past = now_secs().saturating_sub(10_000);
        let claims = Claims {
            sub: "expired".to_string(),
            iss: "test-issuer".to_string(),
            iat: past.saturating_sub(100),
            exp: past,
            permissions: vec![Permission::Read],
        };
        let token = encode(&Header::new(JWT_ALGORITHM), &claims, &auth.encoding_key).expect("sign");

        let err = auth.verify_token(&token).expect_err("must be expired");
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn test_wrong_key_rejected() {
        // Sign with an unrelated HS256 secret so the signature cannot match the
        // RSA public key embedded in the dev certificate.
        let attacker = JwtAuth::assemble(
            EncodingKey::from_secret(b"attacker-secret"),
            DecodingKey::from_secret(b"attacker-secret"),
            Algorithm::HS256,
            "test-issuer".to_string(),
            3600,
        );
        let (token, _) = attacker
            .mint_token(vec![Permission::Admin], 3600)
            .expect("mint");

        let verifier = dev_auth();
        let err = verifier
            .verify_token(&token)
            .expect_err("wrong-key token must be rejected");
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn test_permission_mapping_from_claims() {
        // Claims permissions deserialize from the lowercase wire form.
        let json = r#"{"sub":"s","iss":"i","iat":1,"exp":2,"permissions":["read","trade"]}"#;
        let claims: Claims = serde_json::from_str(json).expect("deserialize");
        assert_eq!(claims.permissions.len(), 2);
        assert!(claims.has_permission(Permission::Read));
        assert!(claims.has_permission(Permission::Trade));
        assert!(!claims.has_permission(Permission::Admin));
    }

    #[test]
    fn test_admin_implies_all() {
        let claims = Claims {
            sub: "s".to_string(),
            iss: "i".to_string(),
            iat: 1,
            exp: 2,
            permissions: vec![Permission::Admin],
        };
        assert!(claims.has_permission(Permission::Read));
        assert!(claims.has_permission(Permission::Trade));
        assert!(claims.has_permission(Permission::Admin));
    }

    #[test]
    fn test_dev_auth_round_trip() {
        let auth = JwtAuth::dev();
        let (token, _) = auth.mint_token(vec![Permission::Read], 60).expect("mint");
        let claims = auth.verify_token(&token).expect("verify");
        assert!(claims.has_permission(Permission::Read));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secres"));
        assert!(!constant_time_eq(b"secret", b"secre"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new();
        for _ in 0..10 {
            assert!(limiter.check_and_record("sub-1", 10));
        }
        assert!(!limiter.check_and_record("sub-1", 10));
        // A different subject has its own window.
        assert!(limiter.check_and_record("sub-2", 10));
    }

    #[test]
    fn test_rate_limiter_boundary_nth_allowed_n_plus_one_rejected() {
        let limiter = RateLimiter::new();
        // Exactly `limit` requests are admitted within the window.
        for i in 0..5 {
            assert!(
                limiter.check_and_record("boundary", 5),
                "request {i} within the limit must be admitted"
            );
        }
        // The (limit + 1)-th request in the same window is rejected.
        assert!(!limiter.check_and_record("boundary", 5));
    }

    #[test]
    fn test_sweep_evicts_fully_expired_entries() {
        let limiter = RateLimiter::new();

        // Seed a key with a timestamp older than the full window so it counts as
        // fully expired (simulate an idle bucket from a past burst).
        let stale = now_millis().saturating_sub(RATE_LIMIT_WINDOW_MS + 1);
        {
            let mut entry = limiter.windows.entry("stale".to_string()).or_default();
            entry.value_mut().push_back(stale);
        }
        // And a key that is still fresh.
        assert!(limiter.check_and_record("fresh", 10));
        assert_eq!(limiter.tracked_keys(), 2);

        let removed = limiter.sweep_expired();
        assert_eq!(removed, 1, "only the fully-expired key is reaped");
        assert_eq!(limiter.tracked_keys(), 1);
        // The fresh key survives.
        assert!(limiter.check_and_record("fresh", 10));
    }

    #[test]
    fn test_sweep_reaps_burst_from_many_keys_back_to_baseline() {
        let limiter = RateLimiter::new();

        // Simulate a burst from many distinct keys (e.g. many peer IPs), all with
        // timestamps already outside the window.
        let stale = now_millis().saturating_sub(RATE_LIMIT_WINDOW_MS + 1);
        for i in 0..1_000 {
            let mut entry = limiter.windows.entry(format!("ip-{i}")).or_default();
            entry.value_mut().push_back(stale);
        }
        assert_eq!(limiter.tracked_keys(), 1_000);

        // After the idle window, a sweep reaps the map back to baseline (empty).
        let removed = limiter.sweep_expired();
        assert_eq!(removed, 1_000);
        assert_eq!(limiter.tracked_keys(), 0);
    }

    #[test]
    fn test_map_does_not_grow_unbounded_across_expired_keys() {
        let limiter = RateLimiter::new();

        // Repeatedly record one-shot keys whose timestamps are immediately stale,
        // sweeping between rounds. The map must not accumulate across rounds.
        let stale = now_millis().saturating_sub(RATE_LIMIT_WINDOW_MS + 1);
        for round in 0..5 {
            for i in 0..200 {
                let mut entry = limiter.windows.entry(format!("r{round}-k{i}")).or_default();
                entry.value_mut().push_back(stale);
            }
            limiter.sweep_expired();
            assert_eq!(
                limiter.tracked_keys(),
                0,
                "expired keys from round {round} must be reaped"
            );
        }
    }

    #[test]
    fn test_per_key_isolation_within_window() {
        let limiter = RateLimiter::new();
        // Exhaust key A.
        for _ in 0..3 {
            assert!(limiter.check_and_record("A", 3));
        }
        assert!(!limiter.check_and_record("A", 3));
        // Key B is unaffected and still has its full allowance.
        for _ in 0..3 {
            assert!(limiter.check_and_record("B", 3));
        }
        assert!(!limiter.check_and_record("B", 3));
    }
}
