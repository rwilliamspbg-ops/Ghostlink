# Security Policy

## Supported Versions

Ghostlink actively receives security patches and updates for the following versions:

| Version | Supported | Notes |
| :--- | :--- | :--- |
| **2.0.x** | :white_check_mark: | Active release lineage (RBAC, durable audit trail, RPC peer auth, PQC/hybrid TLS, TCP circuit breakers, gateway security). |
| **1.17.x** | :warning: | Critical security patches only. |
| **< 1.17** | :x: | Unsupported. Please upgrade to `v2.0.x`. |

---

## Security Architecture Overview

Ghostlink is built for local-first, privacy-focused inference across heterogeneous hardware. Key security boundaries include:

* **Control-Plane Gateway (`Port 8000`):** Acts as the primary security ingress for the GUI, handling CORS policies, rate-limiting, and request sanitization.
* **Network Discovery:** HMAC-SHA256 authenticated frames prevent unauthorized node peering over local UDP/mDNS discovery broadcasts.
* **Transport Encryption:** Support for hybrid Post-Quantum Cryptography (PQC) and standard TLS for inter-node RPC channels.

---

## Reporting a Vulnerability

We take the security and privacy of Ghostlink seriously. If you discover a potential security vulnerability, please **do not** open a public issue.

### How to Report

1. **Email:** Contact us directly at **`r.williamspbg@gmail.com`**.
2. **Details to Include:**
   * A clear description of the vulnerability and potential impact.
   * Steps to reproduce the issue (proof-of-concept script, configuration, or CLI flags).
   * Affected components (e.g., Go Control-Plane Gateway, Rust Transport, ring buffer, API endpoint).

### Disclosure & Response Timeline

* **Acknowledgment:** You will receive an initial response within **24–48 hours** acknowledging receipt of your report.
* **Assessment:** We will assess the severity and impact within **5 business days** and provide an estimated fix timeline.
* **Resolution & Patching:** Once a fix is verified, a patch release (e.g., `v2.0.x`) will be published, along with appropriate credit to the reporter (unless anonymity is requested).

---

## Security Best Practices for Operators

* **Gateway Binding:** Always expose the Go Control-Plane Gateway (`8000`) to external UI clients rather than binding the internal Ghostlink API (`8003`) directly to public interfaces.
* **Network Isolation:** Run multi-node Ghostlink clusters within trusted local area networks (LAN) or secure VPN overlays (e.g., Tailscale, WireGuard).
