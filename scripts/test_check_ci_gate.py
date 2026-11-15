import json
import os
import subprocess
import sys
import unittest

from check_ci_gate import WORKFLOW, check_results, required_jobs


class GateTests(unittest.TestCase):
    def setUp(self):
        self.workflow = WORKFLOW.read_text()
        self.expected = required_jobs(self.workflow)

    def test_real_workflow_and_successful_results(self):
        self.assertIn("image-scan", self.expected)
        check_results(self.expected, self.results())

    def results(self):
        return {job: {"result": "success"} for job in self.expected}

    def test_unsuccessful_results_are_rejected(self):
        for status in ("failure", "cancelled", "skipped", "", None, "neutral"):
            with self.subTest(status=status):
                results = self.results()
                results["image-scan"]["result"] = status
                with self.assertRaises(ValueError):
                    check_results(self.expected, results)

    def test_workflow_command_fails_closed(self):
        script = WORKFLOW.parents[2] / "scripts/check_ci_gate.py"
        for payload, succeeds in (
            (json.dumps(self.results()), True),
            ("{}", False),
            ("not json", False),
            (None, False),
        ):
            with self.subTest(payload=payload):
                env = os.environ.copy()
                env.pop("NEEDS_JSON", None)
                if payload is not None:
                    env["NEEDS_JSON"] = payload
                result = subprocess.run(
                    [sys.executable, str(script), "--results"],
                    env=env,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode == 0, succeeds, result.stderr)

    def test_missing_or_unexpected_jobs_are_rejected(self):
        for missing, extra in ((True, False), (False, True)):
            with self.subTest(missing=missing, extra=extra):
                results = self.results()
                if missing:
                    del results["image-scan"]
                if extra:
                    results["new-job"] = {"result": "success"}
                with self.assertRaises(ValueError):
                    check_results(self.expected, results)

    def test_malformed_results_are_rejected(self):
        for value in (None, [], "success", {}, {"result": True}):
            with self.subTest(value=value):
                results = self.results()
                results["image-scan"] = value
                with self.assertRaises(ValueError):
                    check_results(self.expected, results)
        with self.assertRaises(ValueError):
            check_results(self.expected, [])

    def test_new_job_requires_gate_dependency(self):
        with self.assertRaisesRegex(ValueError, "coverage mismatch"):
            required_jobs(self.workflow + "\n  new-job:\n    runs-on: ubuntu-latest\n")

    def test_removed_job_requires_gate_update(self):
        with self.assertRaisesRegex(ValueError, "coverage mismatch"):
            required_jobs(self.workflow.replace("  fmt:", "  renamed-fmt:"))

    def test_comments_do_not_add_required_jobs(self):
        self.assertEqual(
            required_jobs(self.workflow + "\n  # optional-job:\n"), self.expected
        )

    def test_unsupported_job_declarations_fail_closed(self):
        for declaration in ('  "new-job":', "  new-job: *template", "  <<: *jobs"):
            with self.subTest(declaration=declaration):
                with self.assertRaisesRegex(ValueError, "unsupported jobs syntax"):
                    required_jobs(self.workflow + "\n" + declaration + "\n")

    def test_duplicate_jobs_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate job"):
            required_jobs(self.workflow + "\n  fmt:\n")

    def test_gate_must_run_after_failed_or_skipped_dependencies(self):
        with self.assertRaisesRegex(ValueError, "always"):
            required_jobs(self.workflow.replace("    if: always()", "    if: success()"))

    def test_job_continue_on_error_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "continue-on-error"):
            required_jobs(self.workflow.replace("  fmt:\n", "  fmt:\n    continue-on-error: true\n"))

    def test_unsupported_job_properties_fail_closed(self):
        for property in ('"continue-on-error": true', 'continue-on-error : true', '<<: {continue-on-error: true}'):
            with self.subTest(property=property):
                with self.assertRaisesRegex(ValueError, "unsupported job property"):
                    required_jobs(self.workflow.replace("  fmt:\n", f"  fmt:\n    {property}\n"))

    def test_duplicate_job_properties_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate property"):
            required_jobs(self.workflow.replace("    if: always()", "    if: always()\n    if: false"))

    def test_reindented_job_properties_are_rejected(self):
        start = self.workflow.index("  fmt:\n") + len("  fmt:\n")
        end = self.workflow.index("  actionlint:\n")
        for spaces in (1, 2, 4):
            with self.subTest(spaces=spaces):
                body = "\n".join(" " * spaces + line if line.strip() else line for line in self.workflow[start:end].splitlines())
                workflow = self.workflow[:start] + body + "\n" + " " * (4 + spaces) + "continue-on-error: true\n\n" + self.workflow[end:]
                with self.assertRaisesRegex(ValueError, "four-space indentation"):
                    required_jobs(workflow)

    def test_duplicate_gate_dependencies_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "distinct"):
            required_jobs(self.workflow.replace("needs: [fmt,", "needs: [fmt, fmt,"))


if __name__ == "__main__":
    unittest.main()
