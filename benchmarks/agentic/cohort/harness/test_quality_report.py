"""The real scorer's retained evidence must reach the real cost-report decision."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import scoreboard
from scoring import quality_verified
from test_scoring import TASK, PASS, FAIL, execution, load_lib


def measured_row(score, **changes):
    row = dict(task="oracle_fixture", arm="none", model="synthetic-model", rep=1,
               cohort_id="synthetic-quality-test", max_turns=40, rc=0, is_error=False,
               tokens=100, cost_usd=0.01, agent_wall_s=1, turns=1, ripr_calls=0,
               task_sha256=score["subject"]["task_sha256"], protocol_sha256="b" * 64,
               final_tests_sha256=score["subject"]["tests_sha256"], score=score,
               valid=score["valid"], src_unchanged=score["src_unchanged"], mutants=score["mutants"],
               killed=score["killed"], survived=score["survived"], score_error=score["error"])
    row.update(changes)
    return row


class QualityReportTests(unittest.TestCase):
    def setUp(self):
        lib = load_lib()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        lib.build_repo(TASK, str(root / "agent"))
        with mock.patch.object(lib, "cargo_test", side_effect=[execution(), execution(101, FAIL), execution()]):
            self.score = lib.score(TASK, str(root / "agent"), str(root / "target"))
        self.row = measured_row(self.score)

    def report(self, rows):
        path = Path(self.tmp.name) / "records.jsonl"
        path.write_text("".join(json.dumps(r) + "\n" for r in rows))
        return scoreboard.build_report(scoreboard.load([path]))

    def test_real_scorer_output_reaches_verified_cost_metric(self):
        self.assertTrue(quality_verified(self.row))
        result = self.report([self.row])[0]
        self.assertEqual(result["verified_quality_runs"], 1)
        self.assertEqual(result["verified_successful_runs"], 1)
        self.assertEqual(result["tokens_per_verified_success"], 100)
        self.assertIn("Tokens/verified success", scoreboard.render([result]))

    def test_all_attempt_spend_includes_unverified_failure(self):
        bad = dict(self.row, rep=2, score=None, valid=False, tokens=300, cost_usd=0.03)
        result = self.report([self.row, bad])[0]
        self.assertEqual(result["runs"], 2)
        self.assertEqual(result["verified_successful_runs"], 1)
        self.assertEqual(result["tokens_per_verified_success"], 400)
        self.assertAlmostEqual(result["cost_usd_per_verified_success"], 0.04)

    def test_historical_success_is_not_silently_requalified(self):
        legacy = dict(self.row)
        del legacy["score"]
        result = self.report([legacy])[0]
        self.assertEqual(result["successful_runs"], 1)
        self.assertEqual(result["verified_successful_runs"], 0)
        self.assertEqual(result["legacy_quality_runs"], 1)
        self.assertIsNone(result["tokens_per_verified_success"])

    def test_unknown_spend_does_not_become_cost_per_verified_success(self):
        result = self.report([self.row, dict(self.row, rep=2, tokens=None)])[0]
        self.assertEqual(result["verified_successful_runs"], 2)
        self.assertIsNone(result["tokens_per_verified_success"])

    def test_incomplete_or_wrong_protocol_never_gets_verified_credit(self):
        for changes in (dict(quality_status="incomplete"), dict(score_protocol="future-v9"),
                        dict(baseline=None), dict(unresolved=[0]), dict(scope_violations=["src/lib.rs"]),
                        dict(mutants=0), dict(killed=True)):
            with self.subTest(changes=changes):
                row = copy.deepcopy(self.row)
                row["score"].update(changes)
                self.assertFalse(quality_verified(row))

    def test_mutation_labels_cannot_replace_execution_evidence(self):
        for status in ("timeout", "compile_error", "process_error", "zero_tests", "passed"):
            with self.subTest(status=status):
                row = copy.deepcopy(self.row)
                row["score"]["mutant_results"][0]["test"]["status"] = status
                self.assertFalse(quality_verified(row))

    def test_counter_and_identity_mismatches_withhold_verified_credit(self):
        variants = [dict(self.row, killed=0), dict(self.row, task_sha256="c" * 64),
                    dict(self.row, final_tests_sha256="c" * 64),
                    dict(self.row, artifact_error="archive missing"), dict(self.row, mutants=True)]
        for row in variants:
            with self.subTest(row=row):
                self.assertFalse(quality_verified(row))

    def test_baseline_and_restore_require_actual_test_execution(self):
        for field in ("baseline", "restored"):
            row = copy.deepcopy(self.row)
            item = row["score"]["baseline"] if field == "baseline" else row["score"]["mutant_results"][0]["restored"]
            item["executed_tests"] = 0
            self.assertFalse(quality_verified(row))

    def test_no_duplicate_or_boolean_mutant_id_is_accepted(self):
        for value in (False, 2, "0"):
            row = copy.deepcopy(self.row)
            row["score"]["mutant_results"][0]["id"] = value
            self.assertFalse(quality_verified(row))

    def test_paired_tokens_do_not_claim_verified_quality_without_it(self):
        control = dict(self.row)
        arm = dict(self.row, arm="check-mandatory", tokens=50)
        del arm["score"]
        result = next(r for r in self.report([control, arm]) if r["config"] != "none")
        self.assertEqual(result["tokens_vs_none_x"], 0.5)
        self.assertEqual(result["paired"]["comparison_status"], "quality_not_verified")
        self.assertEqual(result["paired"]["verified_quality_pairs"], 0)
        self.assertIsNone(result["paired"]["verified_escape_rate_vs_none"])

    def test_complete_quality_pair_remains_descriptive_not_an_efficiency_win(self):
        result = next(r for r in self.report([self.row, dict(self.row, arm="check-mandatory", tokens=50)])
                      if r["config"] != "none")
        self.assertEqual(result["paired"]["comparison_status"], "descriptive_matched")
        self.assertEqual(result["paired"]["verified_quality_pairs"], 1)
        self.assertEqual(result["paired"]["verified_escape_rate_vs_none"], 0)

    def test_scorer_cost_is_visible_without_changing_agent_token_accounting(self):
        result = self.report([dict(self.row, scoring_wall_s=3, evaluation_wall_s=4)])[0]
        self.assertEqual(result["metrics"]["scoring_wall_s"]["mean"], 3)
        self.assertEqual(result["metrics"]["evaluation_wall_s"]["mean"], 4)
        self.assertEqual(result["tokens"], 100)

    def test_future_or_malformed_receipt_versions_are_not_reinterpreted(self):
        for version in (4, True, None, "3"):
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, "receipt_schema_version"):
                self.report([dict(self.row, receipt_schema_version=version)])
