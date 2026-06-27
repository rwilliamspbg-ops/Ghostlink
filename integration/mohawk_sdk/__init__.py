"""
Mohawk Inference Engine SDK v3.0

A user-friendly Python SDK for managing multi-device inference with Mohawk Inference Engine.

Provides high-level abstractions for:
- Model loading and deployment
- Session management
- Distributed inference execution
- Metrics collection and monitoring

Example usage:
    >>> from mohawk_sdk import MohawkClient
    >>> client = MohawkClient(host="localhost", port=8003)
    >>> with client.load_model("model.onnx") as session:
    ...     output = client.infer(session, input_tensor)

For more examples, see the documentation.
"""

from mohawk_sdk.client import MohawkClient
from mohawk_sdk.session import Session
from mohawk_sdk.config import MohawkConfig
from mohawk_sdk.metrics import MetricCollector
from mohawk_sdk.utils import (
    create_tensor,
    load_model_from_file,
    save_tensor_to_file,
    benchmark_inference,
)

__version__ = "3.0.0"
__author__ = "Mohawk Ops Team <mohawk@sovereign-mohawk-proto.io>"

__all__ = [
    "MohawkClient",
    "Session",
    "MohawkConfig",
    "MetricCollector",
    "create_tensor",
    "load_model_from_file",
    "save_tensor_to_file",
    "benchmark_inference",
]
