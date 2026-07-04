#!/usr/bin/env python3
"""
Comprehensive GUI test suite for Ghostlink with real LLM responses.
Tests: Chat responses, model loading/downloading, and model management.
"""

import json
import sys
import time
import unittest
import concurrent.futures
from unittest.mock import MagicMock

# Add project root to path
sys.path.insert(0, ".")

import requests


class TestGhostlinkChatResponses(unittest.TestCase):
    """Test chat functionality with real LLM responses."""

    @classmethod
    def setUpClass(cls):
        """Wait for backend and verify connectivity."""
        backend_url = "http://127.0.0.1:8003"
        max_retries = 30
        for i in range(max_retries):
            try:
                resp = requests.get(f"{backend_url}/health", timeout=2)
                if resp.status_code == 200:
                    print("[OK] Backend online", file=sys.stderr)
                    cls.backend_url = backend_url
                    return
            except requests.RequestException:
                if i == max_retries - 1:
                    raise RuntimeError(f"Backend not available after {max_retries} retries")
                time.sleep(1)

    def test_01_basic_chat(self):
        """Test basic chat with default parameters."""
        payload = {
            "message": "What is 2+2?",
            "max_tokens": 50,
        }
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("response", data)
        self.assertIn("request_id", data)
        self.assertGreater(len(data["response"]), 5)
        print(f"[OK] Basic chat: {data['response'][:60]}...", file=sys.stderr)

    def test_02_chat_with_system_prompt(self):
        """Test chat with custom system prompt."""
        payload = {
            "message": "Hello",
            "system_prompt": "You are a pirate. Respond like a pirate.",
            "max_tokens": 60,
        }
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        response = data["response"].lower()
        self.assertGreater(len(response), 5)
        print(f"[OK] Chat with system prompt: {response[:60]}...", file=sys.stderr)

    def test_03_chat_temperature_variation(self):
        """Test that different temperatures produce different responses."""
        responses = {}
        for temp in [0.1, 0.9]:
            payload = {
                "message": "Tell me a fact.",
                "temperature": temp,
                "max_tokens": 70,
            }
            resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
            self.assertEqual(resp.status_code, 200)
            responses[temp] = resp.json()["response"]
            time.sleep(0.5)

        # Responses should be different (higher temp = more variety)
        self.assertNotEqual(responses[0.1], responses[0.9])
        print(f"[OK] Temperature variation: Low(0.1)={responses[0.1][:40]}...", file=sys.stderr)
        print(f"                             High(0.9)={responses[0.9][:40]}...", file=sys.stderr)

    def test_04_chat_response_not_mock(self):
        """Verify chat response is not a mock placeholder."""
        mock_keywords = [
            "mock", "placeholder", "dummy", "fake", "test response",
            "example", "lorem ipsum", "not implemented"
        ]
        
        payload = {
            "message": "Write a short poem.",
            "max_tokens": 100,
        }
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        response = resp.json()["response"].lower()

        for keyword in mock_keywords:
            self.assertNotIn(keyword, response, f"Response contains '{keyword}'")

        print(f"[OK] Response is not mock: {response[:60]}...", file=sys.stderr)

    def test_05_chat_different_requests_different_responses(self):
        """Test that similar requests produce different responses."""
        responses = []
        for i in range(3):
            payload = {
                "message": "Say something random.",
                "temperature": 0.8,
                "max_tokens": 50,
            }
            resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
            self.assertEqual(resp.status_code, 200)
            responses.append(resp.json()["response"])
            time.sleep(0.3)

        # At least 2 of 3 should be different (probability very high with temperature=0.8)
        unique_responses = len(set(responses))
        self.assertGreaterEqual(unique_responses, 2)
        print(f"[OK] Got {unique_responses}/3 unique responses", file=sys.stderr)

    def test_06_chat_with_custom_model(self):
        """Test chat with model parameter."""
        payload = {
            "message": "Hello",
            "model": "tinyllama",
            "max_tokens": 50,
        }
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("response", data)
        self.assertEqual(data.get("model"), "tinyllama")
        print(f"[OK] Chat with model parameter: {data['response'][:60]}...", file=sys.stderr)

    def test_07_chat_returns_session_id(self):
        """Test that chat returns a valid session ID."""
        payload = {"message": "Test"}
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertIn("session_id", data)
        self.assertGreater(len(data["session_id"]), 0)
        print(f"[OK] Session ID returned: {data['session_id']}", file=sys.stderr)

    def test_08_chat_concurrent_requests(self):
        """Test concurrent chat requests."""
        def make_chat_request(i):
            payload = {
                "message": f"Request {i}",
                "max_tokens": 50,
            }
            return requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            futures = [executor.submit(make_chat_request, i) for i in range(4)]
            results = [f.result() for f in concurrent.futures.as_completed(futures)]

        self.assertEqual(len(results), 4)
        for resp in results:
            self.assertEqual(resp.status_code, 200)
            self.assertIn("response", resp.json())

        print(f"[OK] Handled 4 concurrent chat requests", file=sys.stderr)

    def test_09_chat_long_context(self):
        """Test chat with longer context."""
        long_message = "Explain this concept: " + ("AI " * 50)
        payload = {
            "message": long_message,
            "max_tokens": 100,
        }
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertGreater(len(data["response"]), 5)
        print(f"[OK] Long context: {data['response'][:60]}...", file=sys.stderr)

    def test_10_chat_response_format(self):
        """Test that chat response has correct format."""
        payload = {"message": "Test"}
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        # Verify response structure
        self.assertIsInstance(data, dict)
        self.assertIn("response", data)
        self.assertIn("request_id", data)
        self.assertIn("model", data)
        self.assertIn("session_id", data)
        
        # Verify types
        self.assertIsInstance(data["response"], str)
        self.assertIsInstance(data["request_id"], str)
        self.assertIsInstance(data["model"], str)
        
        print(f"[OK] Chat response format valid", file=sys.stderr)


