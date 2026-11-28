"""Reconcile frozen trials with the existing cohort quality/accounting owners.

This is a consistency instrument, not provider attestation or a promotion gate.
An absent planned trial cannot disappear from its denominator. Only complete,
identity-matched and quality-verified pairs receive comparative estimates.
"""
import collections
import math

import scoreboard
from experiment_plan import parse_json, validate_plan
from experiment_statistics import cluster_summary
from measurements import TOKEN_FIELDS, completed, nonnegative, token_count
from scoring import quality_verified


MEASURES = ("tokens", "cost_usd", "agent_wall_s", "scoring_wall_s",
            "evaluation_wall_s", "turns", "tool_calls", "ripr_calls", *TOKEN_FIELDS)


def _finite(value, label):
    if type(value) in (int, float):
        try:
            finite = math.isfinite(value)
        except OverflowError:
            finite = False
        if not finite:
            raise ValueError(f"{label}: measurement exceeds finite arithmetic range")


def _catalog(plan):
    return {key: {item["id"]: item for item in plan["design"][key]}
            for key in ("tasks", "models", "arms")}


def trial_metadata(plan, trial, *, catalog=None):
    """Planned identities for a collector; this alone is never an outcome receipt."""
    design = plan["design"]
    catalog = _catalog(plan) if catalog is None else catalog
    task = catalog["tasks"][trial["task"]]
    model = catalog["models"][trial["model"]]
    arm = catalog["arms"][trial["arm"]]
    return dict(receipt_schema_version=3, cohort_id=design["experiment_id"],
                experiment_plan_sha256=plan["plan_sha256"], trial_id=trial["trial_id"],
                schedule_position=trial["position"], task=task["id"], model=model["id"],
                arm=arm["id"], rep=trial["rep"], max_turns=design["max_turns"],
                trial_timeout_s=design["trial_timeout_s"],
                task_sha256=task["task_sha256"], oracle_sha256=task["oracle_sha256"],
                mutants=task["mutants"], protocol_sha256=design["protocol_sha256"],
                evaluator_sha256=design["evaluator_sha256"],
                runner_id=design["runner"]["id"],
                runner_environment_sha256=design["runner"]["environment_sha256"],
                provider=model["provider"], model_revision=model["revision"],
                model_settings_sha256=model["settings_sha256"],
                arm_configuration_sha256=arm["configuration_sha256"],
                ripr_revision=arm["ripr_revision"], ripr_binary_sha256=arm["ripr_binary_sha256"])


def _oracle_identity(row, task):
    """Bind available scorer evidence to the planned mutants, including partials."""
    score = row.get("score")
    if not isinstance(score, dict):
        return
    if "mutants" in score and (type(score["mutants"]) is not int
                               or score["mutants"] != task["mutants"]):
        raise ValueError(f"{row['trial_id']}: scorer mutant population differs from plan")
    subject = score.get("subject")
    if isinstance(subject, dict) and subject.get("task_sha256") != task["task_sha256"]:
        raise ValueError(f"{row['trial_id']}: scorer task identity differs from plan")
    results = score.get("mutant_results")
    if not isinstance(results, list):
        return  # The canonical quality validator withholds incomplete evidence.
    seen = set()
    for result in results:
        index = result.get("id") if isinstance(result, dict) else None
        if (type(index) is not int or not 0 <= index < task["mutants"] or index in seen
                or result.get("mutation_sha256") != task["mutant_sha256s"][index]):
            raise ValueError(f"{row['trial_id']}: scorer mutation identity differs from plan")
        seen.add(index)


