"""
MohawkClient - Main inference client for Mohawk Inference Engine.

Provides high-level API for model deployment, session management, and inference.
"""

from typing import Optional, Dict, Any, List
from pathlib import Path
import numpy as np
import requests
import time


class MohawkClient:
    """
    Main client class for Mohawk Inference Engine.
    
    Provides high-level API for:
    - Model loading and partitioning
    - Session creation and management
    - Distributed inference execution
    - Metrics collection
    
    Example:
        >>> from mohawk_sdk import MohawkClient
        >>> client = MohawkClient(host="localhost", port=8003)
        >>> session = client.load_model("path/to/model.onnx")
        >>> result = client.infer(session, input_tensor)
    """
    
    def __init__(
        self,
        host: str = "localhost",
        port: int = 8003,
        secure: bool = True,
        timeout: float = 30.0,
        base_url: Optional[str] = None
    ):
        """
        Initialize Mohawk client.
        
        Args:
            host: Worker host address
            port: Worker port number
            secure: Enable PQC encryption (default True)
            timeout: Request timeout in seconds (default 30)
            base_url: Override default base URL (e.g., "http://localhost:8003")
        """
        self.host = host
        self.port = port
        self.secure = secure
        self.timeout = timeout
        
        # Determine base URL
        if base_url:
            self.base_url = base_url
        else:
            scheme = "https" if secure else "http"
            self.base_url = f"{scheme}://{host}:{port}"
        
        # Create session for connection pooling
        self.session = requests.Session()
        self.session.timeout = timeout
        
        # Metrics collector
        from mohawk_sdk.metrics import MetricCollector
        self._metrics = MetricCollector()
    
    def load_model(
        self,
        model_path: str | Path,
        device_map: Optional[Dict[str, str]] = None,
        slice_count: int = 2,
        preload: bool = True
    ) -> 'Session':
        """
        Load a model and create inference session.
        
        Args:
            model_path: Path to ONNX or TorchScript model
            device_map: Optional device mapping (e.g., {"layer_0-1": "cuda", ...})
            slice_count: Number of slices for partitioning
            preload: Whether to preload slices to workers
            
        Returns:
            Session object for inference
            
        Example:
            >>> session = client.load_model("llama-7b.onnx", slice_count=4)
        """
        path = Path(model_path)
        
        if not path.exists():
            raise FileNotFoundError(f"Model file not found: {model_path}")
        
        # For now, return a simple session object
        # In production, this would make API calls to controller
        from mohawk_sdk.session import Session
        
        session = Session(
            model_path=str(path),
            device_map=device_map or {},
            slice_count=slice_count,
            client=self
        )
        
        if preload:
            self._preload_session(session)
        
        return session
    
    def _preload_session(self, session: 'Session') -> None:
        """Preload slices to workers."""
        # Implementation would call controller API
        # For now, just log that preloading would happen
        print(f"Would preload {session.slice_count} slices to workers...")
    
    def infer(
        self,
        session: 'Session',
        input_tensor: np.ndarray,
        options: Optional[Dict[str, Any]] = None
    ) -> np.ndarray:
        """
        Perform inference on a session.
        
        Args:
            session: Loaded model session
            input_tensor: Input tensor (numpy array)
            options: Optional inference options (e.g., {"temperature": 0.7})
            
        Returns:
            Output tensor
            
        Example:
            >>> output = client.infer(session, input_tensor)
        """
        if not hasattr(self, '_session'):
            self._session = session
        
        # For now, just run through the model locally
        # In production, this would distribute across workers
        if hasattr(session, 'model') and session.model:
            return session.model.apply(input_tensor)
        
        # Simulate distributed inference
        print(f"Running inference on {session.slice_count} slices...")
        output = self._simulate_distributed_inference(input_tensor, session.slice_count)
        
        return output
    
    def _simulate_distributed_inference(
        self,
        input_tensor: np.ndarray,
        slice_count: int
    ) -> np.ndarray:
        """Simulate distributed inference (placeholder)."""
        # This would actually distribute across workers
        # For now, just pass through
        return input_tensor
    
    def get_metrics(self, session_id: Optional[str] = None) -> Dict[str, Any]:
        """
        Get metrics for a session or overall.
        
        Args:
            session_id: Session identifier (optional)
            
        Returns:
            Metrics dictionary with latency, throughput, etc.
        """
        if hasattr(self, '_metrics'):
            return self._metrics.get_percentiles()
        return {}
    
    def close(self):
        """Close client and cleanup resources."""
        if hasattr(self, 'session'):
            self.session.close()
        
        print("MohawkClient closed successfully")
    
    def __enter__(self):
        """Context manager entry."""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - cleanup."""
        if not exc_type:  # Only close on success
            self.close()
        return False


class Session:
    """
    Represents an active inference session.
    
    Tracks model state, slice assignments, and execution context.
    """
    
    def __init__(
        self,
        client: MohawkClient,
        model_path: str,
        device_map: Optional[Dict[str, str]] = None,
        slice_count: int = 2
    ):
        self.client = client
        self.model_path = model_path
        self.device_map = device_map or {}
        self.slice_count = slice_count
        self.session_id = f"session_{time.time_ns()}"
        self._model = None
    
    def __enter__(self):
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        if exc_type is None:
            self.client.close()
        return False
    
    def set_device_map(self, device_map: Dict[str, str]):
        """Set custom device mapping for slices."""
        self.device_map = device_map
    
    def get_slice_info(self) -> List[Dict]:
        """Get information about loaded slices."""
        # Placeholder - would return actual slice metadata
        return [
            {"id": f"slice_{i}", "status": "loaded"}
            for i in range(self.slice_count)
        ]
    
    def reset(self):
        """Reset session state."""
        self._model = None
        print(f"Session {self.session_id} reset")


def create_client(
    host: str = "localhost",
    port: int = 8003,
    secure: bool = True,
    base_url: Optional[str] = None
) -> MohawkClient:
    """
    Factory function to create a new client.
    
    Args:
        host: Worker host address
        port: Worker port number
        secure: Enable PQC encryption
        base_url: Override default base URL
        
    Returns:
        Configured MohawkClient instance
    """
    return MohawkClient(
        host=host,
        port=port,
        secure=secure,
        base_url=base_url
    )