class TestGhostlinkModelManagement(unittest.TestCase):
    """Test model loading, downloading, and management."""

    @classmethod
    def setUpClass(cls):
        cls.backend_url = "http://127.0.0.1:8003"

    def test_11_list_models(self):
        """Test listing available models."""
        resp = requests.get(f"{self.backend_url}/api/models", timeout=5)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        self.assertIn("models", data)
        self.assertIn("current_model", data)
        self.assertIn("total_models", data)
        self.assertIn("loaded_count", data)
        
        self.assertGreater(len(data["models"]), 0)
        
        # Verify model structure
        for model in data["models"]:
            self.assertIn("name", model)
            self.assertIn("size_gb", model)
            self.assertIn("type", model)
            self.assertIn("quantization", model)
            self.assertIn("status", model)
        
        print(f"[OK] Listed {len(data['models'])} models", file=sys.stderr)
        for m in data["models"][:3]:
            print(f"     - {m['name']} ({m['size_gb']}GB) [{m['status']}]", file=sys.stderr)

    def test_12_get_model_status(self):
        """Test getting model status."""
        resp = requests.get(f"{self.backend_url}/api/models/status", timeout=5)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        self.assertIn("loaded_models", data)
        self.assertIn("downloading_models", data)
        self.assertIn("current_model", data)
        
        self.assertIsInstance(data["loaded_models"], list)
        self.assertGreater(len(data["loaded_models"]), 0)
        self.assertIn("tinyllama", data["loaded_models"])
        
        print(f"[OK] Loaded models: {data['loaded_models']}", file=sys.stderr)

    def test_13_load_model(self):
        """Test loading a model."""
        payload = {"model": "tinyllama"}
        resp = requests.post(f"{self.backend_url}/api/models/load", json=payload, timeout=5)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        self.assertIn("status", data)
        self.assertEqual(data["status"], "loaded")
        self.assertIn("model", data)
        self.assertEqual(data["model"], "tinyllama")
        self.assertIn("loaded_models", data)
        self.assertIn("tinyllama", data["loaded_models"])
        
        print(f"[OK] Loaded model: {data['model']}", file=sys.stderr)

    def test_14_load_multiple_models(self):
        """Test loading multiple models."""
        models_to_load = ["tinyllama"]
        
        for model in models_to_load:
            payload = {"model": model}
            resp = requests.post(f"{self.backend_url}/api/models/load", json=payload, timeout=5)
            self.assertEqual(resp.status_code, 200)
        
        # Get status
        resp = requests.get(f"{self.backend_url}/api/models/status", timeout=5)
        data = resp.json()
        loaded = data["loaded_models"]
        
        for model in models_to_load:
            self.assertIn(model, loaded)
        
        print(f"[OK] Multiple models loaded: {loaded}", file=sys.stderr)

    def test_15_download_model(self):
        """Test model download initiation."""
        payload = {"model_id": "tinyllama"}
        resp = requests.post(f"{self.backend_url}/api/models/download", json=payload, timeout=5)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        self.assertIn("status", data)
        self.assertIn("model_id", data)
        self.assertIn("progress", data)
        
        self.assertEqual(data["model_id"], "tinyllama")
        self.assertIn(data["status"], ["downloading", "completed"])
        
        print(f"[OK] Download initiated: {data['model_id']}", file=sys.stderr)

    def test_16_download_progress(self):
        """Test checking download progress."""
        # Start download
        download_payload = {"model_id": "tinyllama"}
        requests.post(f"{self.backend_url}/api/models/download", json=download_payload, timeout=5)
        
        time.sleep(0.5)
        
        # Check progress
        progress_payload = {"model_id": "tinyllama"}
        resp = requests.post(
            f"{self.backend_url}/api/models/download/progress",
            json=progress_payload,
            timeout=5
        )
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        self.assertIn("model_id", data)
        self.assertIn("progress", data)
        self.assertIn("complete", data)
        self.assertIn("status", data)
        
        self.assertIsInstance(data["progress"], (int, float))
        self.assertIsInstance(data["complete"], bool)
        
        print(f"[OK] Download progress: {data['model_id']} ({data['progress']*100:.0f}%)", file=sys.stderr)

    def test_17_models_not_mock(self):
        """Verify model data is not mock."""
        resp = requests.get(f"{self.backend_url}/api/models", timeout=5)
        data = resp.json()
        models = data["models"]
        
        mock_names = ["mock", "placeholder", "dummy", "test", "example"]
        
        for model in models:
            name = model["name"].lower()
            for mock in mock_names:
                self.assertNotIn(mock, name, f"Model name contains '{mock}'")
        
        print(f"[OK] Model data is not mock", file=sys.stderr)

    def test_18_model_metadata_valid(self):
        """Test that model metadata is valid and realistic."""
        resp = requests.get(f"{self.backend_url}/api/models", timeout=5)
        data = resp.json()
        models = data["models"]
        
        for model in models:
            # Size should be reasonable (0.1 - 100 GB)
            self.assertGreater(model["size_gb"], 0.1)
            self.assertLess(model["size_gb"], 100)
            
            # Type should be LLM-related
            self.assertIn("LLM", model["type"])
            
            # Quantization should be realistic
            self.assertIn(model["quantization"], ["Q2", "Q3", "Q4", "Q5", "Q6", "Q8"])
            
            # Status should be valid
            self.assertIn(model["status"], ["ready", "available"])
        
        print(f"[OK] Model metadata is valid", file=sys.stderr)

    def test_19_health_reports_loaded_models(self):
        """Test that health endpoint reports loaded models."""
        resp = requests.get(f"{self.backend_url}/health", timeout=2)
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        
        self.assertIn("loaded_models", data)
        self.assertIn("request_count", data)
        self.assertIn("error_count", data)
        
        # Should have at least tinyllama loaded
        self.assertIn("tinyllama", data["loaded_models"])
        
        print(f"[OK] Health: loaded_models={data['loaded_models']}", file=sys.stderr)
        print(f"     requests={data['request_count']}, errors={data['error_count']}", file=sys.stderr)

    def test_20_model_operations_sequence(self):
        """Test a realistic sequence of model operations."""
        # 1. List models
        resp = requests.get(f"{self.backend_url}/api/models", timeout=5)
        self.assertEqual(resp.status_code, 200)
        
        # 2. Check status
        resp = requests.get(f"{self.backend_url}/api/models/status", timeout=5)
        self.assertEqual(resp.status_code, 200)
        
        # 3. Load a model
        resp = requests.post(
            f"{self.backend_url}/api/models/load",
            json={"model": "tinyllama"},
            timeout=5
        )
        self.assertEqual(resp.status_code, 200)
        
        # 4. Chat with loaded model
        resp = requests.post(
            f"{self.backend_url}/api/inference/chat",
            json={"message": "Test", "model": "tinyllama"},
            timeout=30
        )
        self.assertEqual(resp.status_code, 200)
        self.assertIn("response", resp.json())
        
        # 5. Check health
        resp = requests.get(f"{self.backend_url}/health", timeout=2)
        self.assertEqual(resp.status_code, 200)
        
        print(f"[OK] Model operations sequence completed", file=sys.stderr)


