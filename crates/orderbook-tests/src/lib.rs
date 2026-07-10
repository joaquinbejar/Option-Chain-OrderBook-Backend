//! Integration tests for the Option Chain OrderBook API.
//!
//! These tests require the API server to be running. Configure the server URL
//! via the `API_BASE_URL` environment variable (default: `http://localhost:8080`).
//!
//! # Authentication
//!
//! Since JWT auth landed, every endpoint except `/health` and
//! `POST /api/v1/auth/token` requires a Bearer token. Tests mint tokens against a
//! running server using the operator bootstrap secret (`AUTH_BOOTSTRAP_SECRET`).
//! Token issuance is itself rate-limited per client IP, so issued tokens are
//! cached on disk (keyed by the permission set and the target server) and reused
//! across every test binary in a `cargo test` run — see [`obtain_token`].
//!
//! # Expiration handling
//!
//! Since the #110 fix, an 8-digit `YYYYMMDD` segment always resolves to the
//! calendar-date expiration on every path (placement, read, modify, cancel,
//! bulk), so the value a test sends is also the value the server formats
//! back. [`setup_underlying`] / [`formatted_expiration`] are retained as the
//! canonical way to read the server-reported key (for `Days`-form
//! expirations the formatted key is the computed date, not the day count),
//! but for [`TEST_EXPIRATION`] the formatted value now equals the input.

use orderbook_client::{ClientConfig, Error, OrderbookClient, Permission, TokenRequest};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Number of distinct `Admin` tokens (subjects) shared across the whole suite.
///
/// Two rate limits are in tension: the token endpoint issues at most 10 tokens
/// per 60s per client IP, while each token's subject is limited to 100 requests
/// per 60s. A single shared token would exhaust the per-subject budget; a token
/// per test would exhaust the issuance budget. A small pool of cross-process
/// cached tokens balances both: cargo runs the test binaries sequentially and
/// mints are serialized, so exactly `ADMIN_POOL_SIZE` admin tokens (+1 read) are
/// ever issued — well under 10 per 60s — while request load spreads across
/// `ADMIN_POOL_SIZE` subjects (well under 100 each per subject).
const ADMIN_POOL_SIZE: usize = 8;

/// Canonical expiration used for order PLACEMENT in tests. Sent verbatim to
/// `add_order` / market / bulk-write paths, which resolve the book by parsing it.
pub const TEST_EXPIRATION: &str = "20251231";

/// Canonical strike used across tests (in cents).
pub const TEST_STRIKE: u64 = 10000;

/// Gets the API base URL from environment or uses default.
#[must_use]
pub fn get_api_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Gets the operator bootstrap secret from the environment.
///
/// Mirrors the server's `AUTH_BOOTSTRAP_SECRET`; integration tests use it to mint
/// JWTs against a running server.
#[must_use]
pub fn get_bootstrap_secret() -> String {
    std::env::var("AUTH_BOOTSTRAP_SECRET").unwrap_or_else(|_| "test-bootstrap-secret".to_string())
}

