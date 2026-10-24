from __future__ import annotations

import os
import re
import unittest
from pathlib import Path
from unittest import mock

from agent_policy import load_policy
from prepare_codex import request_oidc_token


ROOT = Path(__file__).parents[2]
POLICY = load_policy()


class WorkflowConfigurationTests(unittest.TestCase):
    def test_every_external_action_is_commit_pinned(self) -> None:
        pattern = re.compile(r"^\s*uses:\s*([^#\s]+)", re.MULTILINE)
        for workflow in (ROOT / ".github" / "workflows").glob("*.yml"):
            for reference in pattern.findall(workflow.read_text(encoding="utf-8")):
                if reference.startswith("./"):
                    continue
                self.assertRegex(reference, r"^[^@]+@[0-9a-f]{40}$", workflow.as_posix())

    def test_codex_configuration_is_central_pin(self) -> None:
        agent_workflows = [
            ROOT / ".github/workflows/agent-triage.yml",
            ROOT / ".github/workflows/agent-plan.yml",
            ROOT / ".github/workflows/agent-implement.yml",
            ROOT / ".github/workflows/ci.yml",
        ]
        expected_action = "openai/codex-action@" + POLICY.raw["agents"]["codex_action_sha"]
        for workflow in agent_workflows:
            text = workflow.read_text(encoding="utf-8")
            if "codex-action@" not in text:
                continue
            self.assertIn(expected_action, text)
            self.assertIn(f"model: {POLICY.raw['agents']['model']}", text)
            self.assertIn(f"effort: {POLICY.raw['agents']['effort']}", text)
            self.assertIn(f"codex-version: {POLICY.raw['agents']['codex_version']}", text)
            self.assertIn("safety-strategy: drop-sudo", text)

    def test_missing_oidc_claim_environment_fails_closed(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(KeyError):
                request_oidc_token("test-audience")


if __name__ == "__main__":
    unittest.main()
