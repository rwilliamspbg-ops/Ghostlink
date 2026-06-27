"""
Utility functions for common tasks in Mohawk Inference Engine.

Includes tensor creation, model loading, benchmarking, and formatting helpers.
"""

from typing import Optional, Dict, Any
import numpy as np
from pathlib import Path
import time


def create_tensor(
    shape: tuple[int, ...],
    dtype: str = "float32",
    fill_value: float = 0.0
) -> np.ndarray:
    """
    Create a tensor with specified shape and dtype.
    
    Args:
        shape: Tensor shape (e.g., (8, 1) for batch of 8)
        dtype: Data type ("float32", "float16", etc.)
        fill_value: Value to fill tensor with
        
    Returns:
        Numpy array with specified properties
        
    Example:
        >>> tensor = create_tensor((8, 1), dtype="float32")
    """
    if dtype == "float32":
        return np.full(shape, fill_value, dtype=np.float32)
    elif dtype == "float16":
        return np.full(shape, fill_value, dtype=np.float16)
    else:
        raise ValueError(f"Unsupported dtype: {dtype}")


def create_random_tensor(
    shape: tuple[int, ...],
    dtype: str = "float32",
    rng_seed: Optional[int] = None
) -> np.ndarray:
    """
    Create a random tensor with specified shape and dtype.
    
    Args:
        shape: Tensor shape
        dtype: Data type
        rng_seed: Random number generator seed (optional)
        
    Returns:
        Numpy array with random values
        
    Example:
        >>> tensor = create_random_tensor((8, 1), dtype="float32", rng_seed=42)
    """
    if rng_seed is not None:
        rng = np.random.default_rng(rng_seed)
    else:
        rng = np.random.default_rng()
    
    if dtype == "float32":
        return rng.standard_normal(shape, dtype=np.float32)
    elif dtype == "float16":
        return rng.standard_normal(shape, dtype=np.float32).astype(np.float16)
    else:
        raise ValueError(f"Unsupported dtype: {dtype}")


def create_batched_tensor(
    batch_size: int,
    seq_len: int,
    hidden_dim: int,
    dtype: str = "float32",
    rng_seed: Optional[int] = None
) -> np.ndarray:
    """
    Create a batched tensor (e.g., for text generation).
    
    Args:
        batch_size: Number of sequences in batch
        seq_len: Sequence length per token
        hidden_dim: Hidden dimension size
        dtype: Data type
        rng_seed: Random number generator seed
        
    Returns:
        Numpy array with shape (batch_size, seq_len, hidden_dim)
        
    Example:
        >>> input_tensor = create_batched_tensor(8, 1, 4096, dtype="float32")
    """
    shape = (batch_size, seq_len, hidden_dim)
    return create_random_tensor(shape, dtype=dtype, rng_seed=rng_seed)


def load_model_from_file(model_path: str | Path) -> Any:
    """
    Load model from file (ONNX or TorchScript).
    
    Args:
        model_path: Path to model file
        
    Returns:
        Loaded model object
        
    Example:
        >>> model = load_model_from_file("model.onnx")
    """
    path = Path(model_path)
    
    if not path.exists():
        raise FileNotFoundError(f"Model file not found: {model_path}")
    
    if path.suffix == ".onnx":
        try:
            import onnxruntime as ort
            return ort.InferenceSession(str(path))
        except ImportError:
            print("Warning: onnxruntime not installed. Using placeholder model.")
            return _create_placeholder_model()
    elif path.suffix in [".pt", ".pth"]:
        try:
            import torch
            return torch.jit.load(str(path))
        except ImportError:
            print("Warning: PyTorch not installed. Using placeholder model.")
            return _create_placeholder_model()
    else:
        raise ValueError(f"Unsupported model format: {path.suffix}")


def _create_placeholder_model():
    """Create a placeholder model for testing."""
    class PlaceholderModel:
        def apply(self, x):
            return x
        
        def forward(self, x):
            return x
    
    return PlaceholderModel()


