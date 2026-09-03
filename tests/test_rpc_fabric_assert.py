import json
import pathlib
import sys
import unittest

# Ensure repo root is on python path to import scripts.rpc_fabric_assert
repo_root = pathlib.Path(__file__).resolve().parent.parent
if str(repo_root) not in sys.path:
    sys.path.insert(0, str(repo_root))

from scripts.rpc_fabric_assert import (
    check_peers,
    check_placement_and_offload,
    check_real_inference,
    check_rpc_log_evidence,
)


class TestRpcFabricAssert(unittest.TestCase):
    def setUp(self):
        self.testdata_dir = repo_root / "scripts" / "testdata"

    def test_check_peers_valid(self):
        data = {"count": 2, "peers": ["node1", "node2"]}
        self.assertEqual(check_peers(data), 2)

    def test_check_peers_invalid(self):
        data = {"count": 1, "peers": ["node1"]}
        with self.assertRaises(ValueError) as ctx:
            check_peers(data)
        self.assertIn("fewer than 2 peers", str(ctx.exception))

    def test_check_real_inference_valid(self):
        data = {
            "choices": [
                {
                    "message": {
                        "content": "Once upon a time",
                        "real_inference": True,
                    }
                }
            ]
        }
        real_inf, content = check_real_inference(data)
        self.assertTrue(real_inf)
        self.assertEqual(content, "Once upon a time")

    def test_check_real_inference_missing_flag(self):
        data = {
            "choices": [
                {
                    "message": {
                        "content": "Once upon a time",
                        "real_inference": False,
                    }
                }
            ]
        }
        with self.assertRaises(ValueError) as ctx:
            check_real_inference(data)
        self.assertIn("real_inference was not true", str(ctx.exception))

    def test_check_rpc_log_evidence_good(self):
        good_log = (self.testdata_dir / "good_rpc.log").read_text()
        result = check_rpc_log_evidence(good_log)
        self.assertIn("connection evidence found", result)

    def test_check_rpc_log_evidence_empty(self):
        empty_log = (self.testdata_dir / "empty_rpc.log").read_text()
        with self.assertRaises(ValueError) as ctx:
            check_rpc_log_evidence(empty_log)
        self.assertIn("is empty", str(ctx.exception))

    def test_check_rpc_log_evidence_no_connect(self):
        no_connect_log = (self.testdata_dir / "no_connect_rpc.log").read_text()
        with self.assertRaises(ValueError) as ctx:
            check_rpc_log_evidence(no_connect_log)
        self.assertIn("no recognizable connection evidence", str(ctx.exception))

    def test_check_placement_and_offload_ngl0_valid(self):
        topology = json.loads((self.testdata_dir / "topology_ngl0.json").read_text())
        settings = {"ngl": 0}
        label = check_placement_and_offload(topology, settings)
        self.assertIn("connectivity-only", label)

    def test_check_placement_and_offload_ngl0_falsely_claims_active(self):
        topology = {
            "placement_plan": {
                "distributed_active": True,
                "active_rpc_targets": ["172.30.0.11:50052"],
            }
        }
        settings = {"ngl": 0}
        with self.assertRaises(ValueError) as ctx:
            check_placement_and_offload(topology, settings)
        self.assertIn("-ngl is 0 (CPU-only) but placement plan claims active compute split", str(ctx.exception))

    def test_check_placement_and_offload_ngl0_rejected_if_require_compute_split(self):
        topology = json.loads((self.testdata_dir / "topology_ngl0.json").read_text())
        settings = {"ngl": 0}
        with self.assertRaises(ValueError) as ctx:
            check_placement_and_offload(topology, settings, require_compute_split=True)
        self.assertIn("connectivity-only mode", str(ctx.exception))

    def test_check_placement_and_offload_gpu_valid(self):
        topology = json.loads((self.testdata_dir / "topology_gpu.json").read_text())
        settings = {"ngl": 30}
        label = check_placement_and_offload(topology, settings)
        self.assertIn("compute-split", label)


if __name__ == "__main__":
    unittest.main()
