//! Self-signed TLS termination with a real ML-KEM-768 hybrid (PQC) key
//! exchange preference (via rustls's `prefer-post-quantum` feature, backed
//! by aws-lc-rs — the same mechanism Chrome/Cloudflare/AWS use today), not
//! a bespoke handshake protocol. Opt-in via `RuntimeSettings.enable_tls`,
//! forced on when the server binds a non-loopback address (see
//! `is_loopback_host`) since that's the LAN/remote scenario PQC-hybrid TLS
//! is meant to protect. This is a self-hosted local tool, not a public
//! service, so a self-signed cert generated once on first use — no CA
//! involved — is the right call.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Once;

static INSTALL_PROVIDER: Once = Once::new();

/// Installs the aws-lc-rs-backed rustls crypto provider (the one built
/// with the `prefer-post-quantum` Cargo feature) as the process default,
/// if not already installed. Idempotent — safe to call on every listener
/// start, including in tests.
pub fn ensure_crypto_provider_installed() {
    INSTALL_PROVIDER.call_once(|| {
        // Only fails if a default was already installed by someone else;
        // fine to ignore since that means a provider is active either way.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

pub fn cert_path() -> PathBuf {
    std::env::var("GHOSTLINK_TLS_CERT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tls_cert.pem"))
}

pub fn key_path() -> PathBuf {
    std::env::var("GHOSTLINK_TLS_KEY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tls_key.pem"))
}

/// Generates a self-signed cert+key for `san_names` if the files at
/// `cert_path`/`key_path` don't already both exist. Idempotent — an
/// existing pair is reused across restarts rather than regenerated, so a
/// client that already trusted the cert once doesn't see a new one every
/// launch.
pub fn ensure_self_signed_cert(
    cert_path: &Path,
    key_path: &Path,
    san_names: Vec<String>,
) -> Result<(), String> {
    if cert_path.exists() && key_path.exists() {
        return Ok(());
    }

    let certified_key = rcgen::generate_simple_self_signed(san_names)
        .map_err(|e| format!("failed to generate self-signed certificate: {e}"))?;

    std::fs::write(cert_path, certified_key.cert.pem())
        .map_err(|e| format!("failed to write TLS cert to {}: {e}", cert_path.display()))?;
    std::fs::write(key_path, certified_key.signing_key.serialize_pem())
        .map_err(|e| format!("failed to write TLS key to {}: {e}", key_path.display()))?;

    Ok(())
}

/// `true` for loopback/localhost-only hosts — the boundary used to decide
/// whether TLS is forced on by default. Binding a non-loopback address is
/// exactly the LAN/remote scenario PQC-hybrid TLS protects; pure-localhost
/// dev flow stays plaintext by default so existing setups see zero
/// disruption.
pub fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Loads (generating if needed) a self-signed cert/key pair and builds an
/// `axum-server` Rustls config from it, with the process's PQC-preferring
/// crypto provider installed first.
pub async fn build_rustls_config(
    host: &str,
) -> Result<axum_server::tls_rustls::RustlsConfig, String> {
    ensure_crypto_provider_installed();

    let cert = cert_path();
    let key = key_path();
    let mut san_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if !is_loopback_host(host) {
        san_names.push(host.to_string());
    }
    ensure_self_signed_cert(&cert, &key, san_names)?;

    axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
        .await
        .map_err(|e| format!("failed to load TLS cert/key into rustls config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_loopback_host_recognizes_common_local_forms() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LOCALHOST"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("::1"));
    }

    #[test]
    fn is_loopback_host_rejects_lan_and_remote_addresses() {
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("192.168.1.50"));
        assert!(!is_loopback_host("example.com"));
        assert!(!is_loopback_host(""));
    }

    #[test]
    fn ensure_self_signed_cert_generates_once_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("ghostlink-tls-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);

        ensure_self_signed_cert(&cert_path, &key_path, vec!["localhost".to_string()])
            .expect("first generation should succeed");
        assert!(cert_path.exists());
        assert!(key_path.exists());

        let cert_bytes_before = std::fs::read(&cert_path).expect("read cert");

        // Second call must be a no-op — same cert reused, not regenerated.
        ensure_self_signed_cert(&cert_path, &key_path, vec!["localhost".to_string()])
            .expect("idempotent call should succeed");
        let cert_bytes_after = std::fs::read(&cert_path).expect("read cert again");
        assert_eq!(cert_bytes_before, cert_bytes_after);

        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
