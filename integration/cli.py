"""
Command-line interface for Mohawk Inference Engine
"""

import argparse
import sys
from typing import Optional


def create_parser() -> argparse.ArgumentParser:
    """Create the argument parser"""
    parser = argparse.ArgumentParser(
        prog="mohawk",
        description="Mohawk Inference Engine - High-performance local LLM inference",
    )
    
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # Server command
    server_parser = subparsers.add_parser("serve", help="Start the API server")
    server_parser.add_argument(
        "--host", "-H",
        default="0.0.0.0",
        help="Host to bind to (default: 0.0.0.0)",
    )
    server_parser.add_argument(
        "--port", "-p",
        type=int,
        default=8080,
        help="Port to listen on (default: 8080)",
    )
    server_parser.add_argument(
        "--model", "-m",
        default=None,
        help="Path to model or HuggingFace model ID",
    )
    server_parser.add_argument(
        "--device", "-d",
        default="cpu",
        choices=["cpu", "cuda", "mps"],
        help="Device to run inference on (default: cpu)",
    )
    server_parser.add_argument(
        "--log-level",
        default="INFO",
        choices=["DEBUG", "INFO", "WARNING", "ERROR"],
        help="Logging level (default: INFO)",
    )
    
    # Generate command (CLI inference)
    gen_parser = subparsers.add_parser("generate", help="Run inference from CLI")
    gen_parser.add_argument(
        "--model", "-m",
        required=True,
        help="Path to model or HuggingFace model ID",
    )
    gen_parser.add_argument(
        "--prompt", "-P",
        default=None,
        help="Input prompt (reads from stdin if not provided)",
    )
    gen_parser.add_argument(
        "--max-tokens", "-n",
        type=int,
        default=100,
        help="Maximum tokens to generate (default: 100)",
    )
    gen_parser.add_argument(
        "--temperature", "-t",
        type=float,
        default=0.7,
        help="Sampling temperature (default: 0.7)",
    )
    gen_parser.add_argument(
        "--stream", "-s",
        action="store_true",
        help="Stream output token by token",
    )
    
    return parser


def main(args: Optional[list] = None):
    """Main entry point for CLI"""
    parser = create_parser()
    parsed_args = parser.parse_args(args)
    
    if parsed_args.command is None:
        parser.print_help()
        sys.exit(0)
    
    if parsed_args.command == "serve":
        run_server(parsed_args)
    elif parsed_args.command == "generate":
        run_generate(parsed_args)
    else:
        parser.print_help()
        sys.exit(1)


def run_server(args: argparse.Namespace):
    """Start the API server"""
    from .engine import InferenceEngine
    from .api.server import APIServer
    from .utils.logging_config import setup_logging
    
    # Setup logging
    setup_logging(level=args.log_level)
    
    # Initialize engine with model if provided
    engine = InferenceEngine(device=args.device)
    if args.model:
        engine.load_model(args.model)
    
    # Start server
    server = APIServer(engine=engine, host=args.host, port=args.port)
    server.run(host=args.host, port=args.port)


def run_generate(args: argparse.Namespace):
    """Run inference from CLI"""
    from .engine import InferenceEngine
    from .utils.logging_config import setup_logging
    
    # Setup logging
    setup_logging(level="WARNING")
    
    # Initialize engine
    engine = InferenceEngine()
    engine.load_model(args.model)
    
    # Get prompt
    if args.prompt:
        prompt = args.prompt
    else:
        prompt = sys.stdin.read().strip()
    
    if not prompt:
        print("Error: No prompt provided", file=sys.stderr)
        sys.exit(1)
    
    # Generate
    if args.stream:
        for token in engine.generate(
            prompt=prompt,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
            stream=True,
        ):
            print(token, end="", flush=True)
        print()
    else:
        result = engine.generate(
            prompt=prompt,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
        )
        print(result.text)


if __name__ == "__main__":
    main()
