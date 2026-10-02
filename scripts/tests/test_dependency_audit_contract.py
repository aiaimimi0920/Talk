"""Keep the independent Rust dependency gate fail-closed and read-only."""

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DependencyAuditContract(unittest.TestCase):
    def test_audit_runs_on_pull_requests_main_and_schedule(self):
        workflow = (ROOT / ".github/workflows/dependency-audit.yml").read_text()
        self.assertRegex(workflow, r"pull_request:\s+branches: \[main\]")
        self.assertRegex(workflow, r"push:\s+branches: \[main\]")
        self.assertIn("schedule:", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("timeout-minutes: 15", workflow)

    def test_audit_does_not_ignore_findings_or_mutate_dependencies(self):
        workflow = (ROOT / ".github/workflows/dependency-audit.yml").read_text()
        self.assertIn("cargo install cargo-audit --locked --version 0.22.2", workflow)
        self.assertIn("cargo audit --file Cargo.lock --deny unsound", workflow)
        self.assertIn("test_dependency_audit_contract.py", workflow)
        self.assertNotRegex(workflow, r"continue-on-error|--ignore|\|\| true|cargo update")
        self.assertTrue((ROOT / "Cargo.lock").is_file())

    def test_actions_are_pinned_and_permissions_are_read_only(self):
        workflow = (ROOT / ".github/workflows/dependency-audit.yml").read_text()
        self.assertIn("contents: read", workflow)
        self.assertIn("persist-credentials: false", workflow)
        self.assertNotRegex(workflow, r"pull_request_target|secrets\.|: write")
        refs = re.findall(r"uses: (\S+)", workflow)
        self.assertEqual(len(refs), 2)
        for ref in refs:
            self.assertRegex(ref, r"@[a-f0-9]{40}$")


if __name__ == "__main__":
    unittest.main()
