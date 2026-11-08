"""Adversarial plan/report controls using synthetic scorer-boundary evidence.

No provider or native Cargo execution is claimed. The retained score structure
comes through the existing scorer and is independently varied at report seams.
"""
import contextlib
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import experiment
from experiment_plan import freeze
from experiment_report import build_report, load_receipts, render, schedule_rows
from measurements import digest
from scoring import quality_verified
from test_experiment_plan import design
import test_quality_report


def planned_design(score):
    result = design()
    result.update(repetitions=2, models=result["models"][:1], arms=result["arms"][:2])
    second = dict(result["tasks"][0], id="difficult", family="independent-fixture",
                  stratum="challenging", task_sha256=digest("difficult"))
    result["tasks"].append(second)
    mutation_hashes = [r["mutation_sha256"] for r in score["mutant_results"]]
    for task in result["tasks"]:
        task.update(mutants=len(mutation_hashes), mutant_sha256s=mutation_hashes[:],
                    oracle_sha256=digest(mutation_hashes))
    return result


class ExperimentReportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixture = test_quality_report.QualityReportTests()
        fixture.setUp()
        cls.addClassCleanup(fixture.doCleanups)
        cls.score = copy.deepcopy(fixture.score)

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.plan = freeze(planned_design(self.score))
        self.rows = self.receipts(self.plan)

    def receipts(self, plan):
        rows = []
        for metadata in schedule_rows(plan, "development") + schedule_rows(plan, "holdout"):
            score = copy.deepcopy(self.score)
            score["subject"]["task_sha256"] = metadata["task_sha256"]
            tokens = 100 if metadata["arm"] == plan["design"]["control_arm"] else 50
            row = dict(metadata, score=score, final_tests_sha256=score["subject"]["tests_sha256"],
                       valid=True, src_unchanged=True, killed=1, survived=[], score_error=None,
                       rc=0, is_error=False, result_observed=True, result_subtype="success",
                       tokens=tokens, usage=dict(input_tokens=tokens, output_tokens=0,
                           cache_creation_input_tokens=0, cache_read_input_tokens=0),
                       cost_usd=tokens / 10_000, agent_wall_s=tokens / 50)
            self.assertTrue(quality_verified(row))
            rows.append(row)
        return rows

    def report(self, rows=None, partition="development"):
        path = self.root / "receipts.jsonl"
        path.write_text("".join(json.dumps(r) + "\n" for r in (self.rows if rows is None else rows)), encoding="utf-8")
        return build_report(self.plan, load_receipts(self.plan, [path]), partition=partition)

    def candidate(self, report, stratum=None, model="model-0"):
        return next(g for g in report["groups"] if g["arm"] == "candidate"
                    and g["stratum"] == stratum and g["model"] == model)

    def make_survivor(self, row):
        row.update(killed=0, survived=[0])
        row["score"].update(killed=0, survived=[0])
        result = row["score"]["mutant_results"][0]
        result.update(outcome="survived", test=copy.deepcopy(row["score"]["baseline"]))
        self.assertTrue(quality_verified(row))

    def test_complete_plan_uses_named_control_and_shared_quality(self):
        report = self.report()
        group = self.candidate(report)
        self.assertEqual(report["control_arm"], "baseline")
        self.assertEqual((report["planned_trials_total"], report["planned_trials_selected"]), (12, 8))
        self.assertEqual(report["observed_trials_other_partition"], 4)
        self.assertTrue(report["comparison_complete"])
        self.assertEqual(group["summary"]["verified_success_trials"], 4)
        self.assertEqual(group["summary"]["cost_per_verified_success"]["tokens"], 50)
        self.assertEqual(group["comparison"]["estimates"]["tokens"]["ratio"], 0.5)
        self.assertEqual(group["comparison"]["estimates"]["tokens"]["point_delta"], -50)
        self.assertEqual(group["comparison"]["estimates"]["verified_success"]["point_delta"], 0)
        self.assertEqual(group["comparison"]["promotion_decision"], "not_made")
        self.assertEqual(report["analyzer_correctness_evidence"], "not_evaluated_here")

    def test_dropping_both_sides_cannot_shrink_the_planned_denominator(self):
        rows = [r for r in self.rows if r["task"] != "difficult"]
        report = self.report(rows)
        group = self.candidate(report)
        self.assertEqual((group["summary"]["observed_trials"], group["summary"]["planned_trials"]), (2, 4))
        self.assertEqual(len(group["summary"]["missing_trial_ids"]), 2)
        self.assertEqual(group["summary"]["metrics"]["tokens"]["observed_total"], 100)
        self.assertIsNone(group["summary"]["metrics"]["tokens"]["mean"])
        self.assertIsNone(group["summary"]["cost_per_verified_success"]["tokens"])
        self.assertFalse(report["comparison_complete"])
        self.assertFalse(group["comparison"]["complete_planned_pairing"])
        self.assertIsNone(group["comparison"]["estimates"]["tokens"])
        missing_stratum = self.candidate(report, "challenging")
        self.assertEqual(missing_stratum["summary"]["observed_trials"], 0)
        self.assertEqual(missing_stratum["summary"]["planned_trials"], 2)

    def test_entire_missing_arm_or_control_and_empty_experiment_stay_visible(self):
        for rows in ([], [r for r in self.rows if r["arm"] == "baseline"],
                     [r for r in self.rows if r["arm"] == "candidate"]):
            with self.subTest(arm=rows[0]["arm"] if rows else "empty"):
                report = self.report(rows)
                self.assertFalse(report["comparison_complete"])
                self.assertEqual(len([g for g in report["groups"] if g["stratum"] is None]), 2)
                self.assertEqual(self.candidate(report)["comparison"]["expected_pairs"], 4)
                self.assertIsNone(self.candidate(report)["comparison"]["estimates"]["tokens"])
        self.assertEqual(load_receipts(self.plan, []), [])

    def test_schedule_metadata_cannot_claim_an_observed_attempt(self):
        with self.assertRaisesRegex(ValueError, "not a terminal attempt"):
            self.report(schedule_rows(self.plan, "development"))
        with self.assertRaisesRegex(ValueError, "not a terminal attempt"):
            build_report(self.plan, schedule_rows(self.plan, "development"))
        row = dict(schedule_rows(self.plan, "development")[0], rc=None, is_error=None,
                   result_observed=False, error="setup executable unavailable")
        report = self.report([row])
        self.assertEqual(report["observed_trials_selected"], 1)
        group = next(g for g in report["groups"] if g["stratum"] is None and g["arm"] == row["arm"])
        self.assertEqual(group["summary"]["harness_error_trials"], 1)
        self.assertEqual(group["summary"]["verified_quality_trials"], 0)
        self.assertEqual(group["summary"]["metrics"]["tokens"]["missing_runs"], 4)

    def test_wholly_missing_model_is_not_dropped(self):
        raw = planned_design(self.score)
        raw["models"].append(dict(raw["models"][0], id="model-1", revision="second-model"))
        self.plan = freeze(raw)
        self.rows = [r for r in self.receipts(self.plan) if r["model"] == "model-0"]
        report = self.report()
        self.assertFalse(report["comparison_complete"])
        self.assertEqual(self.candidate(report, model="model-1")["summary"]["planned_trials"], 4)
        self.assertEqual(self.candidate(report, model="model-1")["summary"]["observed_trials"], 0)

    def test_failed_attempt_keeps_spend_and_cannot_earn_verified_success(self):
        failed = next(r for r in self.rows if r["arm"] == "candidate" and r["task"] == "difficult")
        failed.update(error="provider failed", rc=1, is_error=True, score=None, valid=False,
                      tokens=300, cost_usd=0.03)
        failed["usage"]["input_tokens"] = 300
        group = self.candidate(self.report())
        self.assertEqual(group["summary"]["observed_trials"], 4)
        self.assertEqual(group["summary"]["verified_success_trials"], 3)
        self.assertEqual(group["summary"]["metrics"]["tokens"]["observed_total"], 450)
        self.assertEqual(group["summary"]["cost_per_verified_success"]["tokens"], 150)
        self.assertEqual(group["comparison"]["status"], "quality_not_verified")
        self.assertIsNone(group["comparison"]["estimates"]["tokens"])

    def test_missing_components_cannot_upgrade_a_legacy_total(self):
        missing = next(r for r in self.rows if r["arm"] == "candidate" and r["task"] == "difficult")
        del missing["usage"]
        group = self.candidate(self.report())
        self.assertEqual(group["summary"]["metrics"]["tokens"]["missing_runs"], 1)
        self.assertEqual(group["summary"]["metrics"]["tokens"]["observed_total"], 150)
        self.assertIsNone(group["summary"]["metrics"]["tokens"]["mean"])
        self.assertIsNone(group["summary"]["cost_per_verified_success"]["tokens"])
        self.assertEqual(group["comparison"]["status"], "missing_usage")
        self.assertIsNone(group["comparison"]["estimates"]["tokens"])

    def test_absent_optional_cost_stays_unknown_without_hiding_token_evidence(self):
        row = next(r for r in self.rows if r["arm"] == "candidate" and r["task"] == "difficult")
        del row["cost_usd"]
        group = self.candidate(self.report())
        self.assertIsNone(group["comparison"]["estimates"]["cost_usd"])
        self.assertEqual(group["comparison"]["estimates"]["tokens"]["ratio"], 0.5)
        self.assertIsNone(group["summary"]["cost_per_verified_success"]["cost_usd"])

    def test_cheaper_quality_regression_remains_visible_when_pooled_effects_cancel(self):
        for row in self.rows:
            if (row["task"] == "task-0" and row["arm"] == "candidate"
                    or row["task"] == "difficult" and row["arm"] == "baseline"):
                self.make_survivor(row)
        report = self.report()
        comparison = self.candidate(report)["comparison"]
        self.assertEqual(comparison["estimates"]["verified_success"]["point_delta"], 0)
        self.assertEqual(comparison["estimates"]["tokens"]["ratio"], 0.5)
        self.assertEqual(comparison["quality_regressions"], [dict(task="task-0", verified_success_delta=-1,
                                                               escaped_fraction_delta=1)])
        self.assertIn("task-0", render(report))
        self.assertEqual(report["promotion_decision"], "not_made")

    def test_every_frozen_identity_is_required_and_cannot_be_relabelled(self):
        identity = schedule_rows(self.plan, "development")[0]
        for key, value in identity.items():
            for change in ("missing", "different"):
                rows = copy.deepcopy(self.rows)
                row = next(r for r in rows if r["trial_id"] == identity["trial_id"])
                if change == "missing":
                    del row[key]
                else:
                    row[key] = "different" if not isinstance(value, int) else value + 1
                with self.subTest(key=key, change=change), self.assertRaises(ValueError):
                    self.report(rows)

    def test_duplicate_trial_and_boolean_identity_are_rejected(self):
        with self.assertRaises(ValueError):
            self.report(self.rows + [self.rows[0]])
        rows = copy.deepcopy(self.rows)
        rows[0]["rep"] = True
        with self.assertRaises(ValueError):
            build_report(self.plan, rows)
        with self.assertRaises(ValueError):
            build_report(self.plan, self.rows + [self.rows[0]])

    def test_legacy_default_cannot_invent_an_explicit_matching_budget(self):
        raw = planned_design(self.score)
        raw["max_turns"] = 40
        self.plan = freeze(raw)
        self.rows = self.receipts(self.plan)
        for row in self.rows:
            del row["max_turns"]
        with self.assertRaisesRegex(ValueError, "missing explicit max_turns"):
            self.report()
        with self.assertRaisesRegex(ValueError, "missing explicit max_turns"):
            build_report(self.plan, self.rows)

    def test_copied_top_level_oracle_hash_cannot_hide_mutant_substitution(self):
        for replacement in (dict(mutation_sha256="0" * 64), dict(id=True), dict(id=1)):
            rows = copy.deepcopy(self.rows)
            rows[0]["score"]["mutant_results"][0].update(replacement)
            with self.subTest(replacement=replacement), self.assertRaisesRegex(ValueError, "mutation identity"):
                self.report(rows)
        rows = copy.deepcopy(self.rows)
        rows[0]["score"]["mutants"] = 0
        with self.assertRaisesRegex(ValueError, "mutant population"):
            self.report(rows)

    def test_timeout_and_unverified_historical_scores_never_become_a_comparison(self):
        for change in (dict(rc=-9, is_error=True, error="timeout"), dict(score=None)):
            rows = copy.deepcopy(self.rows)
            next(r for r in rows if r["arm"] == "candidate" and r["task"] == "difficult").update(change)
            comparison = self.candidate(self.report(rows))["comparison"]
            self.assertEqual(comparison["status"], "quality_not_verified")
            self.assertIsNone(comparison["estimates"]["tokens"])

    def test_partitions_and_strata_are_separate_and_input_order_is_irrelevant(self):
        report = self.report()
        self.assertEqual(report, self.report(list(reversed(self.rows))))
        holdout = self.report(partition="holdout")
        self.assertEqual(holdout["planned_trials_selected"], 4)
        self.assertEqual(self.candidate(holdout)["task_ids"], ["task-1"])
        self.assertIsNone(self.candidate(holdout)["comparison"]["estimates"]["tokens"]["delta_interval_95"])
        self.assertEqual(self.candidate(report, "routine")["summary"]["planned_trials"], 2)
        self.assertEqual(self.candidate(report, "challenging")["summary"]["planned_trials"], 2)
        self.assertEqual(report["missing_strata"], ["negative_control", "no_op"])
        with self.assertRaises(ValueError):
            build_report(self.plan, self.rows, partition="combined")

    def test_duplicate_json_key_or_nonfinite_receipt_is_invalid(self):
        path = self.root / "invalid.jsonl"
        for data in ('{"task":"x","task":"y"}', '{"x":1e999}', '{"x":NaN}'):
            path.write_text(data + "\n", encoding="utf-8")
            with self.subTest(data=data), self.assertRaises(ValueError):
                load_receipts(self.plan, [path])

    def test_direct_and_file_admission_share_usage_consistency(self):
        rows = copy.deepcopy(self.rows)
        rows[0]["tokens"] = -1
        with self.assertRaisesRegex(ValueError, "tokens disagrees"):
            build_report(self.plan, rows)
        with self.assertRaisesRegex(ValueError, "tokens disagrees"):
            self.report(rows)

    def test_unrepresentable_measurements_fail_cli_without_partial_report(self):
        plan_path = self.root / "plan.json"
        plan_path.write_text(json.dumps(self.plan), encoding="utf-8")
        for variant in ("individual", "total"):
            rows = copy.deepcopy(self.rows)
            for row in rows:
                if variant == "individual":
                    row["tokens"] = row["usage"]["input_tokens"] = 10 ** 400
                else:
                    row["cost_usd"] = 1e308
            source = self.root / "overflow.jsonl"
            source.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
            output = str(self.root / variant)
            proc = subprocess.run([sys.executable, experiment.__file__, "report", str(plan_path), output,
                                   str(source)], capture_output=True, text=True, check=False)
            self.assertEqual(proc.returncode, 2, proc.stderr)
            self.assertNotIn("Traceback", proc.stderr)
            self.assertIn("finite arithmetic", proc.stderr)
            self.assertFalse(Path(output + ".json").exists())
            self.assertFalse(Path(output + ".md").exists())

    def test_cli_freeze_schedule_report_exit_codes_and_evidence_retention(self):
        design_path, plan_path = self.root / "design.json", self.root / "plan.json"
        design_path.write_text(json.dumps(planned_design(self.score)), encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(experiment.main(["freeze", str(design_path), str(plan_path)]), 0)
            self.assertEqual(experiment.main(["freeze", str(design_path), str(plan_path)]), 2)
        schedule = subprocess.run([sys.executable, experiment.__file__, "schedule", str(plan_path)],
                                  capture_output=True, text=True, check=False)
        self.assertEqual(schedule.returncode, 0, schedule.stderr)
        scheduled = [json.loads(line) for line in schedule.stdout.splitlines()]
        self.assertEqual(len(scheduled), 8)
        self.assertEqual([r["schedule_position"] for r in scheduled], sorted(r["schedule_position"] for r in scheduled))
        self.assertTrue(all("rc" not in r and "score" not in r for r in scheduled))
        self.report()  # writes real input for the CLI path
        output = str(self.root / "complete")
        proc = subprocess.run([sys.executable, experiment.__file__, "report", str(plan_path),
                               output, str(self.root / "receipts.jsonl")], capture_output=True, text=True, check=False)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        saved = Path(output + ".json").read_bytes()
        markdown = Path(output + ".md").read_text(encoding="utf-8")
        self.assertIn("4/4", markdown)
        self.assertIn("Promotion decision: **not made**", markdown)
        self.assertIn("Token delta family interval", markdown)
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(experiment.main(["report", str(plan_path), output]), 2)
            self.assertEqual(experiment.main(["report", str(plan_path), str(self.root / "missing")]), 1)
        self.assertEqual(Path(output + ".json").read_bytes(), saved)
        missing = json.loads((self.root / "missing.json").read_text(encoding="utf-8"))
        self.assertFalse(missing["comparison_complete"])
        self.assertEqual(missing["observed_trials_selected"], 0)
        holdout = subprocess.run([sys.executable, experiment.__file__, "report", str(plan_path),
                                 str(self.root / "holdout"), str(self.root / "receipts.jsonl"),
                                 "--partition", "holdout"], capture_output=True, text=True, check=False)
        self.assertEqual(holdout.returncode, 0, holdout.stderr)
        self.assertEqual(json.loads((self.root / "holdout.json").read_text())["partition"], "holdout")


if __name__ == "__main__":
    unittest.main()
