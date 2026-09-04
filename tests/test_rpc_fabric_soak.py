import pathlib
import sys
import unittest
from unittest.mock import MagicMock

# Ensure repo root is on python path to import scripts.rpc_fabric_soak
repo_root = pathlib.Path(__file__).resolve().parent.parent
if str(repo_root) not in sys.path:
    sys.path.insert(0, str(repo_root))

from scripts.rpc_fabric_soak import (
    check_clean_failure_or_cancellation,
    check_peers,
    check_rpc_target_drained,
)


class TestRpcFabricSoak(unittest.TestCase):
    def test_check_peers_valid(self):
        data = {"count": 2, "peers": ["node1", "node2"]}
        self.assertEqual(check_peers(data, min_count=2), 2)

    def test_check_peers_fewer_than_min(self):
        data = {"count": 1, "peers": ["node1"]}
        with self.assertRaises(ValueError) as ctx:
            check_peers(data, min_count=2)
        self.assertIn("fewer than 2 peers", str(ctx.exception))

    def test_check_rpc_target_drained_clean(self):
        topology = {
            "placement_plan": {
                "active_rpc_targets": ["127.0.0.1:50052"]
            }
        }
        self.assertTrue(check_rpc_target_drained(topology, contributor_ip="172.30.0.11"))

    def test_check_rpc_target_drained_still_present(self):
        topology = {
            "placement_plan": {
                "active_rpc_targets": ["172.30.0.11:50052"]
            }
        }
        with self.assertRaises(ValueError) as ctx:
            check_rpc_target_drained(topology, contributor_ip="172.30.0.11")
        self.assertIn("still advertised in active_rpc_targets", str(ctx.exception))

    def test_check_clean_failure_or_cancellation_exception(self):
        err = RuntimeError("Connection refused")
        res = check_clean_failure_or_cancellation(err)
        self.assertIn("clean exception caught", res)

    def test_check_clean_failure_or_cancellation_http_503(self):
        mock_resp = MagicMock()
        mock_resp.ok = False
        mock_resp.status_code = 503
        res = check_clean_failure_or_cancellation(mock_resp)
        self.assertIn("HTTP 503", res)

    def test_check_clean_failure_or_cancellation_200_json_error(self):
        mock_resp = MagicMock()
        mock_resp.ok = True
        mock_resp.json.return_value = {"error": "Contributor offline"}
        res = check_clean_failure_or_cancellation(mock_resp)
        self.assertIn("error in JSON", res)

    def test_check_clean_failure_or_cancellation_200_fallback(self):
        mock_resp = MagicMock()
        mock_resp.ok = True
        mock_resp.json.return_value = {"choices": [{"message": {"content": "Local fallback"}}]}
        res = check_clean_failure_or_cancellation(mock_resp)
        self.assertIn("succeeded on local node", res)


if __name__ == "__main__":
    unittest.main()
