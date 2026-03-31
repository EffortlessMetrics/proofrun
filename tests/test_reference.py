import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REF = ROOT / "reference" / "proofrun_ref.py"
FIXTURE_REPO = ROOT / "fixtures" / "demo-workspace" / "repo"
COMMITS = json.loads((ROOT / "fixtures" / "demo-workspace" / "sample" / "commits.json").read_text())


class ReferenceCliTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.repo = Path(self.tempdir.name) / "repo-copy"
        shutil.copytree(FIXTURE_REPO, self.repo)
        subprocess.run(["git", "config", "--global", "--add", "safe.directory", str(self.repo)], check=True)

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def run_ref(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(REF), "--repo", str(self.repo), *args],
            text=True,
            capture_output=True,
            check=True,
        )

    def test_core_change_plan_contains_expected_obligations(self) -> None:
        self.run_ref(
            "plan",
            "--base",
            COMMITS["initial"],
            "--head",
            COMMITS["core_change"],
            "--profile",
            "ci",
        )
        plan = json.loads((self.repo / ".proofrun" / "plan.json").read_text())
        obligations = [item["id"] for item in plan["obligations"]]
        self.assertEqual(
            obligations,
            ["pkg:core:mutation-diff", "pkg:core:tests", "workspace:smoke"],
        )
        selected = [item["id"] for item in plan["selected_surfaces"]]
        self.assertEqual(
            selected,
            ["mutation.diff[pkg=core]", "tests.pkg[pkg=core]", "workspace.smoke"],
        )

    def test_docs_change_plan_contains_docs_and_smoke(self) -> None:
        self.run_ref(
            "plan",
            "--base",
            COMMITS["core_change"],
            "--head",
            COMMITS["docs_change"],
            "--profile",
            "ci",
        )
        plan = json.loads((self.repo / ".proofrun" / "plan.json").read_text())
        obligations = [item["id"] for item in plan["obligations"]]
        self.assertEqual(obligations, ["workspace:docs", "workspace:smoke"])

    def test_dry_run_receipt_is_written(self) -> None:
        self.run_ref(
            "plan",
            "--base",
            COMMITS["initial"],
            "--head",
            COMMITS["core_change"],
            "--profile",
            "ci",
        )
        self.run_ref("run", "--plan", str(self.repo / ".proofrun" / "plan.json"), "--dry-run")
        receipt = json.loads((self.repo / ".proofrun" / "receipt.json").read_text())
        self.assertEqual(receipt["status"], "dry-run")
        self.assertEqual(len(receipt["steps"]), 3)


if __name__ == "__main__":
    unittest.main()
