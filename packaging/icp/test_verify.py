"""Checks that mainnet verification rejects a local trust key or a miswired backend."""

import unittest
from urllib.parse import quote

from verify import MAINNET_ROOT_KEY, verify_environment


def cookie(key, cloud="backend"):
    return "ic_env=" + quote(f"PUBLIC_CANISTER_ID:cloud={cloud}&ic_root_key={key.hex()}") + "; Secure; SameSite=Lax"


class RuntimeEnvironmentTests(unittest.TestCase):
    def test_mainnet_accepts_only_the_canonical_key(self):
        verify_environment(cookie(MAINNET_ROOT_KEY), "backend", True)
        with self.assertRaisesRegex(RuntimeError, "canonical"):
            verify_environment(cookie(bytes(133)), "backend", True)

    def test_local_network_can_use_its_own_key(self):
        verify_environment(cookie(bytes(133)), "backend", False)

    def test_backend_must_match_the_deployed_mapping(self):
        with self.assertRaisesRegex(RuntimeError, "wrong backend"):
            verify_environment(cookie(MAINNET_ROOT_KEY, "other-backend"), "backend", True)

    def test_missing_or_truncated_environment_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "Missing"):
            verify_environment("", "backend", True)
        with self.assertRaisesRegex(RuntimeError, "trust key"):
            verify_environment(cookie(b"short"), "backend", True)


if __name__ == "__main__":
    unittest.main()