def _admit_rows(plan, rows):
    """One plan-identity authority for the file and in-memory report paths."""
    trials = {t["trial_id"]: t for t in plan["schedule"]}
    catalog = _catalog(plan)
    seen = set()
    admitted = []
    for row in rows:
        row, _ = scoreboard.validate_receipt(row, plan["design"]["experiment_id"], legacy_defaults=False)
        trial_id = row.get("trial_id")
        if not isinstance(trial_id, str) or trial_id not in trials:
            raise ValueError(f"unplanned trial_id {trial_id!r}")
        if trial_id in seen:
            raise ValueError(f"duplicate planned trial {trial_id}")
        seen.add(trial_id)
        for key, expected in trial_metadata(plan, trials[trial_id], catalog=catalog).items():
            if key not in row or type(row[key]) is not type(expected) or row[key] != expected:
                raise ValueError(f"{trial_id}: {key} differs from frozen plan")
        if ("rc" not in row or row["rc"] is not None and type(row["rc"]) is not int
                or type(row.get("result_observed")) is not bool
                or "is_error" not in row or row["is_error"] is not None and type(row["is_error"]) is not bool
                or row["rc"] is None and (not isinstance(row.get("error"), str) or not row["error"].strip())):
            raise ValueError(f"{trial_id}: planned metadata is not a terminal attempt receipt")
        _oracle_identity(row, catalog["tasks"][row["task"]])
        for field in MEASURES:
            _finite(row.get(field), field)
        if isinstance(row.get("usage"), dict):
            for field in TOKEN_FIELDS:
                _finite(row["usage"].get(field), field)
        # New planned trials cannot upgrade a legacy total into component evidence.
        # Missing usage still admits a failed attempt; all measured means stay null.
        admitted.append(dict(row, usage=row.get("usage"), _cohort=plan["design"]["experiment_id"]))
    return admitted


def load_receipts(plan, paths):
    """Keep canonical loader checks while admitting strict plan-bound JSON only."""
    plan = validate_plan(plan)
    return _admit_rows(plan, scoreboard.load(paths, allow_empty=True, decoder=parse_json, legacy_defaults=False))


def schedule_rows(plan, partition):
    """Export metadata in the frozen order without launching or inventing trials."""
    plan = validate_plan(plan)
    if partition not in ("development", "holdout"):
        raise ValueError("partition must be development or holdout")
    catalog = _catalog(plan)
    return [trial_metadata(plan, trial, catalog=catalog) for trial in plan["schedule"]
            if catalog["tasks"][trial["task"]]["partition"] == partition]


def _summary(planned, rows):
    observed_ids = {r["trial_id"] for r in rows}
    missing = [t["trial_id"] for t in planned if t["trial_id"] not in observed_ids]
    verified = sum(quality_verified(r) for r in rows)
    successes = sum(scoreboard.verified_success(r) for r in rows)
    metrics = {}
    for field in MEASURES:
        metric = scoreboard.metric(rows, field)
        _finite(metric["observed_total"], field + " total")
        _finite(metric["mean"], field + " mean")
        metric.update(planned_runs=len(planned), unobserved_runs=len(missing),
                      missing_runs=len(planned) - metric["measured_runs"])
        if metric["missing_runs"]:
            metric["mean"] = None
        metrics[field] = metric
    per_success = {}
    for field in ("tokens", "cost_usd"):
        metric = metrics[field]
        per_success[field] = (metric["observed_total"] / successes
                              if successes and metric["missing_runs"] == 0 else None)
    return dict(planned_trials=len(planned), observed_trials=len(rows),
                missing_trial_ids=missing, completed_trials=sum(completed(r) for r in rows),
                harness_error_trials=sum(bool(r.get("error")) for r in rows),
                verified_quality_trials=verified, verified_success_trials=successes,
                verified_success_rate=(successes / len(planned)
                                       if not missing and verified == len(planned) else None),
                metrics=metrics, cost_per_verified_success=per_success)


def _value(row, metric):
    if metric == "verified_success":
        return int(scoreboard.verified_success(row))
    if metric == "escaped_fraction":
        return scoreboard.escape(row)
    if metric == "tokens":
        return token_count(row)
    return nonnegative(row.get(metric))


