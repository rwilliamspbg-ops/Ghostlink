package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/auth"
	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/proxy"
	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/ratelimit"
)

func controlPlaneAuthToken() string {
	for _, key := range []string{"GHOSTLINK_CONTROL_PLANE_AUTH_TOKEN", "GHOSTLINK_DISCOVERY_AUTH_TOKEN"} {
		if token := strings.TrimSpace(os.Getenv(key)); token != "" {
			return token
		}
	}
	return ""
}

func requireControlPlaneAuth(w http.ResponseWriter, r *http.Request, expectedToken string) bool {
	if expectedToken == "" {
		return true
	}

	authHeader := strings.TrimSpace(r.Header.Get("Authorization"))
	if authHeader == "" {
		w.Header().Set("WWW-Authenticate", `******"Ghostlink Control Plane"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return false
	}

	const bearerPrefix = "Bearer "
	if !strings.HasPrefix(authHeader, bearerPrefix) {
		w.Header().Set("WWW-Authenticate", `******"Ghostlink Control Plane"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return false
	}

	providedToken := strings.TrimSpace(strings.TrimPrefix(authHeader, bearerPrefix))
	if providedToken != expectedToken {
		w.Header().Set("WWW-Authenticate", `******"Ghostlink Control Plane"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return false
	}

	return true
}

func getAllowedOrigins() []string {
	raw := os.Getenv("GHOSTLINK_ALLOWED_ORIGINS")
	if raw == "" {
		return []string{"*"}
	}
	parts := strings.Split(raw, ",")
	var origins []string
	for _, p := range parts {
		if trimmed := strings.TrimSpace(p); trimmed != "" {
			origins = append(origins, trimmed)
		}
	}
	if len(origins) == 0 {
		return []string{"*"}
	}
	return origins
}

func corsMiddleware(next http.Handler) http.Handler {
	allowedOrigins := getAllowedOrigins()

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")

		var allowOrigin string
		allowAll := false

		for _, o := range allowedOrigins {
			if o == "*" {
				allowAll = true
				break
			}
			if origin != "" && strings.EqualFold(o, origin) {
				allowOrigin = origin
				break
			}
		}

		if allowAll {
			allowOrigin = "*"
		}

		if allowOrigin != "" {
			w.Header().Set("Access-Control-Allow-Origin", allowOrigin)
			if allowOrigin != "*" {
				w.Header().Add("Vary", "Origin")
			}
			w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, OPTIONS")
			w.Header().Set("Access-Control-Allow-Headers", "*")
		}

		if r.Method == http.MethodOptions {
			if allowOrigin == "" && origin != "" {
				http.Error(w, "CORS origin not allowed", http.StatusForbidden)
				return
			}
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
// against a fake backend instead of a hand-rolled subset of it. apiKey may
// be empty (see auth.Middleware) — auth still applies via the proxy
// forwarding Authorization through to ghost-link either way.
func buildHandler(backendURL string, rateLimit int, rateWindow time.Duration, apiKey string, guiDistDir string) http.Handler {
	chatProxy := proxy.NewChatProxy(backendURL)
	authToken := controlPlaneAuthToken()
	withAuth := func(next http.HandlerFunc) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			if !requireControlPlaneAuth(w, r, authToken) {
				return
			}
			next(w, r)
		}
	}
	limiter := ratelimit.New(rateLimit, rateWindow)

	mux := http.NewServeMux()

	mux.HandleFunc("/v1/chat/completions", withAuth(chatProxy.HandleChatCompletions))
	// OpenAI model list + any other /v1/* path
	mux.HandleFunc("/v1/", withAuth(chatProxy.HandleBackendProxy))

	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet && r.Method != http.MethodHead {
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"status":        "ok",
			"backend":       backendURL,
			"auth_required": authToken != "",
		})
	})

	// Every other GUI route — models, settings, sessions, workers, chat,
	// MCP, metrics — proxies straight through to ghost-link's real backend.
	mux.HandleFunc("/api/", withAuth(chatProxy.HandleBackendProxy))

	// Serve the built GUI (ghostlink_gui_modern's `npm run build` output) for
	// everything else, so a release binary can run standalone without `npm
	// run dev` (which needs Node at runtime, not just at build time). Only
	// registered when the directory is actually present — the normal dev
	// flow (Vite serving the GUI on its own port) has no gui/dist at all, and
	// must keep 404ing "/" exactly as before rather than serving a stale or
	// nonexistent tree. The GUI has no client-side router (no react-router
	// dependency), so a plain file server with no SPA fallback is correct —
	// every route the GUI shows is client-state, not a distinct URL.
	if guiDistDir != "" {
		if info, err := os.Stat(guiDistDir); err == nil && info.IsDir() {
			mux.Handle("/", http.FileServer(http.Dir(guiDistDir)))
		}
	}

	// auth runs inside cors (cors already fully short-circuits OPTIONS
	// preflight before reaching anything wrapped inside it, so ordering
	// relative to preflight isn't in question here) and outside the rate
	// limiter, so an unauthenticated request gets rejected before it
	// consumes any of a client's rate-limit budget.
	return loggingMiddleware(corsMiddleware(auth.Middleware(apiKey)(limiter.Middleware(mux))))
}

// defaultGuiDistDir resolves gui/dist next to the running binary — matching
// scripts/release_bundle.sh's layout ($OUT_DIR/control-plane(.exe) sitting
// beside $OUT_DIR/gui/dist) — rather than relative to the process's cwd,
// which varies depending on how the binary is launched.
func defaultGuiDistDir() string {
	exePath, err := os.Executable()
	if err != nil {
		return ""
	}
	return filepath.Join(filepath.Dir(exePath), "gui", "dist")
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

	apiKey, err := auth.LoadAPIKey()
	if err != nil {
		log.Printf("warning: could not read ghost-link's API key (%v) — the gateway won't reject unauthenticated requests itself; ghost-link's own auth still applies to proxied traffic", err)
		apiKey = ""
	}

	guiDistDir := os.Getenv("GHOSTLINK_GUI_DIST_DIR")
	if guiDistDir == "" {
		guiDistDir = defaultGuiDistDir()
	}

	handler := buildHandler(backendURL, rateLimit, time.Duration(rateWindowSec)*time.Second, apiKey, guiDistDir)

	log.Printf("Ghostlink Control Plane starting on :%s", port)
	log.Printf("Proxying GUI/API routes to backend: %s", backendURL)
	log.Printf("Rate limit: %d requests / %ds per client", rateLimit, rateWindowSec)
	if info, err := os.Stat(guiDistDir); err == nil && info.IsDir() {
		log.Printf("Serving built GUI from: %s", guiDistDir)
	} else {
		log.Printf("No built GUI found at %s — run the Vite dev server for the GUI, or `npm run build` + set GHOSTLINK_GUI_DIST_DIR for a standalone binary", guiDistDir)
	}
	if err := http.ListenAndServe(":"+port, handler); err != nil {
		log.Fatalf("Server failed: %v", err)
	}
}
