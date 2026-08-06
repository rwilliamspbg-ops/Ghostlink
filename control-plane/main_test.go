package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"
)

func TestHealthEndpoint(t *testing.T) {
	handler := buildHandler("http://127.0.0.1:19999", 1000, time.Second, "", "")

	req := httptest.NewRequest("GET", "/health", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

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
	if resp["backend"] != "http://127.0.0.1:19999" {
		t.Errorf("expected backend to be reported, got %v", resp["backend"])
	}
	if resp["auth_required"] != false {
		t.Errorf("expected auth_required=false, got %v", resp["auth_required"])
	}
}

func TestHealthRejectsPost(t *testing.T) {
	handler := buildHandler("http://127.0.0.1:19999", 1000, time.Second, "", "")

	req := httptest.NewRequest("POST", "/health", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusMethodNotAllowed {
		t.Fatalf("expected 405, got %d", w.Code)
	}
}

func TestWorkersProxiesToBackend(t *testing.T) {
	backend := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/workers" {
			t.Errorf("expected backend to receive /api/workers, got %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"workers":[{"id":"real-node-1"}]}`))
	}))
	defer backend.Close()

	handler := buildHandler(backend.URL, 1000, time.Second, "", "")

	req := httptest.NewRequest("GET", "/api/workers", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
	var resp map[string]any
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("invalid JSON response: %v", err)
	}
	workers, ok := resp["workers"].([]any)
	if !ok || len(workers) != 1 {
		t.Fatalf("expected the backend's real worker list to pass through, got %v", resp)
	}
}

func TestCORSHeaders(t *testing.T) {
	handler := buildHandler("http://127.0.0.1:19999", 1000, time.Second, "", "")

	req := httptest.NewRequest("OPTIONS", "/api/models", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusNoContent {
		t.Fatalf("expected 204 for OPTIONS preflight, got %d", w.Code)
	}
	if got := w.Header().Get("Access-Control-Allow-Origin"); got != "*" {
		t.Errorf("expected permissive CORS origin, got %q", got)
	}
}

func TestRateLimitingBlocksExcessRequests(t *testing.T) {
	backend := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	defer backend.Close()

	handler := buildHandler(backend.URL, 2, time.Minute, "", "")

	makeReq := func() int {
		req := httptest.NewRequest("GET", "/health", nil)
		req.RemoteAddr = "10.0.0.1:1234"
		w := httptest.NewRecorder()
		handler.ServeHTTP(w, req)
		return w.Code
	}

	if code := makeReq(); code != http.StatusOK {
		t.Fatalf("request 1: expected 200, got %d", code)
	}
	if code := makeReq(); code != http.StatusOK {
		t.Fatalf("request 2: expected 200, got %d", code)
	}
	if code := makeReq(); code != http.StatusTooManyRequests {
		t.Fatalf("request 3: expected 429 (over limit), got %d", code)
	}
}

func TestRateLimitingIsolatesByClient(t *testing.T) {
	handler := buildHandler("http://127.0.0.1:19999", 1, time.Minute, "", "")

	req1 := httptest.NewRequest("GET", "/health", nil)
	req1.RemoteAddr = "10.0.0.1:1111"
	w1 := httptest.NewRecorder()
	handler.ServeHTTP(w1, req1)

	req2 := httptest.NewRequest("GET", "/health", nil)
	req2.RemoteAddr = "10.0.0.2:2222"
	w2 := httptest.NewRecorder()
	handler.ServeHTTP(w2, req2)

	if w1.Code != http.StatusOK || w2.Code != http.StatusOK {
		t.Fatalf("distinct clients should each get their own limit: client1=%d client2=%d", w1.Code, w2.Code)
	}
}

func TestStreamingResponsesAreFlushed(t *testing.T) {
	backend := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		flusher, ok := w.(http.Flusher)
		if !ok {
			t.Fatal("test backend's ResponseWriter must support flushing")
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("data: {\"token\":\"hel\"}\n\n"))
		flusher.Flush()
		_, _ = w.Write([]byte("data: {\"token\":\"lo\"}\n\n"))
		flusher.Flush()
	}))
	defer backend.Close()

	handler := buildHandler(backend.URL, 1000, time.Second, "", "")

	req := httptest.NewRequest("POST", "/api/inference/chat", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", w.Code)
	}
	if !w.Flushed {
		t.Fatal("proxy must flush the response writer for streaming bodies, but Flushed is false")
	}
	if w.Body.String() == "" {
		t.Fatal("expected streamed body content to reach the client")
	}
}

