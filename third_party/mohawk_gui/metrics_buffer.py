"""
Metrics Buffer for Mohawk Inference Engine GUI

Provides efficient metrics buffering and downsampling for real-time visualization.
"""

from collections import deque
from dataclasses import dataclass, field
import random
import statistics
import time
from typing import Dict, Any, Optional


@dataclass
class BufferedMetrics:
    """Aggregated metrics over time window."""
    timestamp: float
    latency_p50: float
    latency_p95: float
    latency_p99: float
    throughput_rps: float
    gpu_utilization: float
    memory_mb: float = 0.0
    active_requests: int = 0
    
    def __add__(self, other):
        """Weighted average for aggregation."""
        # Weight new data less than existing buffer
        weight_new = 1 / (len(self.buffer) + 1) if hasattr(self, 'buffer') else 0.1
        
        return BufferedMetrics(
            timestamp=self.timestamp,
            latency_p50=(self.latency_p50 * (1 - weight_new) + other.latency_p50 * weight_new),
            latency_p95=(self.latency_p95 * (1 - weight_new) + other.latency_p95 * weight_new),
            latency_p99=(self.latency_p99 * (1 - weight_new) + other.latency_p99 * weight_new),
            throughput_rps=(self.throughput_rps + other.throughput_rps) / 2,
            gpu_utilization=(self.gpu_utilization + other.gpu_utilization) / 2,
            memory_mb=(self.memory_mb + other.memory_mb) / 2,
            active_requests=(self.active_requests + other.active_requests) / 2
        )


class MetricsBuffer:
    """
    Buffer and downsample metrics efficiently.
    
    Features:
    - Configurable window size for time-based aggregation
    - Sample rate control for high-frequency updates
    - Statistical summaries (percentiles, averages)
    - Memory-efficient deque with maxlen
    """
    
    def __init__(self, window_size: int = 1000, sample_rate: float = 0.1):
        """
        Initialize metrics buffer.
        
        Args:
            window_size: Maximum number of metrics to keep in buffer
            sample_rate: Probability of storing each metric (0.0-1.0)
        """
        self.buffer = deque(maxlen=window_size)
        self.sample_rate = sample_rate
        self._latency_history: deque = deque(maxlen=1000)
        self._throughput_history: deque = deque(maxlen=1000)
    
    async def add(self, metrics: Dict[str, Any]):
        """
        Add metrics with optional downsampling.
        
        Args:
            metrics: Dictionary of metric values
            
        Example:
            await buffer.add({
                "latency_p50_ms": 12.5,
                "latency_p95_ms": 45.2,
                "throughput_rps": 1250.0
            })
        """
        # Apply sampling
        if random.random() < self.sample_rate:
            buffered = BufferedMetrics(
                timestamp=metrics.get("timestamp", time.time()),
                latency_p50=metrics.get("latency_p50_ms", 0),
                latency_p95=metrics.get("latency_p95_ms", 0),
                latency_p99=metrics.get("latency_p99_ms", 0),
                throughput_rps=metrics.get("throughput_rps", 0),
                gpu_utilization=metrics.get("gpu_utilization", 0),
                memory_mb=metrics.get("memory_mb", 0),
                active_requests=metrics.get("active_requests", 0)
            )
            self.buffer.append(buffered)
            
            # Track latency and throughput separately for statistics
            if buffered.latency_p50 > 0:
                self._latency_history.append(buffered.latency_p50)
            if buffered.throughput_rps > 0:
                self._throughput_history.append(buffered.throughput_rps)
    
    def get_summary(self) -> Dict[str, Any]:
        """
        Return aggregated statistics.
        
        Returns:
            Dictionary with statistical summaries
        """
        if not self.buffer:
            return {
                "count": 0,
                "avg_latency_p50_ms": 0,
                "min_latency_p50_ms": 0,
                "max_latency_p50_ms": 0,
                "throughput_rps": 0,
                "buffer_utilization": 0
            }
        
        data = list(self.buffer)
        latencies = [m.latency_p50 for m in data if m.latency_p50 > 0]
        throughputs = [m.throughput_rps for m in data if m.throughput_rps > 0]
        
        return {
            "count": len(data),
            "avg_latency_p50_ms": statistics.mean(latencies) if latencies else 0,
            "min_latency_p50_ms": min(latencies) if latencies else 0,
            "max_latency_p50_ms": max(latencies) if latencies else 0,
            "p95_latency_ms": self._calculate_percentile(latencies, 0.95) if latencies else 0,
            "p99_latency_ms": self._calculate_percentile(latencies, 0.99) if latencies else 0,
            "avg_throughput_rps": statistics.mean(throughputs) if throughputs else 0,
            "min_throughput_rps": min(throughputs) if throughputs else 0,
            "max_throughput_rps": max(throughputs) if throughputs else 0,
            "buffer_utilization": len(self.buffer) / self.buffer.maxlen if self.buffer.maxlen > 0 else 0
        }
    
    def _calculate_percentile(self, data: list, percentile: float) -> float:
        """Calculate percentile from sorted data."""
        if not data:
            return 0
        
        if not (0 <= percentile <= 1):
            raise ValueError(f"Percentile must be between 0 and 1, got {percentile}")
        
        sorted_data = sorted(data)
        
        # Proper percentile calculation: map percentile to array index
        # For n items, index should range from 0 to n-1
        index = int((len(sorted_data) - 1) * percentile)
        
        # Explicit bounds checking
        index = max(0, min(index, len(sorted_data) - 1))
        
        return sorted_data[index]
    
    def get_time_series(self, window_size: int = None) -> list:
        """
        Get recent metrics as time series.
        
        Args:
            window_size: Number of recent entries to return (None for all)
            
        Returns:
            List of BufferedMetrics objects
        """
        if window_size is None:
            return list(self.buffer)
        
        return list(self.buffer)[-window_size:]
    
    def clear(self):
        """Clear the buffer."""
        self.buffer.clear()
        self._latency_history.clear()
        self._throughput_history.clear()
    
    def get_memory_usage(self) -> int:
        """Get approximate memory usage of buffer in bytes."""
        return len(self.buffer) * 1024  # Rough estimate


