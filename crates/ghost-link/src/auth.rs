//! Real bearer-token auth: a persisted, hashed API-key store (each key
//! carrying a role — Admin/Operator/Viewer) and short-lived JWTs
//! (`jsonwebtoken`, HS256) issued by exchanging a valid key — replacing
//! both the previous hardcoded `"new-token-123"` JWT-refresh stub and the
//! single-shared-key-with-no-roles model that preceded this file.
//! Independent of whether TLS (see `tls.rs`) is on, though obviously
//! weaker without it: a bearer token sent over plaintext HTTP can be
//! observed in transit.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static ACTIVE_API_KEY: OnceLock<String> = OnceLock::new();
static JWT_SIGNING_SECRET: OnceLock<String> = OnceLock::new();

const JWT_LIFETIME_SECS: u64 = 3600;

/// The bootstrap key's fixed id — deterministic so `load_api_keys` always
/// adopts an existing `api_key.txt` as the same stable Admin record across
/// restarts rather than minting a new id every time one is synthesized.
pub const BOOTSTRAP_KEY_ID: &str = "key_bootstrap";

/// A key's permission level. Ordered `Admin > Operator > Viewer` — see
/// `Role::satisfies`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

impl Role {
    fn rank(self) -> u8 {
        match self {
            Role::Viewer => 0,
            Role::Operator => 1,
            Role::Admin => 2,
        }
    }

    /// Whether a key with this role may access a route requiring `required`.
    pub fn satisfies(self, required: Role) -> bool {
        self.rank() >= required.rank()
    }
}

/// One persisted, hashed API key. The raw key value is never stored —
/// only its SHA-256 hash (for lookup) and a display-safe preview (last 4
/// chars, so an operator can tell keys apart in a list without either of
/// them being able to reconstruct the real value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub key_hash: String,
    pub key_preview: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// The identity attached to an authenticated request — resolved once by
/// `authenticate` and reused by both the role check and (for
/// `/api/security/jwt/refresh`) the handler that needs to know who's
/// asking.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub key_id: String,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
    iat: u64,
}

fn api_key_path() -> PathBuf {
    std::env::var("GHOSTLINK_API_KEY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("api_key.txt"))
}

fn jwt_secret_path() -> PathBuf {
    std::env::var("GHOSTLINK_JWT_SECRET_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("jwt_secret.txt"))
}

/// Generates a fresh 256-bit secret, hex-encoded — used for both API keys
/// and the JWT signing secret, which need the same entropy/format shape
/// but must never be the same value as each other (see `jwt_signing_secret`).
fn generate_secret() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn generate_api_key() -> String {
    generate_secret()
}

/// SHA-256 hex digest of a raw key/token — what's actually persisted and
/// compared, never the raw value itself.
fn hash_key(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Last 4 characters of a raw key, for display in key-list responses —
/// never enough to reconstruct or brute-force the real value from.
fn key_preview(raw: &str) -> String {
    let len = raw.len();
    if len <= 4 {
        raw.to_string()
    } else {
        raw[len - 4..].to_string()
    }
}

/// Loads the persisted API key, generating and persisting a new one on
/// first run (and printing it once, the same "show the first-run secret
/// exactly once" convention tools like Jupyter use — there's no other way
/// for the operator to learn it, since it's never returned by any API
/// response). Reused across restarts once the file exists.
fn load_or_create_api_key() -> String {
    let path = api_key_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    let key = generate_api_key();
    if let Err(err) = std::fs::write(&path, &key) {
        eprintln!(
            "warning: failed to persist API key to {}: {err} (a new one will be generated next restart)",
            path.display()
        );
    }
    println!("=======================================================================");
    println!(
        "Generated a new Ghostlink API key (saved to {}):",
        path.display()
    );
    println!("  {key}");
    println!("Send it as `Authorization: Bearer {key}` on every API request, or");
    println!("exchange it for a short-lived token via POST /api/security/jwt/refresh.");
    println!("This key is now the 'bootstrap' Admin entry in your key store — see");
    println!("GET /api/security/keys to create additional, more narrowly-scoped keys.");
    println!("=======================================================================");
    key
}

/// The process-wide bootstrap API key, loaded (or generated) once and
/// cached. Only used to (a) seed the bootstrap `ApiKeyRecord` the first
/// time `api_keys.json` doesn't exist yet, and (b) print the first-run
/// banner above — actual auth checks go through the key *store*
/// (`authenticate`), not this value directly, once the store exists.
pub fn active_api_key() -> &'static str {
    ACTIVE_API_KEY.get_or_init(load_or_create_api_key)
}

/// Loads (or generates + persists) the JWT signing secret. Deliberately a
/// distinct value from any individual API key: with multiple, independently
/// revocable keys, signing JWTs with one specific key's value would mean
/// rotating that unrelated key silently invalidated every other key's
/// outstanding JWTs.
fn jwt_signing_secret() -> &'static str {
    JWT_SIGNING_SECRET.get_or_init(|| {
        let path = jwt_secret_path();
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let trimmed = existing.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
        let secret = generate_secret();
        if let Err(err) = std::fs::write(&path, &secret) {
            eprintln!(
                "warning: failed to persist JWT signing secret to {}: {err} (a new one will be generated next restart, invalidating outstanding JWTs)",
                path.display()
            );
        }
        secret
    })
}

/// Builds the bootstrap `ApiKeyRecord` from the existing (or freshly
/// generated) `active_api_key()` value — the migration path that makes an
/// existing `api_key.txt` keep working, unchanged, as the sole Admin key
/// once the key store exists.
pub fn bootstrap_key_record() -> ApiKeyRecord {
    let raw = active_api_key();
    ApiKeyRecord {
        id: BOOTSTRAP_KEY_ID.to_string(),
        name: "bootstrap".to_string(),
        role: Role::Admin,
        key_hash: hash_key(raw),
        key_preview: key_preview(raw),
        created_at: chrono::Utc::now().to_rfc3339(),
        last_used_at: None,
    }
}

/// Creates a new key record with a freshly generated raw value. Returns
/// both the record (to persist) and the raw key (to return to the caller
/// exactly once — it's never recoverable after this call returns, since
/// only its hash is kept).
pub fn create_key(name: String, role: Role) -> (ApiKeyRecord, String) {
    let raw = generate_api_key();
    let id = format!(
        "key_{}",
        rand::random::<[u8; 6]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let record = ApiKeyRecord {
        id,
        name,
        role,
        key_hash: hash_key(&raw),
        key_preview: key_preview(&raw),
        created_at: chrono::Utc::now().to_rfc3339(),
        last_used_at: None,
    };
    (record, raw)
}

/// Issues a short-lived JWT for the given key id, signed with the
/// server-wide JWT signing secret (not any individual key's value — see
/// `jwt_signing_secret`).
pub fn issue_jwt(key_id: &str) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock error: {e}"))?
        .as_secs();
    let claims = Claims {
        sub: key_id.to_string(),
        iat: now,
        exp: now + JWT_LIFETIME_SECS,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_signing_secret().as_bytes()),
    )
    .map_err(|e| format!("failed to sign JWT: {e}"))
}

