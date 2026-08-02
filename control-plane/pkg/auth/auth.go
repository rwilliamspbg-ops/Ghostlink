// Package auth verifies the same bearer token ghost-link's own API server
// requires (see crates/ghost-link/src/auth.rs) — real edge-rejection, not
// just relying on the proxy transparently forwarding the Authorization
// header through to ghost-link (which it already does; this middleware
// stops unauthenticated traffic here too, before it's proxied at all).
//
// Accepts either the raw shared API key as the bearer token, or a JWT
// issued by ghost-link's /api/security/jwt/refresh (HS256, signed with
// that same key) — genuine signature verification via golang-jwt, not a
// shape-only check, so it doesn't reject legitimate short-lived tokens the
// GUI actually uses.
package auth

import (
	"net/http"
	"os"
	"strings"

	"github.com/golang-jwt/jwt/v5"
)

// LoadAPIKey reads the API key from the same file ghost-link itself
// generates and persists on first run (default api_key.txt, overridable
// via GHOSTLINK_API_KEY_PATH — the exact env var name ghost-link's own
// auth.rs uses, so both processes agree on where to look without extra
// configuration when they share a working directory, the typical
// same-host deployment this gateway assumes).
func LoadAPIKey() (string, error) {
	path := os.Getenv("GHOSTLINK_API_KEY_PATH")
	if path == "" {
		path = "api_key.txt"
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(data)), nil
}

func extractBearerToken(header string) string {
	const prefix = "Bearer "
	if !strings.HasPrefix(header, prefix) {
		return ""
	}
	return strings.TrimSpace(strings.TrimPrefix(header, prefix))
}

// verify reports whether token is either an exact match for apiKey or a
// currently-valid (unexpired, correctly-signed) JWT signed with apiKey.
func verify(token, apiKey string) bool {
	if token == "" {
		return false
	}
	if token == apiKey {
		return true
	}

	claims := jwt.RegisteredClaims{}
	parsed, err := jwt.ParseWithClaims(token, &claims, func(t *jwt.Token) (interface{}, error) {
		return []byte(apiKey), nil
	}, jwt.WithValidMethods([]string{"HS256"}))
	return err == nil && parsed.Valid
}

// Middleware guards every route except /health behind the shared bearer
// token. If apiKey is empty (the key file wasn't readable at startup —
// e.g. ghost-link hasn't generated one yet, or the gateway is misconfigured
// with a different path), it logs nothing itself and lets every request
// through unchecked: ghost-link's own auth still applies end to end via the
// forwarded Authorization header, so this degrades to "no extra edge
// rejection" rather than "locks everyone out" or "silently insecure in a
// new way" — the pre-this-change baseline either way.
func Middleware(apiKey string) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		if apiKey == "" {
			return next
		}
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path == "/health" {
				next.ServeHTTP(w, r)
				return
			}

			token := extractBearerToken(r.Header.Get("Authorization"))
			if !verify(token, apiKey) {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusUnauthorized)
				_, _ = w.Write([]byte(`{"error":{"message":"missing or invalid Authorization: Bearer <token>","type":"unauthorized"}}`))
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}
