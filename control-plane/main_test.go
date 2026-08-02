package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/registry"
)

func setupTestServer(authToken string) (*registry.Registry, *http.ServeMux) {
	reg := registry.NewRegistry()
	mux := http.NewServeMux()
	withAuth := func(next http.HandlerFunc) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			if !requireControlPlaneAuth(w, r, authToken) {
				return
			}
			next(w, r)
		}
	}

	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"status":  "ok",
			"workers": reg.Summary(),
			"auth_required": authToken != "",
		})
	})

	mux.HandleFunc("/api/workers", withAuth(func(w http.ResponseWriter, r *http.Request) {
		switch r.Method {
		case http.MethodGet:
			workers := reg.List()
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]any{"workers": workers})
		case http.MethodPost:
			var worker registry.Worker
			if err := json.NewDecoder(r.Body).Decode(&worker); err != nil {
				http.Error(w, "invalid request body", http.StatusBadRequest)
				return
			}
			reg.Register(&worker)
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]string{"status": "ok", "id": worker.ID})
		}
	}))

	mux.HandleFunc("/api/workers/", withAuth(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		id := strings.TrimPrefix(r.URL.Path, "/api/workers/")
		if id == "" || strings.Contains(id, "/") {
			http.Error(w, "worker id required", http.StatusBadRequest)
			return
		}
		if reg.Deregister(id) {
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]string{"status": "ok", "id": id})
			return
		}
		w.WriteHeader(http.StatusNotFound)
		json.NewEncoder(w).Encode(map[string]string{"error": "worker not found"})
	}))

	return reg, mux
}

func TestHealthEndpoint(t *testing.T) {
	_, mux := setupTestServer("")

	req := httptest.NewRequest("GET", "/health", nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}

	var resp map[string]any
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("invalid JSON response: %v", err)
	}

	if resp["status"] != "ok" {
		t.Errorf("expected status=ok, got %v", resp["status"])
	}
	if resp["auth_required"] != false {
		t.Errorf("expected auth_required=false, got %v", resp["auth_required"])
	}
}

func TestWorkerRegisterAndList(t *testing.T) {
	reg, mux := setupTestServer("")

	worker := registry.Worker{
		ID:       "test-node-1",
		Hostname: "test-host",
		Port:     8080,
		VRAMGB:   16.0,
	}
	body, _ := json.Marshal(worker)

	req := httptest.NewRequest("POST", "/api/workers", bytes.NewReader(body))
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("register: expected 200, got %d", w.Code)
	}

	workers := reg.List()
	if len(workers) != 1 {
		t.Fatalf("expected 1 worker, got %d", len(workers))
	}
	if workers[0].ID != "test-node-1" {
		t.Errorf("expected worker ID test-node-1, got %s", workers[0].ID)
	}

	req = httptest.NewRequest("GET", "/api/workers", nil)
	w = httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("list: expected 200, got %d", w.Code)
	}
}

func TestWorkerHeartbeat(t *testing.T) {
	reg, _ := setupTestServer("")

	reg.Register(&registry.Worker{ID: "node-hb", Hostname: "hb-host", Port: 9000})

	ok := reg.Heartbeat("node-hb", 45.0, 60.0, 80.0)
	if !ok {
		t.Fatal("heartbeat should succeed for registered worker")
	}

	ok = reg.Heartbeat("nonexistent", 0, 0, 0)
	if ok {
		t.Fatal("heartbeat should fail for unregistered worker")
	}
}

func TestWorkerDeregister(t *testing.T) {
	reg, _ := setupTestServer("")

	reg.Register(&registry.Worker{ID: "node-dr", Hostname: "dr-host", Port: 9000})

	ok := reg.Deregister("node-dr")
	if !ok {
		t.Fatal("deregister should succeed for registered worker")
	}

	workers := reg.List()
	if len(workers) != 0 {
		t.Fatalf("expected 0 workers after deregister, got %d", len(workers))
	}

	ok = reg.Deregister("nonexistent")
	if ok {
		t.Fatal("deregister should fail for unregistered worker")
	}
}

