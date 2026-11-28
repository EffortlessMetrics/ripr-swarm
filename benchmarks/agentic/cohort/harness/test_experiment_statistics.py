"""Independent arithmetic and refusal controls for family-paired statistics."""
import copy
import json
import random
import unittest
from unittest.mock import patch

from experiment_statistics import cluster_summary, _interval


def observation(task="a", family=None, rep=0, arm=2, control=1):
    return dict(task=task, family=family or task, rep=rep, arm=arm, control=control)


class ExperimentStatisticsTests(unittest.TestCase):
    def test_literal_paired_delta_signs_and_ratio_of_sums(self):
        rows = [observation(arm=1, control=1), observation("b", arm=9, control=99),
                observation("c", arm=4, control=2)]
        result = cluster_summary(rows, seed=17, draws=100)
        self.assertAlmostEqual(result["point_delta"], -88 / 3)
        self.assertAlmostEqual(result["ratio"], 14 / 102)
        self.assertEqual([result[key] for key in ("positive_tasks", "zero_tasks", "negative_tasks")], [1, 1, 1])
        self.assertEqual([result[key] for key in ("pairs", "tasks", "declared_families", "repetitions_per_task")], [3, 3, 3, 1])
        json.dumps(result, allow_nan=False)

    def test_task_weighting_is_not_family_weighting(self):
        rows = [observation("a", "one", arm=1), observation("b", "one", arm=3),
                observation("c", "two", arm=11)]
        result = cluster_summary(rows, seed=17, draws=100)
        self.assertEqual(result["point_delta"], 4)  # task deltas 0, 2, 10; not 5.5
        self.assertEqual(result["delta_interval_95"], [1, 10])
        self.assertEqual(result["ratio_interval_95"], [2, 11])

    def test_constant_delta_and_paired_repetitions_survive_resampling(self):
        rows = [observation(task, family, rep, arm=control + 3, control=control)
                for task, family in (("a", "one"), ("b", "one"), ("c", "two"))
                for rep, control in enumerate((1, 100))]
        result = cluster_summary(rows, seed=9, draws=100)
        self.assertEqual(result["delta_interval_95"], [3, 3])
        self.assertEqual(result["point_delta"], 3)
        self.assertAlmostEqual(result["ratio"], 107 / 101)
        self.assertEqual(result["ratio_interval_95"], [107 / 101, 107 / 101])
        self.assertEqual(result["bootstrap_draws"], 100)

    def test_repetition_does_not_invent_declared_families(self):
        rows = [observation(task, "shared", rep) for task in ("a", "b") for rep in range(30)]
        with patch("experiment_statistics.random.Random",
                   side_effect=AssertionError("one family must not resample")) as rng:
            result = cluster_summary(rows, seed=4)
        rng.assert_not_called()
        self.assertEqual(result["interval_status"], "insufficient_declared_families")
        self.assertEqual([result[k] for k in ("pairs", "tasks", "declared_families")], [60, 2, 1])
        self.assertEqual(result["bootstrap_draws"], 0)
        self.assertIsNone(result["delta_interval_95"])
        self.assertIsNone(result["ratio_interval_95"])
        self.assertEqual(result["point_delta"], 1)

    def test_order_invariance_and_no_global_rng_or_input_mutation(self):
        rows = [observation(task, family, rep, arm=i + 1, control=rep + 1)
                for i, (task, family) in enumerate((("a", "one"), ("b", "one"), ("c", "two")))
                for rep in (1, 0)]
        original, rng_state = copy.deepcopy(rows), random.getstate()
        expected = cluster_summary(rows, seed=123, draws=100)
        self.assertEqual(cluster_summary(list(reversed(rows)), seed=123, draws=100), expected)
        self.assertEqual(cluster_summary(rows, seed=123, draws=100), expected)
        self.assertEqual(random.getstate(), rng_state)
        self.assertEqual(rows, original)

    def test_percentile_interpolation_has_literal_bounds(self):
        self.assertEqual(_interval([0, 10]), [0.25, 9.75])
        self.assertEqual(_interval([40, 0, 20, 10, 30]), [1, 39])

    def test_zero_control_totals_and_undefined_resamples_remain_null(self):
        result = cluster_summary([observation(arm=0, control=0),
                                  observation("b", arm=0, control=0)], seed=1, draws=100)
        self.assertIsNone(result["ratio"])
        self.assertIsNone(result["ratio_interval_95"])
        self.assertEqual(result["undefined_ratio_draws"], 100)
        self.assertEqual(result["delta_interval_95"], [0, 0])
        result = cluster_summary([observation(arm=1, control=0),
                                  observation("b", arm=2, control=1)], seed=1, draws=100)
        self.assertEqual(result["ratio"], 3)
        self.assertIsNone(result["ratio_interval_95"])
        self.assertEqual(result["undefined_ratio_draws"], 21)
        self.assertEqual(result["ratio_interval_status"], "undefined_ratio_resamples")

    def test_budget_refusal_retains_points_without_reducing_draws(self):
        rows = [observation(str(i)) for i in range(201)]
        with patch("experiment_statistics.random.Random",
                   side_effect=AssertionError("over-budget request must not resample")) as rng:
            result = cluster_summary(rows, seed=4, draws=10000)
        rng.assert_not_called()
        self.assertEqual(result["interval_status"], "resample_budget_exceeded")
        self.assertEqual(result["point_delta"], 1)
        self.assertEqual(result["ratio"], 2)
        self.assertEqual(result["requested_bootstrap_draws"], 10000)
        self.assertEqual(result["bootstrap_draws"], 0)
        self.assertIsNone(result["delta_interval_95"])
        self.assertIsNone(result["ratio_interval_95"])

    def test_invalid_pairs_and_unbalanced_replications_refuse(self):
        invalid = [[], (), [None], [observation(), observation()],
                   [observation(), observation(family="other", rep=1)],
                   [observation(), observation(rep=1), observation("b")],
                   [observation(), observation("b", rep=1)]]
        for field, values in {"task": (None, " ", 1), "family": (None, "", 1),
                              "rep": (None, -1, True, 1.5),
                              "arm": (None, True, "1", 10**400, float("nan"), float("inf"), -float("inf")),
                              "control": (None, False, "1", 10**400, float("nan"), float("inf"))}.items():
            for value in values:
                invalid.append([dict(observation(), **{field: value})])
        for rows in invalid:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                cluster_summary(rows, seed=0, draws=100)
        for seed, draws in ((True, 100), (1.5, 100), ("1", 100), (0, True),
                            (0, 99), (0, 10001), (0, 100.5)):
            with self.subTest(seed=seed, draws=draws), self.assertRaises(ValueError):
                cluster_summary([observation()], seed=seed, draws=draws)

    def test_signed_values_work_and_overflow_never_emits_infinity(self):
        result = cluster_summary([observation(arm=-3, control=-1)], seed=0, draws=100)
        self.assertEqual(result["point_delta"], -2)
        self.assertEqual(result["ratio"], 3)
        for rows in ([observation(arm=1e308, control=-1e308)],
                     [observation(arm=1e308, control=1e-308)],
                     [observation(arm=1e308), observation(rep=1, arm=1e308)]):
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                cluster_summary(rows, seed=0, draws=100)


if __name__ == "__main__":
    unittest.main()
