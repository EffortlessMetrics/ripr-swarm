"""Exercise the real retained JSONL inputs without requalifying historical runs."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import scoreboard


RETAINED = {
    "cohort-2026-11-04.jsonl": 99,
    "pilot-big-haiku.jsonl": 6,
    "pilot-small-A0-haiku.jsonl": 8,
    "real-pilot-main-0.11.0-2026-11-04.jsonl": 36,
    "strsim-matrix-main-0.11.0-2026-11-04.jsonl": 100,
}
ROOT = Path(__file__).resolve().parent


class RetainedReceiptTests(unittest.TestCase):
    def retained_paths(self):
        paths = sorted((ROOT.parent / "receipts").glob("*.jsonl"))
        self.assertCountEqual([path.name for path in paths], RETAINED,
                              "the real retained receipt population must be present")
        return paths

    def test_every_retained_attempt_is_reported_without_verified_quality_credit(self):
        paths = self.retained_paths()
        for path in paths:
            with self.subTest(receipt=path.name):
                rows = scoreboard.load([path])
                self.assertEqual(len(rows), RETAINED[path.name])
                self.assertTrue(all("score" not in row for row in rows))
        rows = scoreboard.load(paths)
        report = scoreboard.build_report(rows)
        self.assertEqual(len(rows), 249)
        self.assertEqual(len({row["_cohort"] for row in rows}), 5)
        self.assertEqual(sum(group["runs"] for group in report), 249)
        self.assertEqual(sum(group["legacy_quality_runs"] for group in report), 249)
        for group in report:
            self.assertEqual(group["verified_quality_runs"], 0)
            self.assertEqual(group["verified_successful_runs"], 0)
            self.assertIsNone(group["tokens_per_verified_success"])
            self.assertIsNone(group["cost_usd_per_verified_success"])

    def test_default_aggregation_reads_the_retained_cohort_from_another_directory(self):
        self.retained_paths()
        rows = scoreboard.load([ROOT.parent / "receipts" / "cohort-2026-11-04.jsonl"])
        report = scoreboard.build_report(rows)
        self.assertEqual(sum(group["runs"] for group in report), 99)
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, str(ROOT / "agg.py")], cwd=directory,
                                    capture_output=True, text=True, timeout=15, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, scoreboard.render(report))
        self.assertIn("Tokens/verified success", result.stdout)
