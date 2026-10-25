"""Unit tests for ``scripts/kernel_token.py`` — no kernel, no network."""

import sys
import tempfile
import unittest
from pathlib import Path

_SCRIPTS_DIR = Path(__file__).resolve().parent.parent
if str(_SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPTS_DIR))

from kernel_token import ENV_VAR, REPO_ROOT, TOKEN_FILE, kernel_headers, resolve_token  # noqa: E402


class ResolveTokenTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.token_file = Path(self._tmp.name) / ".tylluan-token"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_env_wins_over_file(self) -> None:
        self.token_file.write_text("file-token\n", encoding="utf-8")
        token = resolve_token(env={ENV_VAR: "env-token"}, token_file=self.token_file)
        self.assertEqual(token, "env-token")

    def test_file_used_when_env_missing(self) -> None:
        self.token_file.write_text("  file-token \n", encoding="utf-8")
        self.assertEqual(resolve_token(env={}, token_file=self.token_file), "file-token")

    def test_empty_env_falls_through_to_file(self) -> None:
        self.token_file.write_text("file-token", encoding="utf-8")
        token = resolve_token(env={ENV_VAR: "   "}, token_file=self.token_file)
        self.assertEqual(token, "file-token")

    def test_missing_file_returns_none(self) -> None:
        self.assertIsNone(resolve_token(env={}, token_file=self.token_file))

    def test_blank_file_returns_none(self) -> None:
        self.token_file.write_text("  \n\t\n", encoding="utf-8")
        self.assertIsNone(resolve_token(env={}, token_file=self.token_file))

    def test_repo_token_file_lives_at_repo_root(self) -> None:
        self.assertEqual(TOKEN_FILE.parent, REPO_ROOT)
        self.assertGreaterEqual(len(REPO_ROOT.parts), 1)


class KernelHeadersTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.token_file = Path(self._tmp.name) / ".tylluan-token"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_no_token_means_no_authorization_header(self) -> None:
        headers = kernel_headers(env={}, token_file=self.token_file)
        self.assertEqual(headers, {})

    def test_extra_headers_preserved_without_token(self) -> None:
        headers = kernel_headers({"Content-Type": "application/json"}, env={}, token_file=self.token_file)
        self.assertEqual(headers, {"Content-Type": "application/json"})

    def test_token_becomes_bearer_and_keeps_extra(self) -> None:
        self.token_file.write_text("s3cr3t-from-file", encoding="utf-8")
        headers = kernel_headers(
            {"Content-Type": "application/json"}, env={}, token_file=self.token_file
        )
        self.assertEqual(headers["Authorization"], "Bearer s3cr3t-from-file")
        self.assertEqual(headers["Content-Type"], "application/json")

    def test_env_token_wins_over_file_for_headers(self) -> None:
        self.token_file.write_text("file-token", encoding="utf-8")
        headers = kernel_headers(env={ENV_VAR: "env-token"}, token_file=self.token_file)
        self.assertEqual(headers["Authorization"], "Bearer env-token")


if __name__ == "__main__":
    unittest.main()
