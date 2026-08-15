//! Optional OpenTelemetry tracing export.
//!
//! `tracing` is used elsewhere in this codebase purely as a logging facade
//! (`tracing::warn!`/`info!`) — this module adds real span export on top,
//! entirely opt-in. When `GHOSTLINK_OTEL_EXPORTER_ENDPOINT` is unset, this
//! reproduces the exact plain-text console logging this codebase has always
//! had (`tracing_subscriber::fmt().init()`), byte-for-byte, zero behavior
//! change. When set, spans — the HTTP root span from `tower-http`'s
//! `TraceLayer`, plus the hand-instrumented phase spans in the
//! distributed-inference model-load path — are additionally exported via
//! OTLP/HTTP to whatever collector/backend the endpoint points at.
//!
//! Real cross-process trace propagation through the actual `ggml-rpc` TCP
//! hop is **not** attempted — upstream llama.cpp's `--rpc` client speaks a
//! raw binary protocol with no header/metadata slot for W3C trace-context
//! propagation, the same hard constraint `rpc_cluster.rs` hit building RPC
//! peer auth. A trace ends at "launched llama-server, here's how long it
//! took," not a per-remote-device breakdown of an RPC hop.
//!
//! Deliberately uses the SDK's **synchronous** `SimpleSpanProcessor`
//! (`reqwest-blocking-client`, not the async reqwest client) rather than
//! the batched processor: this is initialized in `main()` before any tokio
//! runtime exists (`main()` dispatches several non-`serve` subcommands too,
//! e.g. `flow`/`stage-worker`/`probe`), so the exporter's HTTP client must
//! not depend on an ambient async reactor. At the span volumes this first
//! pass produces — a handful of spans per request, not per-token —
//! synchronous export is a non-issue; a batched upgrade is a fine future
//! optimization if trace volume ever grows.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Reads `GHOSTLINK_OTEL_EXPORTER_ENDPOINT` — `None`/empty means tracing
/// export is off. An env var, not a `settings.json` field: the subscriber
/// has to be initialized before `settings.json` is even loaded.
pub fn otel_endpoint() -> Option<String> {
    std::env::var("GHOSTLINK_OTEL_EXPORTER_ENDPOINT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// `service.name` resource attribute — `GHOSTLINK_OTEL_SERVICE_NAME`
/// override, defaulting to `"ghost-link"`.
fn service_name() -> String {
    std::env::var("GHOSTLINK_OTEL_SERVICE_NAME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "ghost-link".to_string())
}

/// Initializes the global `tracing` subscriber — the single call site
/// `main()` makes, replacing the previous bare
/// `tracing_subscriber::fmt().init()`. Console logging (`fmt`) is always
/// active; the OTel export layer is added alongside it only when
/// `otel_endpoint()` returns `Some`, so console output is identical either
/// way. A failure to build the OTLP exporter (a bad endpoint URL, etc.) is
/// best-effort — warns and falls back to console-only logging rather than
/// failing the whole process over a misconfigured trace endpoint, matching
/// every other optional-feature failure mode in this codebase.
pub fn init_tracing() {
    let fmt_layer = tracing_subscriber::fmt::layer();

    let Some(endpoint) = otel_endpoint() else {
        tracing_subscriber::registry().with(fmt_layer).init();
        return;
    };

    match build_tracer(&endpoint) {
        Ok(tracer) => {
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            tracing_subscriber::registry()
                .with(fmt_layer)
                .with(otel_layer)
                .init();
            tracing::info!("otel: exporting traces to {endpoint}");
        }
        Err(err) => {
            tracing_subscriber::registry().with(fmt_layer).init();
            tracing::warn!(
                "otel: failed to initialize OTLP export to {endpoint}: {err} — continuing with \
                 console logging only"
            );
        }
    }
}

fn build_tracer(endpoint: &str) -> Result<opentelemetry_sdk::trace::SdkTracer, String> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
        .map_err(|err| err.to_string())?;

    let resource = Resource::builder()
        .with_service_name(service_name())
        .with_attribute(opentelemetry::KeyValue::new(
            "service.version",
            env!("CARGO_PKG_VERSION"),
        ))
        .build();

    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_resource(resource)
        .build();

    Ok(provider.tracer("ghost-link"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // Real env var mutation — serialized like every other env-var-mutating
    // test in this codebase (see auth.rs's own env_lock), since these vars
    // are process-global and `cargo test` runs in parallel by default.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn otel_endpoint_is_none_when_unset_or_whitespace_only() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_OTEL_EXPORTER_ENDPOINT");
        assert_eq!(otel_endpoint(), None);

        std::env::set_var("GHOSTLINK_OTEL_EXPORTER_ENDPOINT", "   ");
        assert_eq!(
            otel_endpoint(),
            None,
            "whitespace-only must count as unset, not a literal '   ' endpoint"
        );

        std::env::remove_var("GHOSTLINK_OTEL_EXPORTER_ENDPOINT");
    }

    #[test]
    fn otel_endpoint_returns_trimmed_value_when_set() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::set_var(
            "GHOSTLINK_OTEL_EXPORTER_ENDPOINT",
            "  http://localhost:4318  ",
        );
        assert_eq!(otel_endpoint(), Some("http://localhost:4318".to_string()));
        std::env::remove_var("GHOSTLINK_OTEL_EXPORTER_ENDPOINT");
    }

    #[test]
    fn service_name_defaults_and_honors_override() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        std::env::remove_var("GHOSTLINK_OTEL_SERVICE_NAME");
        assert_eq!(service_name(), "ghost-link");

        std::env::set_var("GHOSTLINK_OTEL_SERVICE_NAME", "my-service");
        assert_eq!(service_name(), "my-service");

        std::env::remove_var("GHOSTLINK_OTEL_SERVICE_NAME");
    }
}
