package proxy

import (
	"testing"
	"time"
)

func TestContinuousBatcher(t *testing.T) {
	cb := NewContinuousBatcher(10)
	respChan := make(chan []byte, 1)

	req := &BatchRequest{
		ID:       "req-1",
		Body:     []byte(`{"prompt": "hello"}`),
		RespChan: respChan,
	}

	cb.SubmitRequest(req)

	select {
	case resp := <-respChan:
		if len(resp) == 0 {
			t.Fatal("expected non-empty response from continuous batcher")
		}
	case <-time.After(1 * time.Second):
		t.Fatal("timed out waiting for continuous batcher response")
	}
}

func TestPrefixCacheRouter(t *testing.T) {
	router := NewPrefixCacheRouter()
	systemPrompt := "You are a helpful assistant."
	hash := router.HashPrefix(systemPrompt)

	if len(hash) == 0 {
		t.Fatal("expected non-empty hash for system prompt")
	}

	_, exists := router.GetRoute(hash)
	if exists {
		t.Fatal("expected no affinity before registration")
	}

	nodeURL := "http://10.0.0.5:8003"
	router.RegisterAffinity(hash, nodeURL)

	routed, exists := router.GetRoute(hash)
	if !exists || routed != nodeURL {
		t.Fatalf("expected nodeURL %s, got %s (exists=%v)", nodeURL, routed, exists)
	}
}

func TestInjectGbnfGrammar(t *testing.T) {
	payload := map[string]interface{}{"model": "llama3"}
	grammar := "root ::= [a-z]+"
	res := InjectGbnfGrammar(payload, grammar)

	if res["grammar"] != grammar {
		t.Fatalf("expected grammar %s, got %v", grammar, res["grammar"])
	}
}
