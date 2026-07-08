package proxy

import (
	"bytes"
	"io"
	"net/http"
)

type ChatProxy struct {
	BackendURL string
}

func NewChatProxy(backendURL string) *ChatProxy {
	return &ChatProxy{
		BackendURL: backendURL,
	}
}

func (p *ChatProxy) HandleChatCompletions(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "Failed to read request body", http.StatusBadRequest)
		return
	}

	req, err := http.NewRequest(http.MethodPost, p.BackendURL+"/v1/chat/completions", bytes.NewBuffer(body))
	if err != nil {
		http.Error(w, "Failed to create backend request", http.StatusInternalServerError)
		return
	}
	req.Header = r.Header

	client := &http.Client{}
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
	io.Copy(w, resp.Body)
}
