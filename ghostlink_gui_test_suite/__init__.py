"""
Ghostlink GUI Test Suite - Main Package

This package contains comprehensive testing utilities for Ghostlink's 
GUI functionality, including model management, chat interface, and session handling.
"""

__version__ = "1.0.0"
__author__ = "Sovereign Mohawk LLC"

# Import core test components
from .test_gui_framework import TestGhostlinkGUI
from .performance_tester import PerformanceTester

# Define what gets imported with 'import *'
__all__ = [
    'TestGhostlinkGUI',
    'PerformanceTester'
]

# Package metadata  
__metadata__ = {
    "name": "ghostlink-gui-tests",
    "version": __version__,
    "description": "Comprehensive GUI testing framework for Ghostlink project",
    "author": "Sovereign Mohawk LLC", 
    "license": "MIT"
}