def save_tensor_to_file(
    tensor: np.ndarray,
    path: str | Path,
    format: str = "npy"
) -> None:
    """
    Save tensor to file.
    
    Args:
        tensor: Numpy array to save
        path: Output path
        format: File format ("npy", "npz", etc.)
        
    Example:
        >>> save_tensor_to_file(tensor, "output.npy")
    """
    path = Path(path)
    
    if format == "npy":
        np.save(str(path), tensor)
    elif format == "npz":
        np.savez(str(path), array=tensor)
    else:
        raise ValueError(f"Unsupported format: {format}")


def benchmark_inference(
    client,
    session,
    input_tensor: np.ndarray,
    iterations: int = 10,
    warmup: int = 5
) -> Dict[str, float]:
    """
    Benchmark inference performance.
    
    Args:
        client: Mohawk client instance
        session: Loaded model session
        input_tensor: Input tensor for benchmarking
        iterations: Number of inference runs
        warmup: Warmup iterations before timing
        
    Returns:
        Dictionary with latency, throughput, and other metrics
        
    Example:
        >>> results = benchmark_inference(client, session, input_tensor)
    """
    # Warmup
    for _ in range(warmup):
        client.infer(session, input_tensor)
    
    # Benchmark
    latencies = []
    for i in range(iterations):
        start = time.perf_counter()
        client.infer(session, input_tensor)
        end = time.perf_counter()
        latencies.append(end - start)
    
    latencies.sort()
    
    return {
        "p50_ms": latencies[int(len(latencies) * 0.5)] * 1000,
        "p95_ms": latencies[int(len(latencies) * 0.95)] * 1000,
        "p99_ms": latencies[int(len(latencies) * 0.99)] * 1000,
        "avg_ms": np.mean(latencies) * 1000,
        "min_ms": min(latencies) * 1000,
        "max_ms": max(latencies) * 1000,
        "throughput_tokens_per_sec": iterations / (sum(latencies) if latencies else 1),
    }


def convert_tensor_dtype(tensor: np.ndarray, target_dtype: str) -> np.ndarray:
    """
    Convert tensor to specified dtype.
    
    Args:
        tensor: Input tensor
        target_dtype: Target dtype ("float32", "float16", etc.)
        
    Returns:
        Tensor with converted dtype
        
    Example:
        >>> float16_tensor = convert_tensor_dtype(tensor, "float16")
    """
    dtype_map = {
        "float32": np.float32,
        "float16": np.float16,
        "int32": np.int32,
    }
    
    target = dtype_map.get(target_dtype, np.float32)
    return tensor.astype(target)


def format_tensor_info(tensor: np.ndarray) -> str:
    """
    Format tensor information for display.
    
    Args:
        tensor: Numpy array
        
    Returns:
        Formatted string with shape, dtype, size, etc.
        
    Example:
        >>> info = format_tensor_info(input_tensor)
    """
    info = [
        f"Shape: {tensor.shape}",
        f"Dtype: {tensor.dtype}",
        f"Size: {tensor.nbytes / 1024:.1f} KB",
        f"Min: {tensor.min():.4f}",
        f"Max: {tensor.max():.4f}",
        f"Mean: {tensor.mean():.4f}",
    ]
    
    return "\n".join(info)


def parse_model_metadata(metadata: Dict[str, Any]) -> Dict[str, Any]:
    """
    Parse model metadata from JSON/dict.
    
    Args:
        metadata: Raw metadata dictionary
        
    Returns:
        Parsed metadata with normalized fields
    """
    return {
        "name": metadata.get("name", "Unknown"),
        "version": metadata.get("version", "1.0"),
        "input_shape": tuple(metadata.get("input_shape", ())),
        "output_shape": tuple(metadata.get("output_shape", ())),
        "num_parameters": sum(
            w.size * b.item() 
            for w, b in metadata.get("weights", [])
        ) if "weights" in metadata else 0,
    }


def create_test_input(batch_size: int = 1, seq_len: int = 1, hidden_dim: int = 4096) -> np.ndarray:
    """
    Create test input tensor for benchmarking.
    
    Args:
        batch_size: Batch size (default 1)
        seq_len: Sequence length (default 1)
        hidden_dim: Hidden dimension (default 4096)
        
    Returns:
        Test input tensor
        
    Example:
        >>> test_input = create_test_input()
    """
    return create_batched_tensor(batch_size, seq_len, hidden_dim)