func TestControlPlaneMutationsRequireBearerToken(t *testing.T) {
	const token = "test-control-plane-token"
	if err := os.Setenv("GHOSTLINK_CONTROL_PLANE_AUTH_TOKEN", token); err != nil {
		t.Fatalf("set env: %v", err)
	}
	t.Cleanup(func() {
		_ = os.Unsetenv("GHOSTLINK_CONTROL_PLANE_AUTH_TOKEN")
	})

	backend := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer backend.Close()

	handler := buildHandler(backend.URL, 1000, time.Second, "", "")

	body := []byte(`{"id":"secured-node"}`)
	createReq := httptest.NewRequest("POST", "/api/workers", bytes.NewReader(body))
	createResp := httptest.NewRecorder()
	handler.ServeHTTP(createResp, createReq)
	if createResp.Code != http.StatusUnauthorized {
		t.Fatalf("expected unauthenticated POST to be rejected, got %d", createResp.Code)
	}

	createReq = httptest.NewRequest("POST", "/api/workers", bytes.NewReader(body))
	createReq.Header.Set("Authorization", "Bearer "+token)
	createResp = httptest.NewRecorder()
	handler.ServeHTTP(createResp, createReq)
	if createResp.Code != http.StatusOK {
		t.Fatalf("expected authenticated POST to succeed, got %d", createResp.Code)
	}

	listReq := httptest.NewRequest("GET", "/api/workers", nil)
	listResp := httptest.NewRecorder()
	handler.ServeHTTP(listResp, listReq)
	if listResp.Code != http.StatusUnauthorized {
		t.Fatalf("expected unauthenticated GET to be rejected, got %d", listResp.Code)
	}

	deleteReq := httptest.NewRequest("DELETE", "/api/workers/secured-node", nil)
	deleteResp := httptest.NewRecorder()
	handler.ServeHTTP(deleteResp, deleteReq)
	if deleteResp.Code != http.StatusUnauthorized {
		t.Fatalf("expected unauthenticated DELETE to be rejected, got %d", deleteResp.Code)
	}

	healthReq := httptest.NewRequest("GET", "/health", nil)
	healthResp := httptest.NewRecorder()
	handler.ServeHTTP(healthResp, healthReq)
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

func TestServesBuiltGUIWhenDistDirPresent(t *testing.T) {
	distDir := t.TempDir()
	if err := os.WriteFile(distDir+"/index.html", []byte("<html>ghostlink studio</html>"), 0o644); err != nil {
		t.Fatalf("write fixture index.html: %v", err)
	}

	handler := buildHandler("http://127.0.0.1:19999", 1000, time.Second, "", distDir)

	req := httptest.NewRequest("GET", "/", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("expected 200 serving built GUI, got %d", w.Code)
	}
	if w.Body.String() != "<html>ghostlink studio</html>" {
		t.Fatalf("expected fixture index.html content, got %q", w.Body.String())
	}
}

func TestNoStaticServingWhenDistDirAbsent(t *testing.T) {
	handler := buildHandler("http://127.0.0.1:19999", 1000, time.Second, "", "/nonexistent/gui/dist")

	req := httptest.NewRequest("GET", "/", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound {
		t.Fatalf("expected 404 when no built GUI is present (dev flow unaffected), got %d", w.Code)
	}
}
