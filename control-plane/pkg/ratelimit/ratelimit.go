// Package ratelimit implements a simple per-client sliding-window rate
// limiter. This is a local single-node gateway in front of one backend, not
// a distributed system — the goal is protecting the backend from a runaway
// client (a buggy retry loop, a stuck poll interval), not defending against
// coordinated abuse. A stdlib-only in-memory limiter is the right tool for
// that; anything fancier would be solving a problem this deployment doesn't
// have.
package ratelimit

import (
	"net/http"
	"strings"
	"sync"
	"time"
)

// Limiter tracks request timestamps per client key within a trailing window.
type Limiter struct {
	mu       sync.Mutex
	requests map[string][]time.Time
	limit    int
	window   time.Duration
}

// New creates a limiter allowing up to limit requests per client within window.
func New(limit int, window time.Duration) *Limiter {
	return &Limiter{
		requests: make(map[string][]time.Time),
		limit:    limit,
		window:   window,
	}
}

// Allow reports whether a request from key is within the limit, recording it
// if so. Expired timestamps for key are trimmed on every call, which keeps
// per-key memory bounded without a separate cleanup goroutine — adequate for
// the small, mostly-single-client (localhost) population this gateway
// actually sees.
func (l *Limiter) Allow(key string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()

	now := time.Now()
	cutoff := now.Add(-l.window)

	kept := l.requests[key][:0]
	for _, t := range l.requests[key] {
		if t.After(cutoff) {
			kept = append(kept, t)
		}
	}

	if len(kept) >= l.limit {
		l.requests[key] = kept
		return false
	}

	l.requests[key] = append(kept, now)
	return true
}

// ClientKey derives a rate-limit key from a request: the first X-Forwarded-For
// entry if present (this gateway may itself sit behind another proxy in some
// deployments), otherwise the direct remote address.
func ClientKey(r *http.Request) string {
	if xff := r.Header.Get("X-Forwarded-For"); xff != "" {
		if first, _, ok := strings.Cut(xff, ","); ok {
			return strings.TrimSpace(first)
		}
		return strings.TrimSpace(xff)
	}
	return r.RemoteAddr
}

// Middleware wraps next, rejecting requests over the limit with 429.
func (l *Limiter) Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !l.Allow(ClientKey(r)) {
			http.Error(w, "rate limit exceeded", http.StatusTooManyRequests)
			return
		}
		next.ServeHTTP(w, r)
	})
}