def _comparison(tasks, repetitions, rows, controls, seed):
    expected = len(tasks) * repetitions
    a = {(r["task"], r["rep"]): r for r in rows}
    b = {(r["task"], r["rep"]): r for r in controls}
    core = scoreboard.paired(rows, controls)
    complete = len(a) == len(b) == expected and core["complete_pairing"]
    verified = complete and core["verified_quality_pairs"] == expected
    usage = complete and core["tokens_measured_pairs"] == expected
    reasons = []
    if not complete:
        reasons.append("incomplete_planned_population")
    if core["verified_quality_pairs"] != expected:
        reasons.append("quality_not_verified")
    if core["tokens_measured_pairs"] != expected:
        reasons.append("missing_usage")
    estimates = {}
    for metric in ("verified_success", "escaped_fraction", "tokens", "cost_usd", "agent_wall_s"):
        observations = []
        if verified:
            for key in sorted(a):
                av, bv = _value(a[key], metric), _value(b[key], metric)
                if av is None or bv is None:
                    break
                observations.append(dict(task=key[0], family=tasks[key[0]]["family"],
                                         rep=key[1], arm=av, control=bv))
        estimates[metric] = (cluster_summary(observations, seed=seed)
                             if len(observations) == expected else None)
    regressions = []
    if verified:
        for task in sorted(tasks):
            pairs = [(a[(task, rep)], b[(task, rep)]) for rep in range(1, repetitions + 1)]
            success_delta = sum(int(scoreboard.verified_success(x)) - int(scoreboard.verified_success(y))
                                for x, y in pairs) / repetitions
            escape_delta = sum(scoreboard.escape(x) - scoreboard.escape(y) for x, y in pairs) / repetitions
            if success_delta < 0 or escape_delta > 0:
                regressions.append(dict(task=task, verified_success_delta=success_delta,
                                        escaped_fraction_delta=escape_delta))
    return dict(status=reasons[0] if reasons else "descriptive_complete", blockers=reasons,
                expected_pairs=expected, observed_pairs=core["pairs"],
                verified_quality_pairs=core["verified_quality_pairs"],
                usage_measured_pairs=core["tokens_measured_pairs"],
                complete_planned_pairing=complete, estimates=estimates,
                quality_regressions=regressions, promotion_decision="not_made")


def build_report(plan, rows, *, partition="development"):
    """Build a selected-partition report from rows admitted by load_receipts."""
    plan = validate_plan(plan)
    if partition not in ("development", "holdout"):
        raise ValueError("partition must be development or holdout")
    design = plan["design"]
    tasks = {t["id"]: t for t in design["tasks"] if t["partition"] == partition}
    normalized = _admit_rows(plan, rows)
    groups = []
    strata = sorted({t["stratum"] for t in tasks.values()})
    for scope in [None, *strata]:
        subjects = {key: task for key, task in tasks.items() if scope is None or task["stratum"] == scope}
        planned = collections.defaultdict(list)
        observed = collections.defaultdict(list)
        for trial in plan["schedule"]:
            if trial["task"] in subjects:
                planned[(trial["model"], trial["arm"])].append(trial)
        for row in normalized:
            if row["task"] in subjects:
                observed[(row["model"], row["arm"])].append(row)
        for model in design["models"]:
            for arm in design["arms"]:
                key = (model["id"], arm["id"])
                group = dict(partition=partition, stratum=scope, model=model["id"], arm=arm["id"],
                             task_ids=sorted(subjects), task_count=len(subjects),
                             family_count=len({t["family"] for t in subjects.values()}),
                             summary=_summary(planned[key], observed[key]), comparison=None)
                if arm["id"] != design["control_arm"]:
                    group["comparison"] = _comparison(subjects, design["repetitions"], observed[key],
                        observed[(model["id"], design["control_arm"])], design["seed"])
                groups.append(group)
    comparisons = [g["comparison"] for g in groups if g["stratum"] is None and g["comparison"]]
    return dict(report_schema_version=1, experiment_id=design["experiment_id"],
                plan_sha256=plan["plan_sha256"], partition=partition, control_arm=design["control_arm"],
                planned_trials_total=len(plan["schedule"]),
                planned_trials_selected=sum(t["task"] in tasks for t in plan["schedule"]),
                observed_trials_selected=sum(r["task"] in tasks for r in normalized),
                observed_trials_other_partition=sum(r["task"] not in tasks for r in normalized),
                missing_strata=sorted({"routine", "no_op", "negative_control", "challenging"} - set(strata)),
                model_count=len(design["models"]),
                provider_count=len({m["provider"] for m in design["models"]}),
                comparison_complete=bool(comparisons) and all(c["status"] == "descriptive_complete" for c in comparisons),
                promotion_decision="not_made", analyzer_correctness_evidence="not_evaluated_here", groups=groups)


