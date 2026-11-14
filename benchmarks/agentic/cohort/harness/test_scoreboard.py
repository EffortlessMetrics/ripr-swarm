"""Offline tests of the real receipt loader and scoreboard CLI path."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest

import scoreboard


def receipt(**changes):
    row = dict(task="boundary", arm="none", model="test-model", rep=1,
               max_turns=40, ripr_version=None, rc=0, is_error=False,
               valid=True, src_unchanged=True, mutants=2, killed=2,
               survived=[], score_error=None, tokens=100, cost_usd=0.01,
               agent_wall_s=2.0, turns=3, ripr_calls=0)
    row.update(changes)
    return row


class ScoreboardTests(unittest.TestCase):
    def report(self, *files):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = []
            for i, rows in enumerate(files):
                path = root / f"receipts-{i}.jsonl"
                path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
                paths.append(str(path))
            out = str(root / "report")
            with contextlib.redirect_stdout(io.StringIO()):
                scoreboard.main(out, paths)
            self.markdown = (root / "report.md").read_text(encoding="utf-8")
            return json.loads((root / "report.json").read_text(encoding="utf-8"))

    def test_failed_receipts_remain_in_attempt_denominator(self):
        rows = [receipt(), receipt(rep=2, error="worker failed", valid=False)]
        row = self.report(rows)[0]
        self.assertEqual(row["runs"], 2)
        self.assertEqual(row["fix_rate"], 0.5)

    def test_missing_tokens_are_unknown_not_free(self):
        row = self.report([receipt(), receipt(rep=2, tokens=None)])[0]
        self.assertIsNone(row["tokens"])

    def test_different_tasks_cannot_claim_token_savings(self):
        rows = [receipt(), receipt(task="different", arm="check-mandatory", tokens=10)]
        row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
        self.assertIsNone(row.get("tokens_vs_none_x"))

    def test_different_replicates_cannot_claim_token_savings(self):
        rows = [receipt(), receipt(rep=2, arm="check-mandatory", tokens=10)]
        row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
        self.assertIsNone(row.get("tokens_vs_none_x"))

    def test_timeout_with_good_tests_is_not_completed_success(self):
        row = self.report([receipt(rc=-9, is_error=None, tokens=0)])[0]
        self.assertEqual(row["fix_rate"], 0.0)
        self.assertIsNone(row["tokens"])

    def test_empty_oracle_is_not_vacuous_success(self):
        row = self.report([receipt(mutants=0, killed=0)])[0]
        self.assertEqual(row["fix_rate"], 0.0)
        self.assertIsNone(row["escape_rate"])


    def test_equal_pairs_report_exact_cost_and_quality_deltas(self):
        rows = [receipt(), receipt(rep=2, tokens=300),
                receipt(arm="check-mandatory", tokens=50),
                receipt(arm="check-mandatory", rep=2, tokens=150, killed=1, survived=[1])]
        row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
        self.assertEqual(row["tokens_vs_none_x"], 0.5)
        self.assertEqual(row["fix_rate_vs_none"], -0.5)
        self.assertEqual(row["paired"]["escape_rate_vs_none"], 0.25)
        self.assertEqual(row["paired"]["observed_token_delta_mean"], -100)
        self.assertEqual(row["tokens_per_success"], 200)
        self.assertIn("Success delta", self.markdown)
        self.assertIn("-50.0%", self.markdown)

    def test_failure_spend_is_in_cost_per_success(self):
        row = self.report([receipt(), receipt(rep=2, error="failed", tokens=300, cost_usd=0.03)])[0]
        self.assertEqual(row["tokens_per_success"], 400)
        self.assertAlmostEqual(row["cost_usd_per_success"], 0.04)
        self.assertEqual(row["harness_error_runs"], 1)
        self.assertEqual(row["escape_measured_runs"], 1)

    def test_no_success_has_no_cost_per_success(self):
        row = self.report([receipt(killed=1, survived=[0])])[0]
        self.assertIsNone(row["tokens_per_success"])

    def test_unknown_cost_suppresses_total_mean_and_per_success(self):
        row = self.report([receipt(), receipt(rep=2, cost_usd=None, tokens=None)])[0]
        self.assertIsNone(row["cost_usd"])
        self.assertIsNone(row["tokens_per_success"])
        self.assertEqual(row["metrics"]["tokens"]["measured_runs"], 1)
        self.assertEqual(row["metrics"]["tokens"]["observed_total"], 100)
        self.assertIn("not measured", self.markdown)

    def test_invalid_numeric_measurements_remain_unknown(self):
        for value in (None, True, -1, "100", float("nan"), float("inf")):
            with self.subTest(value=value):
                row = self.report([receipt(tokens=value, cost_usd=value)])[0]
                self.assertIsNone(row["tokens"])
                self.assertIsNone(row["cost_usd"])

    def test_measured_zero_is_distinct_from_unknown(self):
        row = self.report([receipt(tokens=0, cost_usd=0)])[0]
        self.assertEqual(row["tokens"], 0)
        self.assertEqual(row["metrics"]["tokens"]["measured_runs"], 1)
        self.assertEqual(row["cost_usd"], 0)

    def test_zero_control_does_not_become_one(self):
        row = next(r for r in self.report([receipt(tokens=0), receipt(arm="check-mandatory")])
                   if r["config"] == "check-mandatory")
        self.assertIsNone(row["tokens_vs_none_x"])

    def test_legacy_files_cannot_borrow_each_others_controls(self):
        rows = self.report([receipt()], [receipt(arm="check-mandatory", tokens=1)])
        row = next(r for r in rows if r["config"] == "check-mandatory")
        self.assertIsNone(row["tokens_vs_none_x"])
        self.assertEqual(row["paired"]["pairs"], 0)

    def test_explicit_cohort_can_pair_across_files(self):
        rows = self.report([receipt(cohort_id="fixed")],
                           [receipt(cohort_id="fixed", arm="check-mandatory", tokens=50)])
        row = next(r for r in rows if r["config"] == "check-mandatory")
        self.assertEqual(row["tokens_vs_none_x"], 0.5)
        self.assertFalse(row["paired"]["task_protocol_identity_matched"])

    def test_task_and_protocol_drift_cannot_be_paired(self):
        for key in ("task_sha256", "protocol_sha256"):
            with self.subTest(key=key):
                rows = [receipt(**{key: "a" * 64}),
                        receipt(arm="check-mandatory", **{key: "b" * 64})]
                row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
                self.assertEqual(row["paired"]["subject_mismatches"], [["boundary", 1]])
                self.assertIsNone(row["tokens_vs_none_x"])

    def test_unmatched_attempts_remain_visible(self):
        rows = [receipt(), receipt(rep=2), receipt(arm="check-mandatory", tokens=50)]
        row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
        self.assertEqual(row["paired"]["pairs"], 1)
        self.assertEqual(row["paired"]["unmatched_control_runs"], [["boundary", 2]])
        self.assertIsNone(row["tokens_vs_none_x"])
        self.assertEqual(row["paired"]["observed_token_delta_mean"], -50)

    def test_incomplete_paired_usage_cannot_report_a_ratio(self):
        rows = [receipt(), receipt(arm="check-mandatory", tokens=None)]
        row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
        self.assertEqual(row["paired"]["pairs"], 1)
        self.assertEqual(row["paired"]["tokens_measured_pairs"], 0)
        self.assertIsNone(row["tokens_vs_none_x"])

    def test_duplicate_attempt_is_actionable_not_extra_evidence(self):
        with self.assertRaisesRegex(ValueError, r"receipts-0.jsonl:2: duplicate attempt"):
            self.report([receipt(), receipt()])

    def test_versions_are_not_silently_pooled(self):
        with self.assertRaisesRegex(ValueError, "mixed ripr versions"):
            self.report([receipt(ripr_version="ripr 1.0 (aaaa)"),
                         receipt(rep=2, ripr_version="ripr 1.0 (bbbb)")])

    def test_unknown_failure_budget_is_not_defaulted_to_40(self):
        row = receipt(error="old failure", tokens=None)
        del row["max_turns"]
        report = self.report([row])[0]
        self.assertIsNone(report["max_turns"])
        self.assertEqual(report["runs"], 1)

    def test_legacy_arm_names_normalize(self):
        rows = [receipt(arm="A0-none"), receipt(arm="A1-diagnostics", tokens=50)]
        row = next(r for r in self.report(rows) if r["config"] == "check-mandatory")
        self.assertEqual(row["tokens_vs_none_x"], 0.5)

    def test_partial_or_illegal_oracle_evidence_cannot_count_as_success(self):
        for changes in (dict(src_unchanged=False), dict(survived=[0]),
                        dict(mutants=2, killed=1, survived=[]), dict(killed=3),
                        dict(score_error="compile failure"), dict(valid=False)):
            with self.subTest(changes=changes):
                row = self.report([receipt(**changes)])[0]
                self.assertEqual(row["successful_runs"], 0)
                self.assertIsNone(row["escape_rate"])

    def test_input_order_does_not_change_explicit_cohort_metrics(self):
        rows = [receipt(cohort_id="fixed"), receipt(cohort_id="fixed", rep=2, tokens=200),
                receipt(cohort_id="fixed", arm="check-mandatory", tokens=50),
                receipt(cohort_id="fixed", arm="check-mandatory", rep=2, tokens=100)]
        self.assertEqual(self.report(rows), self.report(list(reversed(rows))))

    def test_empty_experiment_is_not_a_result(self):
        with self.assertRaisesRegex(ValueError, "no receipts"):
            self.report([])

    def test_malformed_receipt_has_file_and_line_diagnostic(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.jsonl"
            path.write_text('{"task":', encoding="utf-8")
            proc = subprocess.run([sys.executable, scoreboard.__file__, str(path.parent / "report"), str(path)],
                                  capture_output=True, text=True, check=False)
            self.assertEqual(proc.returncode, 2)
            self.assertIn("bad.jsonl:1:", proc.stderr)
            self.assertFalse((path.parent / "report.json").exists())


if __name__ == "__main__":
    unittest.main()
