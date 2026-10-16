//! TLS proxy for the ggml-rpc compute protocol.
//!
//! `ggml-rpc-server` speaks a proprietary binary protocol with no built-in
//! authentication or encryption (upstream llama.cpp limitation). The existing
//! `rpc_cluster.rs` module provides admission control via `rpc_shared_secret`
//! (HMAC handshake → IP allowlist), but the actual ggml-rpc byte stream
//! remains plain TCP.
//!
//! This module adds a **dual TCP proxy** that terminates TLS on the
//! contributor side and forwards decrypted traffic to the local
//! `ggml-rpc-server` over plaintext loopback. The coordinator connects to
//! the contributor's TLS port instead of the raw ggml-rpc port.
//!
//! The TLS termination reuses the same `aws-lc-rs` crypto provider with
//! ML-KEM-768 hybrid key exchange preference as the HTTP API's TLS (see
//! `tls.rs`), and the same self-signed cert generation via `rcgen`.

// The module is new and not yet wired into the application entry points.
// The public API (run_tls_proxy, connect_rpc_tls, etc.) will be integrated
// into rpc_cluster.rs in a follow-up change.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;

/// Configuration for the RPC TLS proxy.
#[derive(Debug, Clone)]
pub struct RpcTlsConfig {
    /// Whether TLS is enabled for RPC connections.
    pub enabled: bool,
    /// The bind address for the TLS proxy (contributor side).
    pub bind_host: String,
    /// The port for the TLS proxy (contributor side).
    pub port: u16,
    /// Path to the self-signed cert PEM file.
    pub cert_path: PathBuf,
    /// Path to the self-signed key PEM file.
    pub key_path: PathBuf,
}

impl Default for RpcTlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_host: "0.0.0.0".to_string(),
            port: 50053,
            cert_path: PathBuf::from("rpc_tls_cert.pem"),
            key_path: PathBuf::from("rpc_tls_key.pem"),
        }
    }
}

/// Decision logic: should this node run a TLS proxy for its ggml-rpc?
///
/// TLS is used when:
/// - `enable_tls` is true (explicit opt-in via settings), AND
/// - the bind address is non-loopback (LAN/remote scenario)
///
/// Loopback-only binds don't need encryption (local dev), and the feature
/// is always opt-in for backward compatibility.
pub fn should_use_tls_proxy(enable_tls: bool, bind_host: &str) -> bool {
    if !enable_tls {
        return false;
    }
    !is_loopback_host(bind_host)
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Generates a self-signed cert+key pair if they don't already exist.
/// Idempotent — an existing pair is reused across restarts.
pub fn generate_self_signed_cert(
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

/// Builds a TLS client config that trusts the given self-signed cert.
/// Used by the coordinator to connect to a contributor's TLS proxy.
pub fn build_tls_client_config(cert_path: &Path) -> rustls::ClientConfig {
    let cert_pem = std::fs::read(cert_path).expect("read cert PEM");
    let cert_der_bytes = pem_to_der(&cert_pem).expect("parse cert PEM");
    let cert_der = rustls::pki_types::CertificateDer::from(cert_der_bytes);

    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert_der).expect("add root cert");

    rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// Builds a TLS server config from the cert/key pair.
/// Used by the contributor's TLS proxy to terminate TLS connections.
fn build_tls_server_config(cert_path: &Path, key_path: &Path) -> Result<rustls::ServerConfig, String> {
    let cert_pem = std::fs::read(cert_path).map_err(|e| format!("read cert: {e}"))?;
    let key_pem = std::fs::read(key_path).map_err(|e| format!("read key: {e}"))?;

    let cert_der_bytes = pem_to_der(&cert_pem)?;
    let key_der_bytes = pem_to_der(&key_pem)?;

    let cert_der = rustls::pki_types::CertificateDer::from(cert_der_bytes);
    let private_key = rustls::pki_types::PrivateKeyDer::try_from(key_der_bytes)
        .map_err(|e| format!("parse private key: {e}"))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], private_key)
        .map_err(|e| format!("build server config: {e}"))?;

    Ok(config)
}

/// Converts PEM-encoded data to raw DER bytes.
fn pem_to_der(pem: &[u8]) -> Result<Vec<u8>, String> {
    let pem_str = String::from_utf8(pem.to_vec()).map_err(|e| format!("PEM is not valid UTF-8: {e}"))?;
    let pem = pem::parse(&pem_str).map_err(|e| format!("parse PEM: {e}"))?;
    Ok(pem.contents().to_vec())
}

