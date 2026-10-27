from __future__ import annotations

import os
import unittest
from contextlib import redirect_stderr
from io import StringIO
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

import local_agent_workflow as workflow


class LocalAgentWorkflowTests(unittest.TestCase):
    def test_slugify_produces_branch_safe_text(self) -> None:
        self.assertEqual("fix-repeated-headers", workflow.slugify("Fix repeated headers!"))
        self.assertEqual("work", workflow.slugify("!!!"))
        self.assertLessEqual(len(workflow.slugify("a" * 100)), 48)

    def test_codex_environment_removes_github_and_ssh_credentials(self) -> None:
        with TemporaryDirectory() as directory, mock.patch.dict(
            os.environ,
            {
                "GH_TOKEN": "secret",
                "GITHUB_TOKEN": "secret",
                "GITHUB_ACTIONS": "true",
                "SSH_AUTH_SOCK": "/tmp/agent.sock",
                "OPENAI_API_KEY": "kept-for-codex",
            },
            clear=True,
        ):
            environment = workflow.codex_environment(Path(directory) / "gh")

        self.assertNotIn("GH_TOKEN", environment)
        self.assertNotIn("GITHUB_TOKEN", environment)
        self.assertNotIn("GITHUB_ACTIONS", environment)
        self.assertNotIn("SSH_AUTH_SOCK", environment)
        self.assertEqual("kept-for-codex", environment["OPENAI_API_KEY"])
        self.assertEqual("0", environment["GIT_TERMINAL_PROMPT"])

    def test_quick_mode_excludes_nix_checks(self) -> None:
        quick_names = {name for name, _ in workflow.QUICK_CHECKS}
        nix_names = {name for name, _ in workflow.NIX_CHECKS}

        self.assertEqual({"fmt", "clippy", "test", "build"}, quick_names)
        self.assertEqual({"nix", "docker"}, nix_names)

    def test_negative_repair_count_is_rejected(self) -> None:
        with redirect_stderr(StringIO()), self.assertRaises(SystemExit):
            workflow.parse_args(["1", "--max-repairs", "-1"])


if __name__ == "__main__":
    unittest.main()