func TestWorkerCleanup(t *testing.T) {
	reg, _ := setupTestServer("")

	reg.Register(&registry.Worker{ID: "node-old", Hostname: "old-host", Port: 9000})

	time.Sleep(10 * time.Millisecond)
	reg.Cleanup(5 * time.Millisecond)

	workers := reg.List()
	if len(workers) != 0 {
		t.Fatalf("expected 0 workers after cleanup, got %d", len(workers))
	}
}

func TestRegistrySummary(t *testing.T) {
	reg, _ := setupTestServer("")

	reg.Register(&registry.Worker{ID: "n1", Hostname: "h1", Port: 9000})
	reg.Register(&registry.Worker{ID: "n2", Hostname: "h2", Port: 9001})

	summary := reg.Summary()
	if summary["total"] != 2 {
		t.Errorf("expected total=2, got %d", summary["total"])
	}
	if summary["online"] != 2 {
		t.Errorf("expected online=2, got %d", summary["online"])
	}
}

func TestControlPlaneMutationsRequireBearerToken(t *testing.T) {
	const token = "test-control-plane-token"
	_, mux := setupTestServer(token)

	body, _ := json.Marshal(registry.Worker{ID: "secured-node", Hostname: "secure-host", Port: 9000})
	createReq := httptest.NewRequest("POST", "/api/workers", bytes.NewReader(body))
	createResp := httptest.NewRecorder()
	mux.ServeHTTP(createResp, createReq)
	if createResp.Code != http.StatusUnauthorized {
		t.Fatalf("expected unauthenticated POST to be rejected, got %d", createResp.Code)
	}

	createReq = httptest.NewRequest("POST", "/api/workers", bytes.NewReader(body))
	createReq.Header.Set("Authorization", "Bearer "+token)
	createResp = httptest.NewRecorder()
	mux.ServeHTTP(createResp, createReq)
	if createResp.Code != http.StatusOK {
		t.Fatalf("expected authenticated POST to succeed, got %d", createResp.Code)
	}

	listReq := httptest.NewRequest("GET", "/api/workers", nil)
	listResp := httptest.NewRecorder()
	mux.ServeHTTP(listResp, listReq)
	if listResp.Code != http.StatusUnauthorized {
		t.Fatalf("expected unauthenticated GET to be rejected, got %d", listResp.Code)
	}

	deleteReq := httptest.NewRequest("DELETE", "/api/workers/secured-node", nil)
	deleteResp := httptest.NewRecorder()
	mux.ServeHTTP(deleteResp, deleteReq)
	if deleteResp.Code != http.StatusUnauthorized {
		t.Fatalf("expected unauthenticated DELETE to be rejected, got %d", deleteResp.Code)
	}

	healthReq := httptest.NewRequest("GET", "/health", nil)
	healthResp := httptest.NewRecorder()
	mux.ServeHTTP(healthResp, healthReq)
	if healthResp.Code != http.StatusOK {
		t.Fatalf("expected health endpoint to stay open, got %d", healthResp.Code)
	}

	var healthPayload map[string]any
	if err := json.Unmarshal(healthResp.Body.Bytes(), &healthPayload); err != nil {
		t.Fatalf("invalid health payload: %v", err)
	}
	if healthPayload["auth_required"] != true {
		t.Fatalf("expected auth_required=true, got %v", healthPayload["auth_required"])
	}
}

func TestControlPlaneTokenFallbackUsesDiscoveryToken(t *testing.T) {
	const token = "fallback-token"
	if err := os.Setenv("GHOSTLINK_DISCOVERY_AUTH_TOKEN", token); err != nil {
		t.Fatalf("set env: %v", err)
	}
	t.Cleanup(func() {
		_ = os.Unsetenv("GHOSTLINK_DISCOVERY_AUTH_TOKEN")
	})

	if got := controlPlaneAuthToken(); got != token {
		t.Fatalf("expected discovery token fallback, got %q", got)
	}
}
