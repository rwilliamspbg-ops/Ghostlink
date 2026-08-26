#!/usr/bin/env python3
"""Unit tests for AuthManager token generation, verification, and key caching."""

import asyncio
import os
import tempfile
import unittest
from third_party.mohawk_gui.auth_manager import AuthManager


class TestAuthManager(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.key_path = os.path.join(self.tmpdir.name, "jwt_private.pem")
        self.auth = AuthManager(secret_key_path=self.key_path)

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_key_caching_initialization(self):
        """Verify keys are cached during AuthManager initialization."""
        self.assertIsNotNone(self.auth._private_key)
        self.assertIsNotNone(self.auth._public_key)

    def test_generate_and_verify_token(self):
        """Test token generation and verification using cached keys."""
        async def run_test():
            token = await self.auth.generate_session_token("user123", ["admin", "user"])
            self.assertIsInstance(token, str)

            result = await self.auth.verify_token(token)
            self.assertTrue(result["valid"])
            self.assertEqual(result["user_id"], "user123")
            self.assertEqual(result["roles"], ["admin", "user"])

        asyncio.run(run_test())

    def test_invalid_token(self):
        """Test verification of an invalid token."""
        async def run_test():
            result = await self.auth.verify_token("invalid.token.str")
            self.assertFalse(result["valid"])
            self.assertIn("reason", result)

        asyncio.run(run_test())

    def test_key_caching_performance_behavior(self):
        """Verify that token generation does not perform file reads repeatedly."""
        async def run_test():
            # Delete physical key file to confirm cached key is used without file read
            os.remove(self.key_path)
            token = await self.auth.generate_session_token("cached_user")
            result = await self.auth.verify_token(token)
            self.assertTrue(result["valid"])
            self.assertEqual(result["user_id"], "cached_user")

        asyncio.run(run_test())


if __name__ == "__main__":
    unittest.main()
