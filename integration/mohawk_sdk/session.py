"""
Session management for Mohawk Inference Engine.

Handles model sessions, slice assignments, and execution context.
"""

from typing import Optional, Dict, Any, List
from pathlib import Path
import time
import numpy as np


class Session:
    """
    Represents an active inference session.
    
    Tracks model state, slice assignments, and execution context.
    
    Example usage:
        >>> with client.load_model("model.onnx") as session:
        ...     output = client.infer(session, input_tensor)
    """
    
    def __init__(
        self,
        model_path: str,
        device_map: Optional[Dict[str, str]] = None,
        slice_count: int = 2,
        client=None
    ):
        """
        Initialize session.
        
        Args:
            model_path: Path to loaded model
            device_map: Device mapping for slices
            slice_count: Number of slices in model
            client: MohawkClient instance (optional)
        """
        self.model_path = str(model_path)
        self.device_map = device_map or {}
        self.slice_count = slice_count
        self.client = client
        self.session_id = f"session_{time.time_ns()}"
        self.slices: List[Dict] = []
        self.metrics_history: List[Dict] = []
        self._model = None
    
    def __enter__(self):
        """Context manager entry."""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - cleanup."""
        if exc_type is None:
            if hasattr(self, 'client') and self.client:
                self.client.close()
        return False
    
    def set_device_map(self, device_map: Dict[str, str]):
        """
        Set custom device mapping for slices.
        
        Args:
            device_map: Mapping from slice ranges to devices
                       e.g., {"layer_0-1": "cuda", "layer_2-3": "cpu"}
        """
        self.device_map = device_map
    
    def get_slice_info(self) -> List[Dict]:
        """
        Get information about loaded slices.
        
        Returns:
            List of slice metadata dictionaries
        """
        if not self.slices:
            return [
                {
                    "id": f"slice_{i}",
                    "status": "loaded",
                    "device": self.device_map.get(f"layer_{i}-{i+1}", "cpu")
                }
                for i in range(self.slice_count)
            ]
        return self.slices
    
    def reset(self):
        """Reset session state."""
        self._model = None
        self.metrics_history = []
        print(f"Session {self.session_id} reset")
    
    def get_metrics(self) -> Dict[str, Any]:
        """
        Get current metrics for this session.
        
        Returns:
            Metrics dictionary with latency, throughput, etc.
        """
        if self.metrics_history:
            latencies = [m.get('latency_ms', 0) for m in self.metrics_history]
            return {
                "p50_ms": np.percentile(latencies, 50) if latencies else 0,
                "p95_ms": np.percentile(latencies, 95) if len(latencies) > 20 else latencies[-1] if latencies else 0,
                "p99_ms": np.percentile(latencies, 99) if len(latencies) > 100 else latencies[-1] if latencies else 0,
                "avg_ms": np.mean(latencies) if latencies else 0,
            }
        return {}
    
    def __repr__(self):
        return f"Session(path={self.model_path}, slices={self.slice_count}, id={self.session_id})"


class SliceInfo:
    """
    Information about a loaded model slice.
    """
    
    def __init__(
        self,
        slice_id: str,
        start_layer: int,
        end_layer: int,
        device: str = "cpu",
        size_mb: float = 0.0
    ):
        self.slice_id = slice_id
        self.start_layer = start_layer
        self.end_layer = end_layer
        self.device = device
        self.size_mb = size_mb
    
    @property
    def layer_range(self) -> str:
        """Get layer range string."""
        return f"layer_{self.start_layer}-{self.end_layer}"
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "id": self.slice_id,
            "range": self.layer_range,
            "device": self.device,
            "size_mb": self.size_mb,
        }


def create_session(
    model_path: str | Path,
    device_map: Optional[Dict[str, str]] = None,
    slice_count: int = 2
) -> Session:
    """
    Factory function to create a new session.
    
    Args:
        model_path: Path to model file
        device_map: Device mapping for slices
        slice_count: Number of slices
        
    Returns:
        Configured Session instance
    """
    return Session(
        model_path=str(model_path),
        device_map=device_map or {},
        slice_count=slice_count
    )
