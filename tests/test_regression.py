#!/usr/bin/env python3
"""
Ghostlink Regression Test Suite

Tests that verify previously fixed bugs remain fixed.
Each test corresponds to a specific bug fix documented in CHANGELOG.md.
"""

import unittest
import requests
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

BACKEND_URL = os.environ.get("GHOSTLINK_BACKEND_URL", "http://127.0.0.1:8003")


class TestRegressionChatTab(unittest.TestCase):
    """Regression tests for ChatTab fixes."""

    def test_chat_message_not_empty_after_send(self):
        """Verify chat endpoint returns non-empty response for valid input."""
        resp = requests.post(
            f"{BACKEND_URL}/api/inference/chat",
            json={"message": "Hello regression test", "max_tokens": 32},
            timeout=30,
        )
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("response", data)
        self.assertGreater(len(data["response"]), 0)

    def test_chat_empty_message_handled(self):
        """Verify empty message does not crash the backend."""
        resp = requests.post(
            f"{BACKEND_URL}/api/inference/chat",
            json={"message": "", "max_tokens": 32},
            timeout=10,
        )
        self.assertIn(resp.status_code, [200, 400])


class TestRegressionModelManagement(unittest.TestCase):
    """Regression tests for model load/unload fixes."""

    def test_model_load_returns_status(self):
        """Verify model load endpoint returns status field."""
        resp = requests.post(
            f"{BACKEND_URL}/api/models/load",
            json={"model": "tinyllama"},
            timeout=30,
        )
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("status", data)

    def test_model_unload_resets_current_model(self):
        """Verify unload sets current_model to 'none'."""
        requests.post(
            f"{BACKEND_URL}/api/models/load",
            json={"model": "tinyllama"},
            timeout=30,
        )
        resp = requests.post(
            f"{BACKEND_URL}/api/models/tinyllama/unload",
            timeout=10,
        )
        self.assertEqual(resp.status_code, 200)

        status = requests.get(f"{BACKEND_URL}/api/models/status", timeout=10)
        current = status.json().get("current_model", "")
        self.assertNotEqual(current, "tinyllama")

    def test_model_list_not_empty(self):
        """Verify /api/models returns at least one model."""
        resp = requests.get(f"{BACKEND_URL}/api/models", timeout=10)
        self.assertEqual(resp.status_code, 200)
        models = resp.json().get("models", [])
        self.assertGreater(len(models), 0)


class TestRegressionAPIEndpoints(unittest.TestCase):
    """Regression tests for API endpoint consistency."""

    def test_health_endpoint_structure(self):
        """Verify /health returns expected fields."""
        resp = requests.get(f"{BACKEND_URL}/health", timeout=5)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("status", data)
        self.assertIn("uptime_s", data)

    def test_settings_endpoint_accessible(self):
        """Verify /api/settings is accessible and returns JSON."""
        resp = requests.get(f"{BACKEND_URL}/api/settings", timeout=10)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("temperature", data)

    def test_metrics_endpoint_structure(self):
        """Verify /api/metrics returns expected fields."""
        resp = requests.get(f"{BACKEND_URL}/api/metrics", timeout=10)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("metrics", data)


if __name__ == "__main__":
    unittest.main(verbosity=2)
