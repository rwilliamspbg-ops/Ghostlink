package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/rwilliamspbg-ops/Ghostlink/control-plane/pkg/registry"
)

func setupTestServer() (*registry.Registry, *http.ServeMux) {
	reg := registry.NewRegistry()
	mux := http.NewServeMux()

	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"status":  "ok",
			"workers": reg.Summary(),
		})
	})

	mux.HandleFunc("/api/workers", func(w http.ResponseWriter, r *http.Request) {
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
	})

	return reg, mux
}

func TestHealthEndpoint(t *testing.T) {
	_, mux := setupTestServer()

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
}

func TestWorkerRegisterAndList(t *testing.T) {
	reg, mux := setupTestServer()

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
	reg, _ := setupTestServer()

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
	reg, _ := setupTestServer()

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
	reg, _ := setupTestServer()

	reg.Register(&registry.Worker{ID: "node-old", Hostname: "old-host", Port: 9000})

	time.Sleep(10 * time.Millisecond)
	reg.Cleanup(5 * time.Millisecond)

	workers := reg.List()
	if len(workers) != 0 {
		t.Fatalf("expected 0 workers after cleanup, got %d", len(workers))
	}
}

func TestRegistrySummary(t *testing.T) {
	reg, _ := setupTestServer()

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