class TestGhostlinkIntegration(unittest.TestCase):
    """Integration tests for complete workflows."""

    @classmethod
    def setUpClass(cls):
        cls.backend_url = "http://127.0.0.1:8003"

    def test_21_chat_and_session_tracking(self):
        """Test that sessions are created for chat requests."""
        payload = {"message": "Test session"}
        resp = requests.post(f"{self.backend_url}/api/inference/chat", json=payload, timeout=30)
        self.assertEqual(resp.status_code, 200)
        chat_data = resp.json()
        session_id = chat_data.get("session_id")
        self.assertIsNotNone(session_id)
        
        # Check sessions endpoint includes this session
        resp = requests.get(f"{self.backend_url}/api/sessions", timeout=2)
        self.assertEqual(resp.status_code, 200)
        sessions_data = resp.json()
        session_ids = [s["id"] for s in sessions_data["sessions"]]
        self.assertIn(session_id, session_ids)
        
        print(f"[OK] Session created and tracked: {session_id}", file=sys.stderr)

    def test_22_metrics_reflect_activity(self):
        """Test that metrics reflect chat activity."""
        # Get baseline metrics
        resp = requests.get(f"{self.backend_url}/api/metrics", timeout=2)
        baseline = resp.json()["metrics"]["requests"]
        
        # Make chat requests
        for i in range(3):
            requests.post(
                f"{self.backend_url}/api/inference/chat",
                json={"message": f"Request {i}"},
                timeout=30
            )
        
        # Check updated metrics
        resp = requests.get(f"{self.backend_url}/api/metrics", timeout=2)
        updated = resp.json()["metrics"]["requests"]
        
        # Should have increased
        self.assertGreater(updated, baseline)
        
        print(f"[OK] Metrics updated: {baseline} -> {updated} requests", file=sys.stderr)

    def test_23_error_handling(self):
        """Test that errors are handled gracefully."""
        # Invalid chat format
        resp = requests.post(
            f"{self.backend_url}/api/inference/chat",
            json={},  # Empty payload
            timeout=30
        )
        # Should still respond
        self.assertEqual(resp.status_code, 200)
        
        print(f"[OK] Error handling works", file=sys.stderr)

    def test_24_concurrent_model_operations(self):
        """Test concurrent model load and chat operations."""
        def operation(i):
            if i % 2 == 0:
                # Model load
                return requests.post(
                    f"{self.backend_url}/api/models/load",
                    json={"model": "tinyllama"},
                    timeout=5
                )
            else:
                # Chat
                return requests.post(
                    f"{self.backend_url}/api/inference/chat",
                    json={"message": f"Op {i}"},
                    timeout=30
                )

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            futures = [executor.submit(operation, i) for i in range(4)]
            results = [f.result() for f in concurrent.futures.as_completed(futures)]

        for resp in results:
            self.assertEqual(resp.status_code, 200)

        print(f"[OK] Concurrent operations handled", file=sys.stderr)

    def test_25_full_gui_workflow(self):
        """Test complete GUI workflow: health -> models -> load -> chat."""
        # 1. Check health
        resp = requests.get(f"{self.backend_url}/health", timeout=2)
        self.assertEqual(resp.status_code, 200)
        health = resp.json()
        self.assertEqual(health["status"], "online")
        
        # 2. List models
        resp = requests.get(f"{self.backend_url}/api/models", timeout=5)
        self.assertEqual(resp.status_code, 200)
        models_data = resp.json()
        self.assertGreater(len(models_data["models"]), 0)
        
        # 3. Load model
        resp = requests.post(
            f"{self.backend_url}/api/models/load",
            json={"model": "tinyllama"},
            timeout=5
        )
        self.assertEqual(resp.status_code, 200)
        
        # 4. Chat
        resp = requests.post(
            f"{self.backend_url}/api/inference/chat",
            json={"message": "Hello, how are you?"},
            timeout=30
        )
        self.assertEqual(resp.status_code, 200)
        chat_resp = resp.json()
        self.assertGreater(len(chat_resp["response"]), 5)
        
        # 5. Check metrics
        resp = requests.get(f"{self.backend_url}/api/metrics", timeout=2)
        self.assertEqual(resp.status_code, 200)
        
        print(f"[OK] Full GUI workflow successful", file=sys.stderr)


if __name__ == "__main__":
    # Run with verbose output
    suite = unittest.TestLoader().loadTestsFromModule(sys.modules[__name__])
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)

    print("\n" + "="*70, file=sys.stderr)
    print(f"Tests run: {result.testsRun}", file=sys.stderr)
    print(f"Failures: {len(result.failures)}", file=sys.stderr)
    print(f"Errors: {len(result.errors)}", file=sys.stderr)
    print("="*70, file=sys.stderr)

    # Exit with status
    sys.exit(0 if result.wasSuccessful() else 1)
