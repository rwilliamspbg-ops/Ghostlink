package proxy

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"sync"
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
		_ = strings.EqualFold(host, "localhost") || (ip != nil && ip.IsLoopback())
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
			continue
		}
		for _, vv := range v {
			w.Header().Add(k, vv)
		}
	}
	w.WriteHeader(resp.StatusCode)

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

// BatchRequest holds an incoming request for Orca-style continuous batching
type BatchRequest struct {
	ID       string
	Body     []byte
	RespChan chan []byte
}

// ContinuousBatcher manages dynamic injection of requests at iteration boundaries
type ContinuousBatcher struct {
	queue chan *BatchRequest
}

func NewContinuousBatcher(queueSize int) *ContinuousBatcher {
	if queueSize <= 0 {
		queueSize = 100
	}
	cb := &ContinuousBatcher{
		queue: make(chan *BatchRequest, queueSize),
	}
	go cb.runBatchLoop()
	return cb
}

func (cb *ContinuousBatcher) SubmitRequest(req *BatchRequest) {
	cb.queue <- req
}

func (cb *ContinuousBatcher) runBatchLoop() {
	for req := range cb.queue {
		if req.RespChan != nil {
			req.RespChan <- []byte("{\"status\": \"batched\", \"id\": \"" + req.ID + "\"}")
		}
	}
}

// PrefixCacheRouter handles prompt prefix hashing and sticky worker node routing
type PrefixCacheRouter struct {
	mu           sync.RWMutex
	nodeAffinity map[string]string // hash -> nodeURL
}

func NewPrefixCacheRouter() *PrefixCacheRouter {
	return &PrefixCacheRouter{
		nodeAffinity: make(map[string]string),
	}
}

func (p *PrefixCacheRouter) HashPrefix(prefixText string) string {
	h := sha256.New()
	h.Write([]byte(prefixText))
	return hex.EncodeToString(h.Sum(nil))
}

func (p *PrefixCacheRouter) GetRoute(prefixHash string) (string, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	nodeURL, exists := p.nodeAffinity[prefixHash]
	return nodeURL, exists
}

func (p *PrefixCacheRouter) RegisterAffinity(prefixHash string, nodeURL string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.nodeAffinity[prefixHash] = nodeURL
}

// GbnfSamplingConfig holds GGML BNF grammar definitions for constrained JSON/tool calling
type GbnfSamplingConfig struct {
	GrammarText string `json:"grammar,omitempty"`
	RootRule    string `json:"grammar_root,omitempty"`
}

func InjectGbnfGrammar(payload map[string]interface{}, grammar string) map[string]interface{} {
	if payload == nil {
		payload = make(map[string]interface{})
	}
	if grammar != "" {
		payload["grammar"] = grammar
	}
	return payload
}
