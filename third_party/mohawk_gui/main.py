#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Mohawk Inference Engine GUI - Main Entry Point

Production-ready GUI with:
- Model Library Management (LM Studio-style)
- Real-time Chat Interface
- Performance Monitoring Dashboard
- Session & Queue Management
- Worker Configuration with Multi-device Splitting
- Security Center (PQC + mTLS + JWT)
- System Health Monitor
"""

import sys
import argparse
import os
from pathlib import Path
from urllib.parse import urlparse

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))


def main():
    """Main application entry point."""
    parser = argparse.ArgumentParser(
        description="Mohawk Inference Engine GUI - Professional Dashboard",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python main.py                          # Run with default settings
  python main.py --host 0.0.0.0          # Bind to all interfaces
  python main.py --port 8003             # Use custom port
  python main.py --key-file certs/key.pem # Specify auth key file

The dashboard includes:
  - Model Library (LM Studio-style)
  - Chat Interface with context management
  - Real-time Performance Metrics
  - Session & Queue Manager
  - Worker Configuration
  - Security Center (PQC + mTLS)
  - Conversation History
        """
    )
    
    parser.add_argument(
        "--host",
        default="localhost",
        help="Ghostlink backend host (default: localhost)"
    )
    
    parser.add_argument(
        "--port",
        type=int,
        default=8003,
        help="Ghostlink backend port (default: 8003)"
    )

    parser.add_argument(
        "--backend-url",
        default=None,
        help="Full backend base URL (overrides --host/--port), e.g. http://127.0.0.1:8003"
    )
    
    parser.add_argument(
        "--key-file",
        default=None,
        help="Path to authentication key file"
    )
    
    parser.add_argument(
        "--ssl-enabled",
        action="store_true",
        help="Enable SSL/TLS for connections"
    )
    
    parser.add_argument(
        "--metrics-interval",
        type=int,
        default=1000,
        help="Metrics update interval in ms (default: 1000)"
    )
    
    args = parser.parse_args()
    
    print("=" * 60)
    print("[MOHAWK] Inference Engine GUI v2.1.0")
    print("=" * 60)
    backend_url = args.backend_url
    if not backend_url:
        backend_url = f"http://{args.host}:{args.port}"
    backend_url = backend_url.rstrip("/")

    # Ensure backend URL is usable before constructing the UI.
    parsed = urlparse(backend_url)
    if not parsed.scheme or not parsed.netloc:
        print(f"\n[ERROR] Invalid backend URL: {backend_url}")
        sys.exit(2)

    print(f"Backend: {backend_url}")
    if args.key_file:
        print(f"Auth Key: {args.key_file}")
    print("=" * 60)
    
    try:
        # CRITICAL: Create QApplication FIRST before any QWidgets
        from PyQt6.QtWidgets import QApplication
        app = QApplication(sys.argv)
        
        # NOW create and show the main window
        from main_window import MohawkGUI

        window = MohawkGUI(base_url=backend_url)
        window.show()
        print("\n[INFO] GUI window opened successfully")
        print("[INFO] Connecting to Ghostlink backend services...")
        
        # Run event loop
        sys.exit(app.exec())
        
    except ImportError as e:
        print(f"\n[ERROR] Import error: {e}")
        print("\nPlease install dependencies:")
        print("  pip install PyQt6")
        sys.exit(1)
        
    except Exception as e:
        print(f"\n[ERROR] Error starting application: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