def _label(value):
    return str(value).replace("|", "\\|").replace("\n", " ").replace("\r", " ").replace("`", "'")


def render(report):
    """Expose planned coverage and independent axes without a single win score."""
    lines = ["# Planned RIPR experiment", "",
             f"Experiment: `{_label(report['experiment_id'])}`. Partition: **{report['partition']}**.",
             f"Frozen plan: `{report['plan_sha256']}`. Control: `{_label(report['control_arm'])}`.",
             f"Receipts: {report['observed_trials_selected']}/{report['planned_trials_selected']} planned trials in this partition.",
             "Complete comparison: " + str(report["comparison_complete"]).lower() + ". Promotion decision: **not made**.",
             "Fresh analyzer correctness is a separate prerequisite. These observations do not establish causal benefit.",
             "Bootstrap intervals describe sensitivity to curated family composition; repeated trials are not independent tasks.",
             f"Providers/models: {report['provider_count']}/{report['model_count']}. Missing strata: {', '.join(report['missing_strata']) or 'none'}.", "",
             "| Model / arm / stratum | Tasks / families | Observed / planned | Verified quality / planned | Verified success | Tokens observed | Missing token trials | Tokens / verified success | Comparison |",
             "|---|---:|---:|---:|---:|---:|---:|---:|---|"]
    for group in report["groups"]:
        summary = group["summary"]
        tokens = summary["metrics"]["tokens"]
        label = " / ".join(_label(x) for x in (group["model"], group["arm"], group["stratum"] or "all"))
        status = group["comparison"]["status"] if group["comparison"] else "control"
        lines.append(f"| {label} | {group['task_count']}/{group['family_count']} | "
                     f"{summary['observed_trials']}/{summary['planned_trials']} | "
                     f"{summary['verified_quality_trials']}/{summary['planned_trials']} | "
                     f"{summary['verified_success_trials']} | {scoreboard.cell(tokens['observed_total'])} | "
                     f"{tokens['missing_runs']} | {scoreboard.cell(summary['cost_per_verified_success']['tokens'])} | {status} |")
    lines += ["", "## Paired contrasts", "",
              "Deltas are arm minus control, equally weighted across tasks. Token ratios use pooled sums.",
              "A cheaper arm with worse verified quality is a tradeoff. No result here authorizes promotion.", "",
              "| Model / arm / stratum | Pairs / planned | Verified success delta | Escaped fraction delta | Token delta | Token ratio | Token delta family interval | Quality regressions |",
              "|---|---:|---:|---:|---:|---:|---|---|"]
    for group in report["groups"]:
        comparison = group["comparison"]
        if comparison is None:
            continue
        estimates = comparison["estimates"]
        def estimate(field, key="point_delta"):
            return scoreboard.cell(estimates[field][key] if estimates[field] else None)
        interval = estimates["tokens"]["delta_interval_95"] if estimates["tokens"] else None
        interval_text = "not established" if interval is None else " to ".join(scoreboard.cell(x) for x in interval)
        label = " / ".join(_label(x) for x in (group["model"], group["arm"], group["stratum"] or "all"))
        regressions = ", ".join(_label(r["task"]) for r in comparison["quality_regressions"])
        lines.append(f"| {label} | {comparison['observed_pairs']}/{comparison['expected_pairs']} | "
                     f"{estimate('verified_success')} | {estimate('escaped_fraction')} | {estimate('tokens')} | "
                     f"{estimate('tokens', 'ratio')} | {interval_text} | {regressions or 'none observed / see completeness'} |")
    lines += ["", "## Missing planned trial IDs", ""]
    missing = sorted({trial for group in report["groups"] if group["stratum"] is None
                      for trial in group["summary"]["missing_trial_ids"]})
    lines += [f"- `{trial}`" for trial in missing] or ["None in the selected partition."]
    return "\n".join(lines) + "\n"
