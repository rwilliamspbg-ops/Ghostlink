package main

import (
	"log"
	"net/http"
	"os"

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
	_ = reg

	chatProxy := proxy.NewChatProxy(backendURL)

	http.HandleFunc("/v1/chat/completions", chatProxy.HandleChatCompletions)
	http.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("OK"))
	})

	log.Printf("Ghostlink Control Plane starting on :%s", port)
	log.Printf("Proxying to backend: %s", backendURL)
	if err := http.ListenAndServe(":"+port, nil); err != nil {
		log.Fatalf("Server failed: %v", err)
	}
}
