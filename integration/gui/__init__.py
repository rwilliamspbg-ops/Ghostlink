"""
Mohawk GUI - Professional Web Interface for the Inference Engine

This module provides a modern, responsive web interface built with Gradio
for interacting with the Mohawk Inference Engine.
"""

from .app import create_gui_app

__all__ = ["create_gui_app"]
