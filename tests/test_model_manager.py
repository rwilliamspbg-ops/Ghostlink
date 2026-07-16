import threading
import unittest
from http.server import HTTPServer
from unittest.mock import patch

import requests

from model_manager import ModelManager, ModelManagementHandler, model_manager


class ModelManagerTests(unittest.TestCase):
    def test_load_model_succeeds_when_ollama_is_unavailable(self):
        with patch("model_manager.requests.get", side_effect=requests.ConnectionError("ollama down")), patch(
            "model_manager.requests.post", side_effect=requests.ConnectionError("ollama down")
        ):
            manager = ModelManager()
            result = manager.load_model("tinyllama")

        self.assertEqual(result["status"], "Loaded")
        self.assertIn("tinyllama", manager.loaded_models)
        self.assertEqual(manager.current_model, "tinyllama")


class TestModelManagementHTTPAPI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = HTTPServer(("127.0.0.1", 0), ModelManagementHandler)
        cls.port = cls.server.server_port
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base_url = f"http://127.0.0.1:{cls.port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.thread.join(timeout=5)

    def test_api_models_includes_current_model(self):
        response = requests.get(f"{self.base_url}/api/models", timeout=5)
        data = response.json()
        self.assertIn("current_model", data)

    def test_api_models_status_includes_current_model(self):
        response = requests.get(f"{self.base_url}/api/models/status", timeout=5)
        data = response.json()
        self.assertIn("current_model", data)
        self.assertIn("loaded_models", data)

    def test_load_and_unload_model_returns_current_model(self):
        payload = {"model": "tinyllama"}
        load_response = requests.post(f"{self.base_url}/api/models/load", json=payload, timeout=5).json()
        self.assertEqual(load_response.get("current_model"), "tinyllama")
        self.assertIn("tinyllama", load_response.get("loaded_models", []))

        unload_response = requests.post(f"{self.base_url}/api/models/unload", json=payload, timeout=5).json()
        self.assertEqual(unload_response.get("current_model"), "")
        self.assertNotIn("tinyllama", unload_response.get("loaded_models", []))


if __name__ == "__main__":
    unittest.main()
