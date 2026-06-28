#!/usr/bin/env python3
"""
Mohawk Inference Engine GUI - Simplified Entry Point
Version: 2.1.0

This is a simplified entry point that can be used directly
or embedded in the PyInstaller executable.
"""

import sys
import os

# Add project root to path
project_root = Path(__file__).parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))

from mohawk_gui.main import main

def main():
    """Main entry point for Mohawk Inference Engine GUI."""
    print("╔═══════════════════════════════════════════════════════════╗")
    print("║   MOHAWK INFERENCE ENGINE GUI v2.1.0                      ║")
    print("║   Production-Ready Multi-Device Inference Management      ║")
    print("╚═══════════════════════════════════════════════════════════╝")
    print()
    
    # Parse command line arguments
    import argparse
    
    parser = argparse.ArgumentParser(
        description="Mohawk Inference Engine GUI - Production Ready"
    )
    parser.add_argument(
        "--host", 
        default="localhost",
        help="Worker host (default: localhost)"
    )
    parser.add_argument(
        "--port", 
        type=int, 
        default=8003,
        help="Worker port (default: 8003)"
    )
    parser.add_argument(
        "--config",
        default="config.toml",
        help="Configuration file path"
    )
    
    args = parser.parse_args()
    
    # Initialize and run GUI
    from mohawk_gui.main_window import MohawkGUI
    
    gui = MohawkGUI()
    gui.show()
    
    return gui.exec()

if __name__ == "__main__":
    sys.exit(main())
