package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"strconv"
	"time"

	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/proxy"
	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/ratelimit"
)

// corsMiddleware mirrors ghost-link's tower_http::cors::CorsLayer::permissive()
// — the GUI calls this gateway's absolute URL cross-origin (a different port
// than the Vite dev server), exactly like it already does to ghost-link
// directly today, so the same permissive stance is needed here too.
func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "*")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// loggingResponseWriter captures the status code for the access log line.
// It must forward Flush() to the underlying writer — proxy.forward relies on
// http.Flusher for real-time streaming, and wrapping the ResponseWriter
// without preserving that interface would silently break streaming again.
type loggingResponseWriter struct {
	http.ResponseWriter
	statusCode int
}

func (lrw *loggingResponseWriter) WriteHeader(code int) {
	lrw.statusCode = code
	lrw.ResponseWriter.WriteHeader(code)
}

func (lrw *loggingResponseWriter) Flush() {
	if f, ok := lrw.ResponseWriter.(http.Flusher); ok {
		f.Flush()
	}
}

func loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		lrw := &loggingResponseWriter{ResponseWriter: w, statusCode: http.StatusOK}
		next.ServeHTTP(lrw, r)
		log.Printf("%s %s %d %s", r.Method, r.URL.Path, lrw.statusCode, time.Since(start).Round(time.Millisecond))
	})
}

func envInt(key string, def int) int {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		return def
	}
	return n
}

// buildHandler wires the full route table + middleware chain. Factored out
// of main() so tests can exercise the exact same routing/middleware setup
// against a fake backend instead of a hand-rolled subset of it.
func buildHandler(backendURL string, rateLimit int, rateWindow time.Duration) http.Handler {
	chatProxy := proxy.NewChatProxy(backendURL)
	limiter := ratelimit.New(rateLimit, rateWindow)

	mux := http.NewServeMux()

	mux.HandleFunc("/v1/chat/completions", chatProxy.HandleChatCompletions)
	// OpenAI model list + any other /v1/* path
	mux.HandleFunc("/v1/", chatProxy.HandleBackendProxy)

	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet && r.Method != http.MethodHead {
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"status":  "ok",
			"backend": backendURL,
		})
	})

	// Every other GUI route — models, settings, sessions, workers, chat,
	// MCP, metrics — proxies straight through to ghost-link. Workers used to
	// be handled by a local in-memory registry here, but that registry had
	// no knowledge of ghost-link's real UDP peer discovery / cluster state,
	// so it was a second, disconnected source of truth. Removed in favor of
	// always deferring to ghost-link's actual implementation.
	mux.HandleFunc("/api/", chatProxy.HandleBackendProxy)

	return loggingMiddleware(corsMiddleware(limiter.Middleware(mux)))
}

func main() {
	port := os.Getenv("PORT")
	if port == "" {
		port = "8000"
	}

	backendURL := os.Getenv("GHOSTLINK_BACKEND_URL")
	if backendURL == "" {
		backendURL = "http://127.0.0.1:8003"
	}

	// Generous enough not to disrupt normal use (App.tsx polls metrics every
	// 3s, workers every 15s, health every 30s — a handful of requests per
	// window under normal operation) while still catching a genuinely
	// runaway client. Env-overridable for tuning without a rebuild.
	rateLimit := envInt("GHOSTLINK_RATE_LIMIT", 120)
	rateWindowSec := envInt("GHOSTLINK_RATE_WINDOW_SECONDS", 10)

	handler := buildHandler(backendURL, rateLimit, time.Duration(rateWindowSec)*time.Second)

	log.Printf("Ghostlink Control Plane starting on :%s", port)
	log.Printf("Proxying GUI/API routes to backend: %s", backendURL)
	log.Printf("Rate limit: %d requests / %ds per client", rateLimit, rateWindowSec)
	if err := http.ListenAndServe(":"+port, handler); err != nil {
		log.Fatalf("Server failed: %v", err)
	}
}