class MetricsAggregator:
    """Aggregate metrics across multiple sessions/workers."""
    
    def __init__(self):
        self.session_buffers: Dict[str, MetricsBuffer] = {}
    
    def get_or_create_buffer(self, session_id: str) -> MetricsBuffer:
        """Get existing buffer or create new one for session."""
        if session_id not in self.session_buffers:
            # Aggregation should be deterministic across sessions; avoid sampling loss.
            self.session_buffers[session_id] = MetricsBuffer(sample_rate=1.0)
        return self.session_buffers[session_id]
    
    async def add_metrics(self, session_id: str, metrics: Dict[str, Any]):
        """Add metrics to appropriate session buffer."""
        buffer = self.get_or_create_buffer(session_id)
        await buffer.add(metrics)
    
    def get_global_summary(self) -> Dict[str, Any]:
        """Get aggregated summary across all sessions."""
        all_latencies = []
        all_throughputs = []
        
        for buffer in self.session_buffers.values():
            summary = buffer.get_summary()
            if summary["count"] > 0:
                all_latencies.append(summary["avg_latency_p50_ms"])
                all_throughputs.append(summary["avg_throughput_rps"])
        
        return {
            "total_sessions": len(self.session_buffers),
            "active_sessions_with_metrics": len(all_latencies),
            "global_avg_latency_p50_ms": statistics.mean(all_latencies) if all_latencies else 0,
            "global_avg_throughput_rps": statistics.mean(all_throughputs) if all_throughputs else 0
        }


if __name__ == "__main__":
    # Test metrics buffer
    import asyncio
    
    async def test_buffer():
        buffer = MetricsBuffer(window_size=100, sample_rate=1.0)
        
        # Add some sample metrics
        for i in range(150):
            await buffer.add({
                "timestamp": time.time(),
                "latency_p50_ms": 10 + random.random() * 20,
                "latency_p95_ms": 40 + random.random() * 30,
                "latency_p99_ms": 60 + random.random() * 40,
                "throughput_rps": 1000 + random.random() * 500,
                "gpu_utilization": 50 + random.random() * 30,
                "memory_mb": 2000 + random.random() * 1000,
                "active_requests": 10 + random.randint(0, 50)
            })
        
        summary = buffer.get_summary()
        print(f"Buffer summary: {summary}")
    
    asyncio.run(test_buffer())
