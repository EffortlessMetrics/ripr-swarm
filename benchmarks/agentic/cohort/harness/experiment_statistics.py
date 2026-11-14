"""Descriptive paired estimates; sensitivity to the curated family composition.

Whole families are resampled, retaining their tasks, repetitions and pairing.
These intervals do not estimate unseen-population or within-task run uncertainty.
At most 2,000,000 requested draws * tasks are admitted; no draws are downsampled.
"""
import math
import random
from collections import defaultdict


MAX_RESAMPLE_CONTRIBUTIONS = 2_000_000


def _finite(value):
    try:
        valid = type(value) in (int, float) and math.isfinite(value)
    except OverflowError:
        valid = False
    if not valid:
        raise ValueError("statistics require finite numbers, excluding booleans")
    return value


def _sum(values):
    try:
        return _finite(math.fsum(values))
    except OverflowError as error:
        raise ValueError("statistics exceed finite arithmetic range") from error


def _ratio(numerator, denominator):
    return _finite(numerator / denominator) if denominator != 0 else None


def _interval(values):
    """95% percentile interval: linear interpolation at (n - 1) * p."""
    ordered = sorted(values)
    bounds = []
    for probability in (0.025, 0.975):
        position = (len(ordered) - 1) * probability
        lower, upper = math.floor(position), math.ceil(position)
        weight = position - lower
        bounds.append(_sum(((1 - weight) * ordered[lower], weight * ordered[upper])))
    return bounds


def cluster_summary(observations, *, seed, draws=2000):
    """Summarize complete pairs; callers own identity admission and quality gates.

    Inputs must contain the same nonnegative replicate IDs for every task. Each
    task belongs to one family. Finite signed int/float measurements are accepted;
    an unrepresentable arithmetic result raises ValueError instead of emitting NaN.
    """
    if not isinstance(observations, list) or not observations:
        raise ValueError("observations must be a nonempty list")
    if type(seed) is not int or type(draws) is not int or not 100 <= draws <= 10000:
        raise ValueError("seed must be an integer and draws an integer from 100 to 10000")
    tasks = {}
    for row in observations:
        if not isinstance(row, dict):
            raise ValueError("each observation must be an object")
        task, family, rep = (row.get(key) for key in ("task", "family", "rep"))
        if any(not isinstance(value, str) or not value.strip() for value in (task, family)):
            raise ValueError("task and family must be nonempty strings")
        if type(rep) is not int or rep < 0:
            raise ValueError("rep must be a nonnegative integer")
        values = (_finite(row.get("arm")), _finite(row.get("control")))
        previous_family, replicates = tasks.setdefault(task, (family, {}))
        if previous_family != family:
            raise ValueError("each task must belong to exactly one family")
        if rep in replicates:
            raise ValueError("duplicate task/rep pair")
        replicates[rep] = values
    replicate_sets = {tuple(sorted(replicates)) for _, replicates in tasks.values()}
    if len(replicate_sets) != 1:
        raise ValueError("tasks must have identical replicate sets")
    repetitions = len(next(iter(replicate_sets)))
    by_family = defaultdict(list)
    task_deltas, all_arms, all_controls = [], [], []
    for task in sorted(tasks):
        family, replicates = tasks[task]
        pairs = [replicates[rep] for rep in sorted(replicates)]
        delta = _sum(_finite(arm - control) for arm, control in pairs) / repetitions
        arms, controls = zip(*pairs)
        task_deltas.append(delta)
        all_arms.extend(arms)
        all_controls.extend(controls)
        by_family[family].append((delta, _sum(arms), _sum(controls)))
    # Aggregate once: a draw never expands task repetitions or large families.
    families = [(len(rows), *(_sum(row[i] for row in rows) for i in range(3)))
                for _, rows in sorted(by_family.items())]
    status = ("insufficient_declared_families" if len(families) < 2 else
              "resample_budget_exceeded" if draws * len(tasks) > MAX_RESAMPLE_CONTRIBUTIONS else
              "descriptive_family_bootstrap")
    result = dict(pairs=len(observations), tasks=len(tasks), declared_families=len(families),
                  repetitions_per_task=repetitions, point_delta=_sum(task_deltas) / len(tasks),
                  ratio=_ratio(_sum(all_arms), _sum(all_controls)),
                  positive_tasks=sum(delta > 0 for delta in task_deltas),
                  zero_tasks=sum(delta == 0 for delta in task_deltas),
                  negative_tasks=sum(delta < 0 for delta in task_deltas),
                  estimand="equal_task_weighted_mean_of_within_task_paired_deltas",
                  ratio_estimand="pooled_arm_sum_divided_by_pooled_control_sum",
                  method="whole-family bootstrap; percentile 95%; linear interpolation at (n - 1) * p",
                  interpretation="Descriptive sensitivity to curated family composition; not unseen-population or within-task stochastic uncertainty.",
                  interval_status=status, ratio_interval_status=status,
                  delta_interval_95=None, ratio_interval_95=None, undefined_ratio_draws=0,
                  bootstrap_seed=seed, bootstrap_draws=0, requested_bootstrap_draws=draws,
                  resample_contribution_cap=MAX_RESAMPLE_CONTRIBUTIONS)
    if status != "descriptive_family_bootstrap":
        return result
    rng = random.Random(seed)
    delta_draws, ratio_draws = [], []
    for _ in range(draws):
        sample = [families[rng.randrange(len(families))] for _ in families]
        delta_draws.append(_sum(row[1] for row in sample) / sum(row[0] for row in sample))
        ratio = _ratio(_sum(row[2] for row in sample), _sum(row[3] for row in sample))
        if ratio is not None:
            ratio_draws.append(ratio)
    undefined = draws - len(ratio_draws)
    result.update(bootstrap_draws=draws, delta_interval_95=_interval(delta_draws),
                  undefined_ratio_draws=undefined,
                  ratio_interval_status=("undefined_ratio_resamples" if undefined else
                                         "zero_control_total" if result["ratio"] is None else status))
    if not undefined and result["ratio"] is not None:
        result["ratio_interval_95"] = _interval(ratio_draws)
    return result
