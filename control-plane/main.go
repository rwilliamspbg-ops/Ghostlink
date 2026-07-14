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

	http.HandleFunc("/v1/chat/completions", chatProxy.HandleChatCompletions)
	http.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"status":  "ok",
			"workers": reg.Summary(),
		})
	})

	http.HandleFunc("/api/workers", func(w http.ResponseWriter, r *http.Request) {
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
		default:
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		}
	})

	http.HandleFunc("/api/workers/", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete {
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		id := strings.TrimPrefix(r.URL.Path, "/api/workers/")
		if id == "" {
			http.Error(w, "worker id required", http.StatusBadRequest)
			return
		}
		ok := reg.Deregister(id)
		w.Header().Set("Content-Type", "application/json")
		if ok {
			json.NewEncoder(w).Encode(map[string]string{"status": "ok", "id": id})
		} else {
			w.WriteHeader(http.StatusNotFound)
			json.NewEncoder(w).Encode(map[string]string{"error": "worker not found"})
		}
	})

	http.HandleFunc("/api/workers/heartbeat", func(w http.ResponseWriter, r *http.Request) {
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
			json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
		} else {
			w.WriteHeader(http.StatusNotFound)
			json.NewEncoder(w).Encode(map[string]string{"error": "worker not found"})
		}
	})

	go func() {
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()
		for range ticker.C {
			reg.Cleanup(90 * time.Second)
		}
	}()

	log.Printf("Ghostlink Control Plane starting on :%s", port)
	log.Printf("Proxying to backend: %s", backendURL)
	if err := http.ListenAndServe(":"+port, nil); err != nil {
		log.Fatalf("Server failed: %v", err)
	}
}
