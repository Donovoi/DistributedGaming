import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from check_progress import check_fixed_plan, check_readme_changed, progress_block


class ProgressChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = {"goal": "Test goal", "milestones": [{"id": "M1", "title": "Test", "weight": 100, "status": "pending", "acceptance": "Pass", "evidence": []}]}

    def check(self, data):
        (self.root / "progress.json").write_text(json.dumps(data), encoding="utf-8")
        return progress_block(self.root)

    def test_pending_is_zero(self):
        self.assertIn("**0%**", self.check(self.data))

    def test_complete_requires_evidence(self):
        self.data["milestones"][0]["status"] = "complete"
        with self.assertRaises(ValueError):
            self.check(self.data)
        self.data["milestones"][0]["evidence"] = ["proof.txt"]
        with self.assertRaises(ValueError):
            self.check(self.data)
        (self.root / "proof.txt").write_text("passed", encoding="utf-8")
        self.assertIn("**100%**", self.check(self.data))

    def test_bad_weights_and_states(self):
        for key, value in [("weight", 99), ("weight", True), ("status", "mostly")]:
            data = copy.deepcopy(self.data)
            data["milestones"][0][key] = value
            with self.assertRaises(ValueError):
                self.check(data)

    def test_readme_change_required(self):
        with patch("check_progress.subprocess.check_output", return_value=""):
            with self.assertRaises(ValueError):
                check_readme_changed(self.root, "a" * 40)
        with patch("check_progress.subprocess.check_output", return_value="README.md\n"):
            check_readme_changed(self.root, "a" * 40)
        with self.assertRaises(ValueError):
            check_readme_changed(self.root, "--bad-ref")

    def test_plan_cannot_be_reweighted_or_weakened(self):
        before = json.dumps(self.data)
        for key, value in [("weight", 99), ("acceptance", "Just write a plan"), ("id", "new")]:
            data = copy.deepcopy(self.data)
            data["milestones"][0][key] = value
            self.check(data) if key != "weight" else (self.root / "progress.json").write_text(json.dumps(data), encoding="utf-8")
            with patch("check_progress.subprocess.check_output", side_effect=["progress.json\n", before]):
                with self.assertRaises(ValueError):
                    check_fixed_plan(self.root, "a" * 40)

    def test_first_ledger_is_allowed(self):
        with patch("check_progress.subprocess.check_output", return_value=""):
            check_fixed_plan(self.root, "a" * 40)


if __name__ == "__main__":
    unittest.main()
