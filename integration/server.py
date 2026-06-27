"""
Server entry point for Mohawk Inference Engine
"""

import argparse
import sys


def main():
    """Main entry point for the server"""
    parser = argparse.ArgumentParser(
        description="Mohawk Inference Engine Server"
    )
    parser.add_argument(
        "--host", "-H",
        default="0.0.0.0",
        help="Host to bind to (default: 0.0.0.0)"
    )
    parser.add_argument(
        "--port", "-p",
        type=int,
        default=8080,
        help="Port to listen on (default: 8080)"
    )
    parser.add_argument(
        "--model", "-m",
        default=None,
        help="Path to model or HuggingFace model ID"
    )
    parser.add_argument(
        "--device", "-d",
        default="cpu",
        choices=["cpu", "cuda", "mps"],
        help="Device to run inference on (default: cpu)"
    )
    parser.add_argument(
        "--log-level",
        default="INFO",
        choices=["DEBUG", "INFO", "WARNING", "ERROR"],
        help="Logging level (default: INFO)"
    )
    
    args = parser.parse_args()
    
    from mohawk.engine import InferenceEngine
    from mohawk.api.server import APIServer
    from mohawk.utils.logging_config import setup_logging
    
    # Setup logging
    setup_logging(level=args.log_level)
    
    # Initialize engine with model if provided
    engine = InferenceEngine(device=args.device)
    if args.model:
        engine.load_model(args.model)
    
    # Start server
    server = APIServer(engine=engine, host=args.host, port=args.port)
    server.run(host=args.host, port=args.port)


if __name__ == "__main__":
    main()
