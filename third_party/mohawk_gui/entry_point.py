#!/usr/bin/env python3
"""
Mohawk Inference Engine GUI - Simplified Entry Point
Version: 2.1.0

This is a simplified entry point that can be used directly
or embedded in the PyInstaller executable.
"""

import sys
import os
from pathlib import Path

# Add project root to path
project_root = Path(__file__).parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))

try:
    from mohawk_gui.main import main as run_main
except ImportError:
    # Support direct execution from within third_party/mohawk_gui.
    from main import main as run_main

def main():
    """Main entry point for Mohawk Inference Engine GUI."""
    print("╔═══════════════════════════════════════════════════════════╗")
    print("║   MOHAWK INFERENCE ENGINE GUI v2.1.0                      ║")
    print("║   Production-Ready Multi-Device Inference Management      ║")
    print("╚═══════════════════════════════════════════════════════════╝")
    print()
    
    # Delegate to the canonical launcher so argument parsing and QApplication
    # lifecycle remain consistent with main.py.
    return run_main()

if __name__ == "__main__":
    sys.exit(main())
