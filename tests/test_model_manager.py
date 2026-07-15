import unittest
from unittest.mock import patch

import requests

from model_manager import ModelManager


class ModelManagerTests(unittest.TestCase):
    def test_load_model_succeeds_when_ollama_is_unavailable(self):
        with patch("model_manager.requests.get", side_effect=requests.ConnectionError("ollama down")), patch(
            "model_manager.requests.post", side_effect=requests.ConnectionError("ollama down")
        ):
            manager = ModelManager()
            result = manager.load_model("tinyllama")

        self.assertEqual(result["status"], "loaded")
        self.assertIn("tinyllama", manager.loaded_models)


if __name__ == "__main__":
    unittest.main()
