package registry

import (
	"sync"
	"time"
)

type WorkerStatus string

const (
	StatusOnline  WorkerStatus = "online"
	StatusOffline WorkerStatus = "offline"
	StatusBusy    WorkerStatus = "busy"
)

type Worker struct {
	ID             string       `json:"id"`
	Hostname       string       `json:"hostname"`
	IPAddress      string       `json:"ip_address"`
	Port           int          `json:"port"`
	VRAMGB         float32      `json:"vram_gb"`
	SystemMemoryGB float32      `json:"system_memory_gb"`
	Acceleration   string       `json:"acceleration"`
	Status         WorkerStatus `json:"status"`
	LastHeartbeat  time.Time    `json:"last_heartbeat"`
	CPUUsage       float32      `json:"cpu_usage"`
	MemoryUsage    float32      `json:"memory_usage"`
	GPUUsage       float32      `json:"gpu_usage"`
}

type Registry struct {
	mu      sync.RWMutex
	workers map[string]*Worker
}

func NewRegistry() *Registry {
	return &Registry{
		workers: make(map[string]*Worker),
	}
}

func (r *Registry) Register(w *Worker) {
	r.mu.Lock()
	defer r.mu.Unlock()
	w.LastHeartbeat = time.Now()
	w.Status = StatusOnline
	r.workers[w.ID] = w
}

func (r *Registry) Heartbeat(id string, cpu, mem, gpu float32) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	if w, ok := r.workers[id]; ok {
		w.LastHeartbeat = time.Now()
		w.CPUUsage = cpu
		w.MemoryUsage = mem
		w.GPUUsage = gpu
		w.Status = StatusOnline
		return true
	}
	return false
}

func (r *Registry) List() []*Worker {
	r.mu.RLock()
	defer r.mu.RUnlock()
	list := make([]*Worker, 0, len(r.workers))
	for _, w := range r.workers {
		list = append(list, w)
	}
	return list
}

func (r *Registry) Cleanup(timeout time.Duration) {
	r.mu.Lock()
	defer r.mu.Unlock()
	now := time.Now()
	for id, w := range r.workers {
		if now.Sub(w.LastHeartbeat) > timeout {
			w.Status = StatusOffline
			delete(r.workers, id)
		}
	}
}

func (r *Registry) Deregister(id string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	if _, ok := r.workers[id]; ok {
		delete(r.workers, id)
		return true
	}
	return false
}

func (r *Registry) Summary() map[string]int {
	r.mu.RLock()
	defer r.mu.RUnlock()
	summary := map[string]int{
		"total":   len(r.workers),
		"online":  0,
		"offline": 0,
		"busy":    0,
	}
	for _, w := range r.workers {
		switch w.Status {
		case StatusOnline:
			summary["online"]++
		case StatusOffline:
			summary["offline"]++
		case StatusBusy:
			summary["busy"]++
		}
	}
	return summary
}