/// Creates an unauthenticated test client (carries no token).
///
/// Useful for `/health` and for asserting `401` on protected endpoints.
///
/// # Errors
/// Returns error if client creation fails.
pub fn create_test_client() -> Result<OrderbookClient, Error> {
    OrderbookClient::new(ClientConfig {
        base_url: get_api_url(),
        timeout: Duration::from_secs(10),
        token: None,
    })
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn permission_names(permissions: &[Permission]) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = permissions
        .iter()
        .map(|p| match p {
            Permission::Read => "read",
            Permission::Trade => "trade",
            Permission::Admin => "admin",
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Derives a stable cache key for a permission set + pool slot + target server.
fn token_cache_key(permissions: &[Permission], slot: usize) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    get_api_url().hash(&mut hasher);
    get_bootstrap_secret().hash(&mut hasher);
    permission_names(permissions).hash(&mut hasher);
    slot.hash(&mut hasher);
    format!("obtest-token-{:016x}", hasher.finish())
}

fn token_cache_path(key: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(key)
}

/// Reads a cached token from disk. The cache file stores `expiry_secs\ntoken`;
/// a token is returned only if it is present and stays valid for at least two
/// more minutes.
fn read_cached_token(key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(token_cache_path(key)).ok()?;
    let (expiry, token) = contents.split_once('\n')?;
    let expiry: u64 = expiry.trim().parse().ok()?;
    if expiry <= now_secs() + 120 {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Atomically writes a token to the on-disk cache (temp file + rename).
fn write_cached_token(key: &str, token: &str, expiry_secs: u64) {
    let path = token_cache_path(key);
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&tmp, format!("{expiry_secs}\n{token}")).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// In-process cache: permission-key -> (expiry_secs, token).
fn inproc_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, (u64, String)>> {
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, (u64, String)>>> =
        OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Serializes minting so at most one issuance happens per permission set within a
/// binary even when many tests start concurrently.
static MINT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn inproc_get(key: &str) -> Option<String> {
    let guard = inproc_cache().lock().ok()?;
    let (expiry, token) = guard.get(key)?;
    if *expiry <= now_secs() + 120 {
        None
    } else {
        Some(token.clone())
    }
}

fn inproc_put(key: &str, expiry: u64, token: &str) {
    if let Ok(mut guard) = inproc_cache().lock() {
        guard.insert(key.to_string(), (expiry, token.to_string()));
    }
}

/// Obtains a JWT for the given permission set and pool slot.
///
/// Tokens are cached in-process and on disk (keyed by the permission set, slot,
/// and target server) and reused across the whole `cargo test` run, so the token
/// endpoint's per-IP issuance rate limit is not exhausted. The bootstrap secret
/// comes from the environment.
async fn obtain_token_slot(permissions: Vec<Permission>, slot: usize) -> Result<String, Error> {
    let key = token_cache_key(&permissions, slot);

    if let Some(token) = inproc_get(&key).or_else(|| read_cached_token(&key)) {
        return Ok(token);
    }

    // Only one mint at a time; re-check the caches after acquiring the lock in
    // case another task minted while we waited.
    let _guard = MINT_LOCK.lock().await;
    if let Some(token) = inproc_get(&key).or_else(|| read_cached_token(&key)) {
        return Ok(token);
    }

    let ttl_secs = 3600u64;
    let client = create_test_client()?;
    let response = client
        .issue_token(&TokenRequest {
            secret: get_bootstrap_secret(),
            permissions,
            ttl_secs: Some(ttl_secs),
        })
        .await?;
    let expiry = now_secs() + ttl_secs;
    inproc_put(&key, expiry, &response.token);
    write_cached_token(&key, &response.token, expiry);
    Ok(response.token)
}

/// Obtains a JWT with the given permissions from a single shared cached subject
/// (pool slot 0).
///
/// # Errors
/// Returns error if the request fails or issuance is disabled/rejected.
pub async fn obtain_token(permissions: Vec<Permission>) -> Result<String, Error> {
    obtain_token_slot(permissions, 0).await
}

fn client_with_token(token: String) -> Result<OrderbookClient, Error> {
    OrderbookClient::new(ClientConfig {
        base_url: get_api_url(),
        timeout: Duration::from_secs(10),
        token: Some(token),
    })
}

/// Creates an authenticated test client carrying a freshly issued (or cached)
/// token with the given permissions (single shared subject).
///
/// # Errors
/// Returns error if token issuance or client creation fails.
pub async fn create_authenticated_client(
    permissions: Vec<Permission>,
) -> Result<OrderbookClient, Error> {
    client_with_token(obtain_token(permissions).await?)
}

/// Read-only client (`Read`) backed by a single shared subject.
///
/// # Errors
/// Returns error if token issuance or client creation fails.
pub async fn read_client() -> Result<OrderbookClient, Error> {
    client_with_token(obtain_token_slot(vec![Permission::Read], 0).await?)
}

/// Trading client (`Read` + `Trade`) backed by a single shared subject.
///
/// # Errors
/// Returns error if token issuance or client creation fails.
pub async fn trade_client() -> Result<OrderbookClient, Error> {
    client_with_token(obtain_token_slot(vec![Permission::Read, Permission::Trade], 0).await?)
}

/// Admin client (`Admin`, which implies all), drawn round-robin from a pool of
/// shared subjects so request load spreads across several rate-limit buckets.
///
/// # Errors
/// Returns error if token issuance or client creation fails.
pub async fn admin_client() -> Result<OrderbookClient, Error> {
    static NEXT_SLOT: AtomicUsize = AtomicUsize::new(0);
    static OFFSET: OnceLock<usize> = OnceLock::new();
    // Seed per-process so distinct test binaries start on distinct pool slots,
    // spreading the "first admin call of each binary" load across the pool.
    let offset = *OFFSET.get_or_init(|| std::process::id() as usize);
    let slot = (offset + NEXT_SLOT.fetch_add(1, Ordering::Relaxed)) % ADMIN_POOL_SIZE;
    client_with_token(obtain_token_slot(vec![Permission::Admin], slot).await?)
}

/// Serializes tests that touch GLOBAL market-maker control state.
///
/// The control endpoints (`/api/v1/controls/*`) act on process-global server
/// state: the master kill switch and the shared quoting parameters
/// (`spread_multiplier` / `size_scalar` / `directional_skew`). Within a single
/// test binary `cargo test` runs the test functions concurrently on a thread
/// pool, so two control tests can otherwise interleave a read between another
/// test's write and its restore, or clobber each other's restore. Every test
/// that mutates (or reads-under-mutation) global controls acquires this lock for
/// the WHOLE test body:
///
/// ```ignore
/// let _guard = control_lock().lock().await;
/// ```
///
/// This is an IN-PROCESS async lock, so it only serializes tests *within one
/// test binary*. That is sufficient because `cargo test` runs the test binaries
/// themselves sequentially by default (it parallelizes test functions inside a
/// binary, not the binaries against one another), so no two binaries exercise the
/// control endpoints at the same time. If that default ever changes (e.g.
/// `cargo nextest` running binaries in parallel), this lock would need to become
/// a cross-process guard (e.g. a file lock on the target server).
#[must_use]
pub fn control_lock() -> &'static tokio::sync::Mutex<()> {
    static CONTROL_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    CONTROL_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Generates a unique test symbol to avoid conflicts between tests.
#[must_use]
pub fn unique_symbol(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}_{}_{}", prefix, ts, counter)
}

/// Creates a fresh underlying with a single expiration and strike, and returns
/// `(underlying, formatted_expiration)`.
///
/// The `client` must carry `Trade` (creating the expiration/strike is a mutation).
/// Since the #110 fix the returned formatted expiration equals
/// [`TEST_EXPIRATION`] for date-form input; it remains the canonical read key
/// (see the module docs).
///
/// # Panics
/// Panics if any setup request fails or the underlying reports no expiration.
pub async fn setup_underlying(client: &OrderbookClient, prefix: &str) -> (String, String) {
    let underlying = unique_symbol(prefix);
    client
        .create_underlying(&underlying)
        .await
        .expect("create underlying");
    client
        .create_expiration(&underlying, TEST_EXPIRATION)
        .await
        .expect("create expiration");
    client
        .create_strike(&underlying, TEST_EXPIRATION, TEST_STRIKE)
        .await
        .expect("create strike");
    let formatted = formatted_expiration(client, &underlying).await;
    (underlying, formatted)
}

/// Returns the server-formatted canonical string of an underlying's first
/// expiration (the value the READ / modify / cancel paths resolve books by).
///
/// # Panics
/// Panics if the request fails or the underlying reports no expiration.
pub async fn formatted_expiration(client: &OrderbookClient, underlying: &str) -> String {
    client
        .list_expirations(underlying)
        .await
        .expect("list expirations")
        .expirations
        .into_iter()
        .next()
        .expect("underlying has at least one expiration")
}

/// Best-effort deletion of a test underlying (ignores failures so cleanup never
/// masks a test assertion).
pub async fn cleanup_underlying(client: &OrderbookClient, underlying: &str) {
    let _ = client.delete_underlying(underlying).await;
}
