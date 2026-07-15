#!/usr/bin/env python3
"""Unit tests for Mohawk GUI API contract manifest."""

import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent
CONTRACT = ROOT / "api_contract.json"

REQUIRED_ENDPOINTS = {
    "/api/inference/chat",
    "/api/metrics",
    "/api/models",
    "/api/models/download",
    "/api/models/load",
    "/api/queue",
    "/api/security/jwt/refresh",
    "/api/security/pqc/enable",
    "/api/sessions",
    "/api/sessions/{session_id}/cancel",
    "/api/workers",
    "/api/workers/add",
    "/api/workers/connect",
}


class ApiContractTests(unittest.TestCase):
    def test_contract_contains_required_endpoints(self):
        payload = json.loads(CONTRACT.read_text(encoding="utf-8"))
        endpoints = payload.get("endpoints", [])
        self.assertIsInstance(endpoints, list)

        endpoint_set = set(endpoints)
        missing = REQUIRED_ENDPOINTS - endpoint_set
        self.assertEqual(set(), missing, f"Missing required endpoints: {sorted(missing)}")

    def test_contract_endpoints_are_unique(self):
        payload = json.loads(CONTRACT.read_text(encoding="utf-8"))
        endpoints = payload.get("endpoints", [])
        self.assertEqual(len(endpoints), len(set(endpoints)), "Contract has duplicate endpoints")


if __name__ == "__main__":
    unittest.main()
