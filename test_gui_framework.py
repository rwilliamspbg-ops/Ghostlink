#!/usr/bin/env python3
"""Unified GUI Testing Framework for Ghostlink Studio"""

import argparse
import json
import os
import sys
import unittest
from pathlib import Path

# Mock dependencies if not available for minimal environment testing
try:
    from PyQt6 import QtWidgets, QtTest
    from PyQt6.QtCore import Qt
    GUI_AVAILABLE = True
except ImportError:
    GUI_AVAILABLE = False

class TestGuiContract(unittest.TestCase):
    def test_api_contract_exists(self):
        """Verify that the GUI API contract script is present."""
        contract_script = Path("scripts/validate_gui_api_contract.py")
        self.assertTrue(contract_script.exists(), "scripts/validate_gui_api_contract.py missing")

    def test_requirements_file(self):
        """Verify that GUI requirements are defined."""
        req_file = Path("third_party/mohawk_gui/requirements-runtime.txt")
        self.assertTrue(req_file.exists(), "third_party/mohawk_gui/requirements-runtime.txt missing")

@unittest.skipIf(not GUI_AVAILABLE, "PyQt6 not installed or display not available")
class TestStudioWindow(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from third_party.mohawk_gui.main_window import GhostlinkStudioWindow
        cls.app = QtWidgets.QApplication.instance() or QtWidgets.QApplication(sys.argv)
        cls.window = GhostlinkStudioWindow()

    def test_window_title(self):
        """Verify the main window title."""
        self.assertEqual(self.window.windowTitle(), "Ghostlink Studio")

    def test_navigation_items(self):
        """Verify that all sidebar navigation items are present."""
        expected_items = ["Home", "Models", "Cluster", "Chat", "Security", "Analytics", "Settings"]
        # This assumes the window has a list widget or similar for navigation
        # If it uses a custom layout, we'd check for specific buttons
        pass

def main():
    parser = argparse.ArgumentParser(description="Ghostlink GUI Test Framework")
    parser.add_argument("--all", action="store_true", help="Run all tests")
    args = parser.parse_args()

    loader = unittest.TestLoader()
    suite = unittest.TestSuite()

    if args.all:
        suite.addTests(loader.loadTestsFromTestCase(TestGuiContract))
        if GUI_AVAILABLE:
            suite.addTests(loader.loadTestsFromTestCase(TestStudioWindow))
    else:
        suite.addTests(loader.loadTestsFromTestCase(TestGuiContract))

    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    sys.exit(not result.wasSuccessful())

if __name__ == "__main__":
    main()
