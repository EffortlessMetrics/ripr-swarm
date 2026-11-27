"""A retained score cannot override a top-level harness failure."""
import unittest

from scoring import quality_verified
import scoreboard
import test_quality_report as fixture


class QualityErrorTests(unittest.TestCase):
    # Reuse the real-scorer fixture and the real JSON-loader/report path only.
    setUp = fixture.QualityReportTests.setUp
    report = fixture.QualityReportTests.report

    def test_harness_error_does_not_crash_or_receive_aggregate_quality_credit(self):
        self.assertTrue(quality_verified(self.row), "fixture must first establish quality")
        failed = dict(self.row, error="post-score harness failure")
        report = self.report([failed])[0]
        self.assertFalse(quality_verified(failed))
        self.assertEqual(report["runs"], 1)
        self.assertEqual(report["harness_error_runs"], 1)
        self.assertEqual(report["verified_quality_runs"], 0)
        self.assertEqual(report["verified_successful_runs"], 0)
        self.assertIsNone(report["verified_escape_rate"])
        self.assertIsNone(report["tokens_per_verified_success"])
        self.assertEqual(report["metrics"]["tokens"]["observed_total"], 100)

    def test_harness_error_cannot_enter_paired_verified_arithmetic(self):
        self.assertTrue(quality_verified(self.row), "fixture must first establish quality")
        failed = dict(self.row, arm="check-mandatory", error="post-score harness failure")
        paired = scoreboard.paired([failed], [self.row])
        report = next(row for row in self.report([self.row, failed]) if row["config"] != "none")
        self.assertEqual(paired, report["paired"])
        self.assertEqual(report["paired"]["pairs"], 1)
        self.assertEqual(report["paired"]["verified_quality_pairs"], 0)
        self.assertIsNone(report["paired"]["verified_escape_rate_vs_none"])
        self.assertEqual(report["paired"]["comparison_status"], "quality_not_verified")
        self.assertEqual(report["tokens_vs_none_x"], 1)

    def test_absent_or_empty_error_does_not_revoke_real_quality(self):
        for error in (None, "", False):
            with self.subTest(error=error):
                self.assertTrue(quality_verified(dict(self.row, error=error)))
