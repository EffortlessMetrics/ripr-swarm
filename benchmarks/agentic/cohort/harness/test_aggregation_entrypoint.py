"""Both cohort reporting commands must use the same failure-aware accounting."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import scoreboard
from test_scoreboard import receipt


class AggregationEntrypointTests(unittest.TestCase):
    def run_aggregate(self, rows, *, default=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            harness = root / "cohort" / "harness"
            harness.mkdir(parents=True)
            source = Path(__file__).resolve().parent
            for name in ("agg.py", "scoreboard.py", "measurements.py", "scoring.py"):
                shutil.copyfile(source / name, harness / name)
            records = root / "cohort" / "receipts" / "cohort-2026-11-04.jsonl"
            records.parent.mkdir()
            records.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
            expected = scoreboard.render(scoreboard.build_report(scoreboard.load([records])))
            args = [] if default else [str(records)]
            result = subprocess.run([sys.executable, str(harness / "agg.py"), *args],
                                    cwd=root, capture_output=True, text=True, timeout=10, check=False)
            return result, expected

    def test_failed_attempt_spend_and_denominator_reach_the_legacy_command(self):
        rows = [receipt(), receipt(rep=2, valid=False, error="failed", tokens=300, cost_usd=0.03)]
        result, expected = self.run_aggregate(rows)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, expected)
        self.assertIn("200 [2/2]", result.stdout)
        self.assertIn("Tokens/verified success", result.stdout)

    def test_unknown_usage_remains_unknown_instead_of_crashing(self):
        result, expected = self.run_aggregate([receipt(), receipt(rep=2, tokens=None)])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, expected)
        self.assertIn("not measured [1/2]", result.stdout)

    def test_default_receipt_is_relative_to_the_script_not_working_directory(self):
        result, expected = self.run_aggregate([receipt()], default=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, expected)

    def test_duplicate_attempts_fail_with_an_actionable_diagnostic(self):
        with tempfile.TemporaryDirectory() as directory:
            records = Path(directory) / "duplicates.jsonl"
            records.write_text((json.dumps(receipt()) + "\n") * 2, encoding="utf-8")
            script = Path(__file__).resolve().parent / "agg.py"
            result = subprocess.run([sys.executable, str(script), str(records)], capture_output=True,
                                    text=True, timeout=10, check=False)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("duplicates.jsonl:2: duplicate attempt", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertEqual(result.stdout, "")
