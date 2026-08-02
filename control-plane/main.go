package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/proxy"
	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/registry"
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
		w.Header().Set("WWW-Authenticate", `Bearer realm="Ghostlink Control Plane"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return false
	}

	const bearerPrefix = "Bearer "
	if !strings.HasPrefix(authHeader, bearerPrefix) {
		w.Header().Set("WWW-Authenticate", `Bearer realm="Ghostlink Control Plane"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return false
	}

	providedToken := strings.TrimSpace(strings.TrimPrefix(authHeader, bearerPrefix))
	if providedToken != expectedToken {
		w.Header().Set("WWW-Authenticate", `Bearer realm="Ghostlink Control Plane"`)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return false
	}

	return true
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

	reg := registry.NewRegistry()
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
			"status":  "ok",
			"backend": backendURL,
			"workers": reg.Summary(),
			"auth_required": authToken != "",
		})
	})

	// Worker registry (local to control-plane)
	mux.HandleFunc("/api/workers", withAuth(func(w http.ResponseWriter, r *http.Request) {
		switch r.Method {
		case http.MethodGet:
			workers := reg.List()
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{"workers": workers})
		case http.MethodPost:
			var worker registry.Worker
			if err := json.NewDecoder(r.Body).Decode(&worker); err != nil {
				http.Error(w, "invalid request body", http.StatusBadRequest)
				return
			}
			reg.Register(&worker)
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok", "id": worker.ID})
		default:
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		}
	}))

	mux.HandleFunc("/api/workers/", withAuth(func(w http.ResponseWriter, r *http.Request) {
		path := r.URL.Path
		if strings.HasSuffix(path, "/heartbeat") || path == "/api/workers/heartbeat" {
			if r.Method != http.MethodPost {
				http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
				return
			}
			var req struct {
				ID  string  `json:"id"`
				CPU float32 `json:"cpu_usage"`
				Mem float32 `json:"memory_usage"`
				GPU float32 `json:"gpu_usage"`
			}
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				http.Error(w, "invalid request body", http.StatusBadRequest)
				return
			}
			ok := reg.Heartbeat(req.ID, req.CPU, req.Mem, req.GPU)
			w.Header().Set("Content-Type", "application/json")
			if ok {
				_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
			} else {
				w.WriteHeader(http.StatusNotFound)
				_ = json.NewEncoder(w).Encode(map[string]string{"error": "worker not found"})
			}
			return
		}

		if r.Method != http.MethodDelete {
			// Fall through: proxy model/load/etc. style paths under /api/workers/* that
			// belong to the backend (e.g. disconnect) — only pure DELETE is local deregister
			// when the path is exactly /api/workers/{id}
			id := strings.TrimPrefix(path, "/api/workers/")
			if id == "" || strings.Contains(id, "/") {
				chatProxy.HandleBackendProxy(w, r)
				return
			}
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		id := strings.TrimPrefix(path, "/api/workers/")
		if id == "" || strings.Contains(id, "/") {
			http.Error(w, "worker id required", http.StatusBadRequest)
			return
		}
		ok := reg.Deregister(id)
		w.Header().Set("Content-Type", "application/json")
		if ok {
			_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok", "id": id})
		} else {
			w.WriteHeader(http.StatusNotFound)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": "worker not found"})
		}
	}))

	// Proxy all other /api/* GUI routes to ghost-link (models, settings, chat, ...)
	mux.HandleFunc("/api/", withAuth(chatProxy.HandleBackendProxy))

	go func() {
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()
		for range ticker.C {
			reg.Cleanup(90 * time.Second)
		}
	}()

	log.Printf("Ghostlink Control Plane starting on :%s", port)
	log.Printf("Proxying GUI/API routes to backend: %s", backendURL)
	if err := http.ListenAndServe(":"+port, mux); err != nil {
		log.Fatalf("Server failed: %v", err)
	}
}
