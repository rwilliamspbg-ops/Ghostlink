package proxy

import (
	"bytes"
	"io"
	"net/http"
	"strings"
)

type ChatProxy struct {
	BackendURL string
	Client     *http.Client
}

func NewChatProxy(backendURL string) *ChatProxy {
	return &ChatProxy{
		BackendURL: strings.TrimRight(backendURL, "/"),
		Client:     &http.Client{},
	}
}

func (p *ChatProxy) HandleChatCompletions(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	p.forward(w, r, "/v1/chat/completions")
}

// HandleBackendProxy reverse-proxies GUI/API paths to ghost-link.
// Prevents 404/405 when clients accidentally target the control-plane port
// for /api/models, /api/settings, /api/inference/chat, etc.
func (p *ChatProxy) HandleBackendProxy(w http.ResponseWriter, r *http.Request) {
	p.forward(w, r, r.URL.RequestURI())
}

func (p *ChatProxy) forward(w http.ResponseWriter, r *http.Request, path string) {
	if !strings.HasPrefix(path, "/") {
		path = "/" + path
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "Failed to read request body", http.StatusBadRequest)
		return
	}

	url := p.BackendURL + path
	req, err := http.NewRequest(r.Method, url, bytes.NewReader(body))
	if err != nil {
		http.Error(w, "Failed to create backend request", http.StatusInternalServerError)
		return
	}
	req.Header = r.Header.Clone()

	client := p.Client
	if client == nil {
		client = http.DefaultClient
	}
	resp, err := client.Do(req)
	if err != nil {
		http.Error(w, "Backend unreachable", http.StatusServiceUnavailable)
		return
	}
	defer resp.Body.Close()

	for k, v := range resp.Header {
		for _, vv := range v {
			w.Header().Add(k, vv)
		}
	}
	w.WriteHeader(resp.StatusCode)
	_, _ = io.Copy(w, resp.Body)
}
