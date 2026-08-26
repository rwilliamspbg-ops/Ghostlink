package proxy

import (
	"bytes"
	"crypto/tls"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
)

type ChatProxy struct {
	BackendURL string
	Client     *http.Client
}

// corsHeaders are owned by the gateway's corsMiddleware, not the upstream
// backend — see the skip in forward() below.
var corsHeaders = map[string]bool{
	http.CanonicalHeaderKey("Access-Control-Allow-Origin"):      true,
	http.CanonicalHeaderKey("Access-Control-Allow-Methods"):     true,
	http.CanonicalHeaderKey("Access-Control-Allow-Headers"):     true,
	http.CanonicalHeaderKey("Access-Control-Allow-Credentials"): true,
	http.CanonicalHeaderKey("Access-Control-Expose-Headers"):    true,
}

func NewChatProxy(backendURL string) *ChatProxy {
	client := &http.Client{}
	parsedURL, parseErr := url.Parse(backendURL)
	if parseErr == nil && strings.EqualFold(parsedURL.Scheme, "https") {
		host := parsedURL.Hostname()
		ip := net.ParseIP(host)
		isLoopbackHost := strings.EqualFold(host, "localhost") || (ip != nil && ip.IsLoopback())

		// ghost-link's HTTPS listener (enabled via the persisted settings.json
		// "enable_tls" flag) uses a self-signed loopback cert with no CA chain
		// a Go client would trust, so the default transport's cert validation
		// would fail every request. This gateway only ever talks to ghost-link
		// over loopback, so skipping verification here doesn't expose it to a
		// real network MITM risk.
		if isLoopbackHost {
			client.Transport = &http.Transport{
				TLSClientConfig: &tls.Config{InsecureSkipVerify: true},
			}
		}
	}
	return &ChatProxy{
		BackendURL: strings.TrimRight(backendURL, "/"),
		Client:     client,
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
		if corsHeaders[k] {
			// The gateway's own corsMiddleware already set these on w via
			// Set() before we got here. ghost-link sets its own permissive
			// CORS headers too (same-origin callers hitting it directly
			// still need them), so blindly copying would Add() a second
			// value onto an already-Set() header — e.g. two
			// Access-Control-Allow-Origin values, which browsers treat as
			// an invalid CORS response and hard-fail the request even
			// though curl/server-to-server callers don't care.
			continue
		}
		for _, vv := range v {
			w.Header().Add(k, vv)
		}
	}
	w.WriteHeader(resp.StatusCode)

	// Flush after every chunk instead of a single buffered io.Copy. Without
	// this, SSE token-by-token chat streaming (and the Ollama pull-progress
	// stream) would sit in Go's default write buffer until it filled or the
	// response ended — silently turning real-time streaming back into a
	// long wait-then-dump for anything routed through this proxy.
	flusher, canFlush := w.(http.Flusher)
	buf := make([]byte, 4096)
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			if _, writeErr := w.Write(buf[:n]); writeErr != nil {
				return
			}
			if canFlush {
				flusher.Flush()
			}
		}
		if readErr != nil {
			return
		}
	}
}