fn verify_jwt(token: &str) -> Option<String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_signing_secret().as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims.sub)
}

/// Resolves a bearer token (raw API key or a JWT issued by `issue_jwt`)
/// against the live key store, returning the matching key's identity and
/// current role. A JWT is only honored if its subject key id is *still
/// present* in `keys` — this is what makes revoking a key also invalidate
/// any outstanding JWT for it immediately, rather than waiting out the
/// JWT's lifetime. Role is always read fresh from the current record, not
/// from JWT claims, so a role change also takes effect immediately.
pub fn authenticate(token: &str, keys: &[ApiKeyRecord]) -> Option<AuthContext> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let hashed = hash_key(token);
    if let Some(record) = keys.iter().find(|k| k.key_hash == hashed) {
        return Some(AuthContext {
            key_id: record.id.clone(),
            name: record.name.clone(),
            role: record.role,
        });
    }

    let subject = verify_jwt(token)?;
    keys.iter()
        .find(|k| k.id == subject)
        .map(|record| AuthContext {
            key_id: record.id.clone(),
            name: record.name.clone(),
            role: record.role,
        })
}

/// Extracts the bearer token from an `Authorization: Bearer <token>`
/// header value, if present and well-formed.
pub fn extract_bearer_token(header_value: Option<&str>) -> Option<&str> {
    header_value?.strip_prefix("Bearer ").map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Real filesystem + real env var mutation — serialized like this
    // project's other env-var-mutating tests (see
    // runtime_switcher::env_test_lock) since `GHOSTLINK_API_KEY_PATH` is
    // process-global and `cargo test` runs in parallel by default.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn generate_api_key_produces_64_hex_chars_of_real_entropy() {
        let a = generate_api_key();
        let b = generate_api_key();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two generated keys should not collide");
    }

    #[test]
    fn load_or_create_api_key_persists_and_reuses_across_calls() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path =
            std::env::temp_dir().join(format!("ghostlink-test-api-key-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_API_KEY_PATH", &path);

        let first = load_or_create_api_key();
        assert!(path.exists());
        let second = load_or_create_api_key();
        assert_eq!(
            first, second,
            "second call should reuse the persisted key, not regenerate"
        );

        std::env::remove_var("GHOSTLINK_API_KEY_PATH");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn extract_bearer_token_parses_well_formed_header_only() {
        assert_eq!(extract_bearer_token(Some("Bearer abc123")), Some("abc123"));
        assert_eq!(extract_bearer_token(Some("bearer abc123")), None);
        assert_eq!(extract_bearer_token(Some("abc123")), None);
        assert_eq!(extract_bearer_token(None), None);
    }

    #[test]
    fn role_satisfies_is_a_strict_hierarchy() {
        assert!(Role::Admin.satisfies(Role::Admin));
        assert!(Role::Admin.satisfies(Role::Operator));
        assert!(Role::Admin.satisfies(Role::Viewer));
        assert!(Role::Operator.satisfies(Role::Operator));
        assert!(Role::Operator.satisfies(Role::Viewer));
        assert!(!Role::Operator.satisfies(Role::Admin));
        assert!(Role::Viewer.satisfies(Role::Viewer));
        assert!(!Role::Viewer.satisfies(Role::Operator));
        assert!(!Role::Viewer.satisfies(Role::Admin));
    }

    #[test]
    fn hash_key_is_deterministic_and_distinct_per_input() {
        assert_eq!(hash_key("abc"), hash_key("abc"));
        assert_ne!(hash_key("abc"), hash_key("abd"));
        assert_eq!(hash_key("abc").len(), 64);
    }

    #[test]
    fn key_preview_never_exposes_more_than_the_last_four_chars() {
        assert_eq!(key_preview("abcdefgh"), "efgh");
        assert_eq!(key_preview("ab"), "ab");
    }

    #[test]
    fn create_key_returns_a_record_whose_hash_matches_the_raw_value_and_never_stores_it() {
        let (record, raw) = create_key("ci-bot".to_string(), Role::Operator);
        assert_eq!(record.name, "ci-bot");
        assert_eq!(record.role, Role::Operator);
        assert_eq!(record.key_hash, hash_key(&raw));
        assert_eq!(record.key_preview, key_preview(&raw));
        assert!(record.id.starts_with("key_"));
    }

    #[test]
    fn authenticate_matches_raw_key_and_rejects_unknown_or_empty_tokens() {
        let (record, raw) = create_key("tester".to_string(), Role::Admin);
        let keys = vec![record.clone()];

        let ctx = authenticate(&raw, &keys).expect("raw key should authenticate");
        assert_eq!(ctx.key_id, record.id);
        assert_eq!(ctx.role, Role::Admin);

        assert!(authenticate("not-a-real-key", &keys).is_none());
        assert!(authenticate("", &keys).is_none());
    }

    #[test]
    fn authenticate_honors_a_jwt_only_while_its_key_id_still_exists_in_the_store() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path = std::env::temp_dir().join(format!(
            "ghostlink-test-jwt-secret-{}.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_JWT_SECRET_PATH", &path);

        let (record, _raw) = create_key("jwt-holder".to_string(), Role::Operator);
        let jwt = issue_jwt(&record.id).expect("issue_jwt should succeed");

        let keys_with_record = vec![record.clone()];
        let ctx =
            authenticate(&jwt, &keys_with_record).expect("JWT for a live key should authenticate");
        assert_eq!(ctx.key_id, record.id);
        assert_eq!(ctx.role, Role::Operator);

        // Simulate revocation: the key id is no longer in the store.
        let keys_without_record: Vec<ApiKeyRecord> = vec![];
        assert!(
            authenticate(&jwt, &keys_without_record).is_none(),
            "a JWT for a revoked key must be rejected even though it hasn't expired"
        );

        assert!(authenticate(&format!("{jwt}tampered"), &keys_with_record).is_none());

        std::env::remove_var("GHOSTLINK_JWT_SECRET_PATH");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn authenticate_reflects_a_live_role_change_not_a_stale_jwt_claim() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path = std::env::temp_dir().join(format!(
            "ghostlink-test-jwt-secret-role-{}.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_JWT_SECRET_PATH", &path);

        let (mut record, _raw) = create_key("role-change".to_string(), Role::Viewer);
        let jwt = issue_jwt(&record.id).expect("issue_jwt should succeed");

        record.role = Role::Admin;
        let keys = vec![record];
        let ctx =
            authenticate(&jwt, &keys).expect("JWT should still authenticate after a role change");
        assert_eq!(
            ctx.role,
            Role::Admin,
            "role must be read fresh from the store, not from the JWT claims"
        );

        std::env::remove_var("GHOSTLINK_JWT_SECRET_PATH");
        let _ = std::fs::remove_file(&path);
    }
}
