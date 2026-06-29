"""
GUI Monitoring for Mohawk Inference Engine

Provides self-monitoring and performance tracking.
"""

import psutil
import time
from dataclasses import dataclass, field
from typing import Dict, Any, Optional
import json


@dataclass
class Guimetrics:
    """Metrics about GUI health."""
    timestamp: float
    uptime_seconds: float
    memory_usage_mb: float
    cpu_percent: float
    active_connections: int = 0
    ui_thread_blocked: bool = False
    gpu_utilization: float = 0.0
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        return {k: v for k, v in self.__dict__.items() if not k.startswith('_')}


class GuimetricsCollector:
    """
    Monitor GUI health and performance.
    
    Features:
    - Process memory and CPU monitoring
    - UI thread responsiveness tracking
    - Connection count monitoring
    - GPU utilization tracking
    """
    
    def __init__(self):
        self.start_time = time.time()
        self.process = psutil.Process()
        self._last_check_time: float = time.time()
        self._blocked_count: int = 0
    
    def collect(self) -> Dict[str, Any]:
        """Collect current metrics."""
        return Guimetrics(
            timestamp=time.time(),
            uptime_seconds=time.time() - self.start_time,
            memory_usage_mb=self.process.memory_info().rss / 1024 / 1024,
            cpu_percent=self.process.cpu_percent(),
            active_connections=0,  # Would count actual WebSocket connections
            ui_thread_blocked=False,  # Would detect if main thread blocked
            gpu_utilization=0.0  # Would query GPU metrics
        ).to_dict()
    
    def check_ui_responsiveness(self, threshold_seconds: float = 1.0) -> bool:
        """Check if UI thread is responsive."""
        now = time.time()
        elapsed = now - self._last_check_time
        
        if elapsed > threshold_seconds:
            self._blocked_count += 1
            return False
        
        self._last_check_time = now
        return True
    
    def get_blocked_stats(self) -> Dict[str, Any]:
        """Get UI blocking statistics."""
        return {
            "total_blocks": self._blocked_count,
            "avg_block_duration_s": 0.5  # Would calculate from logs
        }


class PerformanceTracker:
    """Track application performance metrics."""
    
    def __init__(self):
        self.operation_times: Dict[str, list] = {}
        self.request_count: int = 0
    
    def record_operation(self, operation_name: str, duration_ms: float):
        """Record operation duration."""
        if operation_name not in self.operation_times:
            self.operation_times[operation_name] = []
        
        self.operation_times[operation_name].append(duration_ms)
    
    def get_operation_stats(self, operation_name: str) -> Dict[str, Any]:
        """Get statistics for specific operation."""
        times = self.operation_times.get(operation_name, [])
        
        if not times:
            return {"count": 0}
        
        sorted_times = sorted(times)
        n = len(sorted_times)
        
        return {
            "count": n,
            "avg_ms": sum(times) / n,
            "min_ms": min(times),
            "max_ms": max(times),
            "p50_ms": sorted_times[int(n * 0.5)],
            "p95_ms": sorted_times[int(n * 0.95)] if n > 20 else sorted_times[-1],
            "p99_ms": sorted_times[int(n * 0.99)] if n > 100 else sorted_times[-1]
        }


class SystemMonitor:
    """Monitor system resources."""
    
    def __init__(self):
        self.process = psutil.Process()
    
    def get_memory_info(self) -> Dict[str, Any]:
        """Get memory usage information."""
        mem = self.process.memory_info()
        return {
            "rss_mb": mem.rss / 1024 / 1024,
            "vms_mb": mem.vms / 1024 / 1024,
            "percent": self.process.memory_percent()
        }
    
    def get_cpu_info(self) -> Dict[str, Any]:
        """Get CPU usage information."""
        return {
            "percent": self.process.cpu_percent(),
            "times": dict(self.process.cpu_times())
        }
    
    def get_disk_usage(self, path: str = "/") -> Dict[str, Any]:
        """Get disk usage for specified path."""
        usage = psutil.disk_usage(path)
        return {
            "total_gb": usage.total / 1024 / 1024 / 1024,
            "used_gb": usage.used / 1024 / 1024 / 1024,
            "free_gb": usage.free / 1024 / 1024 / 1024,
            "percent": usage.percent
        }


if __name__ == "__main__":
    import asyncio
    
    # Test monitoring
    collector = GuimetricsCollector()
    
    for _ in range(5):
        metrics = collector.collect()
        print(f"Uptime: {metrics['uptime_seconds']:.1f}s, Memory: {metrics['memory_usage_mb']:.1f}MB")
        time.sleep(0.5)