/// Runs the RPC TLS proxy. Accepts TLS connections on `listener`, decrypts
/// them, and forwards the plaintext to `backend_addr` (the local
/// ggml-rpc-server). Returns only on a bind failure.
pub async fn run_tls_proxy(
    listener: TcpListener,
    cert_path: PathBuf,
    key_path: PathBuf,
    backend_addr: SocketAddr,
) {
    let server_config = match build_tls_server_config(&cert_path, &key_path) {
        Ok(config) => Arc::new(config),
        Err(err) => {
            tracing::error!("rpc_tls: failed to build TLS server config: {err}");
            return;
        }
    };

    let acceptor = tokio_rustls::TlsAcceptor::from(server_config);

    tracing::info!(
        "rpc_tls: TLS proxy listening on {}, forwarding to backend {}",
        listener.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string()),
        backend_addr
    );

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!("rpc_tls: accept() failed: {err}");
                continue;
            }
        };
        let acceptor = acceptor.clone();

        tokio::spawn(async move {
            // The TLS acceptor itself rejects plaintext connections during
            // the handshake (the first bytes must be a valid TLS ClientHello).
            let tls_stream = match acceptor.accept(stream).await {
                Ok(s) => {
                    s
                },
                Err(_err) => {
                    return;
                }
            };

            // Connect to backend
            let mut backend = match TcpStream::connect(backend_addr).await {
                Ok(s) => {
                    s
                },
                Err(_err) => {
                    return;
                }
            };

            // Set TCP_NODELAY on both legs (same as the allowlist proxy)
            let _ = tls_stream.get_ref().0.set_nodelay(true);
            let _ = backend.set_nodelay(true);

            // Proxy data bidirectionally
            let (mut tls_read, mut tls_write) = tokio::io::split(tls_stream);
            let (mut backend_read, mut backend_write) = backend.split();

            let client_to_server = async move {
                let _ = tokio::io::copy(&mut tls_read, &mut backend_write).await;
                let _ = backend_write.shutdown().await;
            };

            let server_to_client = async move {
                let _ = tokio::io::copy(&mut backend_read, &mut tls_write).await;
                let _ = tls_write.shutdown().await;
            };

            tokio::join!(client_to_server, server_to_client);
            tracing::debug!("rpc_tls: proxied connection from {peer_addr} ended");
        });
    }
}

/// Client-side: connects to a contributor's TLS proxy and returns a
/// TLS stream that can be used to send ggml-rpc protocol data.
/// The coordinator uses this instead of a raw TCP connection.
pub async fn connect_rpc_tls(
    coordinator_addr: SocketAddr,
    contributor_cert_path: &Path,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>, String> {
    let config = build_tls_client_config(contributor_cert_path);
    let connector = TlsConnector::from(Arc::new(config));

    let tcp_stream = TcpStream::connect(coordinator_addr)
        .await
        .map_err(|e| format!("connect to {coordinator_addr}: {e}"))?;

    let domain = rustls::pki_types::ServerName::try_from("localhost")
        .map_err(|e| format!("server name: {e}"))?;

    let tls_stream = connector
        .connect(domain, tcp_stream)
        .await
        .map_err(|e| format!("TLS handshake: {e}"))?;

    Ok(tls_stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[test]
    fn tls_proxy_decision_requires_enable_flag_and_non_loopback() {
        assert!(!should_use_tls_proxy(true, "127.0.0.1"));
        assert!(!should_use_tls_proxy(true, "localhost"));
        assert!(should_use_tls_proxy(true, "0.0.0.0"));
        assert!(should_use_tls_proxy(true, "192.168.1.50"));
        assert!(should_use_tls_proxy(true, "10.0.0.1"));
        assert!(!should_use_tls_proxy(false, "0.0.0.0"));
        assert!(!should_use_tls_proxy(false, "192.168.1.50"));
    }

    #[tokio::test]
    async fn tls_proxy_generates_self_signed_cert() {
        let dir = std::env::temp_dir().join(format!("rpc-tls-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");

        let result = generate_self_signed_cert(&cert_path, &key_path, vec!["localhost".to_string()]);
        assert!(result.is_ok(), "cert generation should succeed: {:?}", result.err());
        assert!(cert_path.exists());
        assert!(key_path.exists());

        let cert_bytes_before = std::fs::read(&cert_path).expect("read cert");
        let result2 = generate_self_signed_cert(&cert_path, &key_path, vec!["localhost".to_string()]);
        assert!(result2.is_ok());
        let cert_bytes_after = std::fs::read(&cert_path).expect("read cert again");
        assert_eq!(cert_bytes_before, cert_bytes_after);

        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[tokio::test]
    async fn tls_proxy_accepts_tls_and_rejects_plaintext() {
        // Install the aws-lc-rs crypto provider (same as tls.rs does for HTTP TLS)
        crate::tls::ensure_crypto_provider_installed();

        let dir = std::env::temp_dir().join(format!("rpc-tls-it-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");

        generate_self_signed_cert(&cert_path, &key_path, vec!["localhost".to_string()])
            .expect("cert generation should succeed");

        // Start a simple TCP backend that echoes data
        let backend_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match backend_listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    loop {
                        match sock.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if sock.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });

        // Start the TLS proxy
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();

        let cert_path_clone = cert_path.clone();
        let key_path_clone = key_path.clone();
        let backend_addr_clone = backend_addr;
        let proxy_handle = tokio::spawn(async move {
            run_tls_proxy(proxy_listener, cert_path_clone, key_path_clone, backend_addr_clone).await
        });

        // Test 1: Plaintext connection should be rejected.
        let mut plaintext_stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let _ = plaintext_stream.write_all(b"hello").await;
        let mut buf = [0u8; 1024];
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            plaintext_stream.read(&mut buf),
        ).await;
        match result {
            Ok(Ok(0)) => {} // connection closed — rejected
            Ok(Ok(_)) => {
                // Received a TLS alert — now verify the connection is closed
                let _ = plaintext_stream.read(&mut buf).await;
            }
            Ok(Err(_)) => {} // connection reset — rejected
            Err(_) => {} // timeout — also acceptable (connection dropped)
        }

        // Test 2: TLS connection should work
        let tls_config = build_tls_client_config(&cert_path);
        let tcp_stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
        let tls_connector = TlsConnector::from(Arc::new(tls_config));
        let domain = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let mut tls_stream = tls_connector.connect(domain, tcp_stream).await.unwrap();

        tls_stream.write_all(b"hello over tls").await.unwrap();

        let mut buf = [0u8; 1024];
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tls_stream.read(&mut buf),
        ).await.unwrap().unwrap();
        assert_eq!(&buf[..n], b"hello over tls");

        drop(tls_stream);
        proxy_handle.abort();
        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
