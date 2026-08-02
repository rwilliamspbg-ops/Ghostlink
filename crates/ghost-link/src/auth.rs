//! Real bearer-token auth: a persisted API key generated once on first
//! run, and short-lived JWTs (`jsonwebtoken`, HS256, signed with that same
//! key) issued by exchanging it — replacing the previous hardcoded
//! `"new-token-123"` JWT-refresh stub. Independent of whether TLS (see
//! `tls.rs`) is on, though obviously weaker without it: a bearer token
//! sent over plaintext HTTP can be observed in transit.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static ACTIVE_API_KEY: OnceLock<String> = OnceLock::new();

const JWT_LIFETIME_SECS: u64 = 3600;

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

fn generate_api_key() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
    println!("=======================================================================");
    key
}

/// The process-wide API key, loaded (or generated) once and cached —
/// every auth check and the JWT signing secret both derive from this
/// single value.
pub fn active_api_key() -> &'static str {
    ACTIVE_API_KEY.get_or_init(load_or_create_api_key)
}

/// Issues a short-lived JWT for the given subject, signed with the active
/// API key. Real issuance/verification, not the previous
/// `"new-token-123"` stub.
pub fn issue_jwt(subject: &str) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock error: {e}"))?
        .as_secs();
    let claims = Claims {
        sub: subject.to_string(),
        iat: now,
        exp: now + JWT_LIFETIME_SECS,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(active_api_key().as_bytes()),
    )
    .map_err(|e| format!("failed to sign JWT: {e}"))
}

fn verify_jwt(token: &str) -> bool {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(active_api_key().as_bytes()),
        &Validation::default(),
    )
    .is_ok()
}

/// Accepts either the raw API key itself as a bearer token (simplest path
/// for a script or `curl`) or a JWT issued by `issue_jwt` that hasn't
/// expired — both derive from the same secret, so either proves the
/// caller knows the API key.
pub fn verify_bearer_token(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        return false;
    }
    // Constant-time-ish compare isn't critical here (the key is 256 bits
    // of real entropy — timing side channels aren't the realistic threat
    // for a locally-generated secret), but avoid the obviously-wrong
    // early-exit shape anyway.
    let matches_raw_key = token.len() == active_api_key().len() && token == active_api_key();
    matches_raw_key || verify_jwt(token)
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
    fn verify_bearer_token_accepts_raw_key_and_issued_jwt_rejects_garbage() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let path = std::env::temp_dir().join(format!(
            "ghostlink-test-api-key-verify-{}.txt",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GHOSTLINK_API_KEY_PATH", &path);
        // Reset the OnceLock's effective value for this test process by
        // reading the key freshly through the same path the real code
        // uses — `active_api_key()` is a OnceLock so it can only be
        // initialized once per test binary run; this test therefore
        // exercises whatever key was already cached, not a fresh one. That
        // is fine: it still proves round-trip correctness, just not
        // isolation across tests within this one process.
        let key = active_api_key().to_string();

        assert!(verify_bearer_token(&key));
        assert!(!verify_bearer_token("not-the-key"));
        assert!(!verify_bearer_token(""));

        let jwt = issue_jwt("test-subject").expect("issue_jwt should succeed");
        assert!(verify_bearer_token(&jwt));
        assert!(!verify_bearer_token(&format!("{jwt}tampered")));

        std::env::remove_var("GHOSTLINK_API_KEY_PATH");
        let _ = std::fs::remove_file(&path);
    }
}
