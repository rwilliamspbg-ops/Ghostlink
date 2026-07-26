package auth

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

func TestLoadAPIKeyReadsAndTrimsFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "api_key.txt")
	if err := os.WriteFile(path, []byte("  abc123\n"), 0o600); err != nil {
		t.Fatalf("write test key file: %v", err)
	}
	t.Setenv("GHOSTLINK_API_KEY_PATH", path)

	key, err := LoadAPIKey()
	if err != nil {
		t.Fatalf("LoadAPIKey: %v", err)
	}
	if key != "abc123" {
		t.Errorf("expected trimmed key %q, got %q", "abc123", key)
	}
}

func TestLoadAPIKeyReturnsErrorWhenMissing(t *testing.T) {
	t.Setenv("GHOSTLINK_API_KEY_PATH", filepath.Join(t.TempDir(), "does-not-exist.txt"))
	if _, err := LoadAPIKey(); err == nil {
		t.Fatal("expected an error for a missing key file, got nil")
	}
}

func TestExtractBearerTokenParsesWellFormedHeaderOnly(t *testing.T) {
	cases := []struct {
		header string
		want   string
	}{
		{"Bearer abc123", "abc123"},
		{"bearer abc123", ""}, // case-sensitive, matches the real Authorization scheme
		{"abc123", ""},
		{"", ""},
	}
	for _, c := range cases {
		if got := extractBearerToken(c.header); got != c.want {
			t.Errorf("extractBearerToken(%q) = %q, want %q", c.header, got, c.want)
		}
	}
}

func TestVerifyAcceptsRawKeyMatchAndRejectsWrongOne(t *testing.T) {
	const key = "the-real-key"
	if !verify(key, key) {
		t.Error("exact key match should verify")
	}
	if verify("wrong-key", key) {
		t.Error("wrong key should not verify")
	}
	if verify("", key) {
		t.Error("empty token should not verify")
	}
}

func TestVerifyAcceptsAGenuinelyValidJWTAndRejectsTamperedOrExpired(t *testing.T) {
	const key = "the-real-key"

	valid := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.RegisteredClaims{
		Subject:   "ghostlink-client",
		IssuedAt:  jwt.NewNumericDate(time.Now()),
		ExpiresAt: jwt.NewNumericDate(time.Now().Add(time.Hour)),
	})
	signed, err := valid.SignedString([]byte(key))
	if err != nil {
		t.Fatalf("sign test JWT: %v", err)
	}
	if !verify(signed, key) {
		t.Error("a genuinely valid JWT signed with the right key should verify")
	}

	if verify(signed+"tampered", key) {
		t.Error("a tampered JWT must not verify")
	}
	if verify(signed, "different-key") {
		t.Error("a JWT signed with a different key must not verify against this one")
	}

	expired := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.RegisteredClaims{
		Subject:   "ghostlink-client",
		IssuedAt:  jwt.NewNumericDate(time.Now().Add(-2 * time.Hour)),
		ExpiresAt: jwt.NewNumericDate(time.Now().Add(-time.Hour)),
	})
	expiredSigned, err := expired.SignedString([]byte(key))
	if err != nil {
		t.Fatalf("sign expired test JWT: %v", err)
	}
	if verify(expiredSigned, key) {
		t.Error("an expired JWT must not verify")
	}
}

func TestMiddlewareIsANoOpWhenAPIKeyIsEmpty(t *testing.T) {
	called := false
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		called = true
		w.WriteHeader(http.StatusOK)
	})
	handler := Middleware("")(inner)

	req := httptest.NewRequest("GET", "/v1/models", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if !called {
		t.Error("with an empty API key, requests should pass through unchecked")
	}
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}
}

func TestMiddlewareAllowsHealthWithoutAuth(t *testing.T) {
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	handler := Middleware("real-key")(inner)

	req := httptest.NewRequest("GET", "/health", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("/health should bypass auth even with no token, got %d", w.Code)
	}
}

func TestMiddlewareRejectsMissingOrWrongTokenAndAcceptsCorrectOne(t *testing.T) {
	const key = "real-key"
	called := false
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		called = true
		w.WriteHeader(http.StatusOK)
	})
	handler := Middleware(key)(inner)

	req := httptest.NewRequest("GET", "/v1/models", nil)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)
	if w.Code != http.StatusUnauthorized {
		t.Errorf("no Authorization header: expected 401, got %d", w.Code)
	}
	if called {
		t.Error("inner handler must not run for an unauthenticated request")
	}

	req = httptest.NewRequest("GET", "/v1/models", nil)
	req.Header.Set("Authorization", "Bearer wrong-key")
	w = httptest.NewRecorder()
	handler.ServeHTTP(w, req)
	if w.Code != http.StatusUnauthorized {
		t.Errorf("wrong bearer token: expected 401, got %d", w.Code)
	}

	called = false
	req = httptest.NewRequest("GET", "/v1/models", nil)
	req.Header.Set("Authorization", "Bearer "+key)
	w = httptest.NewRecorder()
	handler.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Errorf("correct bearer token: expected 200, got %d", w.Code)
	}
	if !called {
		t.Error("inner handler should run for an authenticated request")
	}
}
