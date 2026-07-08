package registry

import (
	"testing"
)

func TestRegistry(t *testing.T) {
	reg := NewRegistry()

	w := &Worker{
		ID:       "worker-1",
		Hostname: "local",
		Port:     8003,
	}

	reg.Register(w)

	workers := reg.List()
	if len(workers) != 1 {
		t.Errorf("expected 1 worker, got %d", len(workers))
	}

	if !reg.Heartbeat("worker-1", 10.0, 20.0, 30.0) {
		t.Error("heartbeat failed for existing worker")
	}
}
