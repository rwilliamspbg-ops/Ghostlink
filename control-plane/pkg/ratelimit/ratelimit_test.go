package ratelimit

import (
	"net/http/httptest"
	"testing"
	"time"
)

func TestAllowUnderLimit(t *testing.T) {
	l := New(3, time.Minute)
	for i := 0; i < 3; i++ {
		if !l.Allow("client-a") {
			t.Fatalf("request %d should be allowed under a limit of 3", i+1)
		}
	}
}

func TestAllowBlocksOverLimit(t *testing.T) {
	l := New(2, time.Minute)
	l.Allow("client-a")
	l.Allow("client-a")
	if l.Allow("client-a") {
		t.Fatal("3rd request should be blocked at a limit of 2")
	}
}

func TestAllowIsolatesClients(t *testing.T) {
	l := New(1, time.Minute)
	if !l.Allow("client-a") {
		t.Fatal("client-a's first request should be allowed")
	}
	if !l.Allow("client-b") {
		t.Fatal("client-b should have its own independent limit")
	}
	if l.Allow("client-a") {
		t.Fatal("client-a's second request should still be blocked")
	}
}

func TestAllowResetsAfterWindowExpires(t *testing.T) {
	l := New(1, 20*time.Millisecond)
	if !l.Allow("client-a") {
		t.Fatal("first request should be allowed")
	}
	if l.Allow("client-a") {
		t.Fatal("second request within the window should be blocked")
	}
	time.Sleep(30 * time.Millisecond)
	if !l.Allow("client-a") {
		t.Fatal("request after the window expires should be allowed again")
	}
}

func TestClientKeyPrefersForwardedFor(t *testing.T) {
	r := httptest.NewRequest("GET", "/", nil)
	r.RemoteAddr = "10.0.0.1:1234"
	r.Header.Set("X-Forwarded-For", "203.0.113.5, 10.0.0.1")

	if got := ClientKey(r); got != "203.0.113.5" {
		t.Errorf("expected first X-Forwarded-For entry, got %q", got)
	}
}

func TestClientKeyFallsBackToRemoteAddr(t *testing.T) {
	r := httptest.NewRequest("GET", "/", nil)
	r.RemoteAddr = "10.0.0.1:1234"

	if got := ClientKey(r); got != "10.0.0.1:1234" {
		t.Errorf("expected RemoteAddr fallback, got %q", got)
	}
}
