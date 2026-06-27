"""
Metrics collection and aggregation for Mohawk Inference Engine.

Provides metric recording, percentiles calculation, and streaming capabilities.
"""

import time
from typing import Dict, List, Optional
from dataclasses import dataclass
from collections import deque


@dataclass
class MetricSnapshot:
    """Single metric snapshot."""
    timestamp: float
    latency_p50_ms: float
    latency_p95_ms: float
    latency_p99_ms: float
    throughput: float
    active_sessions: int
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            "timestamp": self.timestamp,
            "latency_p50_ms": self.latency_p50_ms,
            "latency_p95_ms": self.latency_p95_ms,
            "latency_p99_ms": self.latency_p99_ms,
            "throughput": self.throughput,
            "active_sessions": self.active_sessions,
        }


class MetricCollector:
    """
    Collect and aggregate metrics from Mohawk sessions.
    
    Example usage:
        >>> collector = MetricCollector()
        >>> snapshot = collector.record(
            latency_p50_ms=12.5,
            latency_p95_ms=45.3,
            throughput=80.2,
            active_sessions=5
        )
    """
    
    def __init__(self, max_history: int = 1000):
        self.snapshots: List[MetricSnapshot] = []
        self._latencies_p50: deque = deque(maxlen=max_history)
        self._latencies_p95: deque = deque(maxlen=max_history)
        self._latencies_p99: deque = deque(maxlen=max_history)
        self._throughput: deque = deque(maxlen=max_history)
    
    def record(
        self,
        latency_p50_ms: float,
        latency_p95_ms: float,
        latency_p99_ms: float,
        throughput: float,
        active_sessions: int
    ) -> MetricSnapshot:
        """Record a metric snapshot."""
        snapshot = MetricSnapshot(
            timestamp=time.time(),
            latency_p50_ms=latency_p50_ms,
            latency_p95_ms=latency_p95_ms,
            latency_p99_ms=latency_p99_ms,
            throughput=throughput,
            active_sessions=active_sessions,
        )
        
        self.snapshots.append(snapshot)
        self._latencies_p50.append(latency_p50_ms)
        self._latencies_p95.append(latency_p95_ms)
        self._latencies_p99.append(latency_p99_ms)
        self._throughput.append(throughput)
        
        return snapshot
    
    def get_percentiles(self) -> Dict[str, float]:
        """Get percentile statistics."""
        latencies_sorted = sorted(self._latencies_p50)
        
        if not latencies_sorted:
            return {}
        
        n = len(latencies_sorted)
        
        return {
            "p50_ms": latencies_sorted[int(n * 0.5)] if n > 0 else 0,
            "p95_ms": latencies_sorted[int(n * 0.95)] if n > 20 else latencies_sorted[-1] if latencies_sorted else 0,
            "p99_ms": latencies_sorted[int(n * 0.99)] if n > 100 else latencies_sorted[-1] if latencies_sorted else 0,
            "avg_ms": sum(latencies_sorted) / len(latencies_sorted),
        }
    
    def get_history(self, limit: int = 100) -> List[Dict[str, Any]]:
        """Get recent metric history."""
        return [s.to_dict() for s in self.snapshots[-limit:]]
    
    def clear(self):
        """Clear all collected metrics."""
        self.snapshots.clear()
        self._latencies_p50.clear()
        self._latencies_p95.clear()
        self._latencies_p99.clear()
        self._throughput.clear()
    
    def reset(self):
        """Reset collector but keep history."""
        self._latencies_p50.clear()
        self._latencies_p95.clear()
        self._latencies_p99.clear()
        self._throughput.clear()


class StreamingMetrics:
    """
    Stream metrics for real-time monitoring.
    
    Example usage:
        >>> stream = StreamingMetrics(interval=1.0)
        >>> def update_metrics(metrics):
        ...     stream.update(metrics)
        >>> stream.start(update_metrics)
    """
    
    def __init__(self, interval: float = 1.0):
        self.interval = interval
        self._callback = None
        self._running = False
    
    def set_callback(self, callback):
        """Set callback for metric updates."""
        self._callback = callback
    
    def update(self, metrics: Dict[str, Any]):
        """Update metrics stream."""
        if self._callback:
            self._callback(metrics)
    
    def start(self, callback=None):
        """Start streaming (placeholder - would use threading)."""
        if callback is None:
            callback = self._callback
        
        # Placeholder for actual implementation
        print(f"Would start metrics streaming with interval={self.interval}s")
    
    def stop(self):
        """Stop streaming."""
        self._running = False
        print("Metrics streaming stopped")


def create_metric_collector(max_history: int = 1000) -> MetricCollector:
    """
    Factory function to create a metric collector.
    
    Args:
        max_history: Maximum number of snapshots to keep
        
    Returns:
        Configured MetricCollector instance
    """
    return MetricCollector(max_history=max_history)
