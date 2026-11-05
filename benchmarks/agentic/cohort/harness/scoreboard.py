"""Descriptive, paired cohort accounting. Usage: scoreboard.py OUT receipts.jsonl [...].

Failed attempts stay in denominators. Unknown cost stays null. Legacy inputs are
file-scoped; new receipts carry cohort and task/protocol identities. No statistical
significance, causal uplift or token-saving verdict is inferred by this report.
"""
import collections
import hashlib
import json
import math
from pathlib import Path
import statistics as st
import sys

from measurements import TOKEN_FIELDS, completed, nonnegative, token_count


def wilson(k, n, z=1.96):
    if not n:
        return (None, None)
    p = k / n
    d = 1 + z * z / n
    c = p + z * z / (2 * n)
    r = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return (max(0.0, (c - r) / d), min(1.0, (c + r) / d))


def cfgname(row):
    return {"A0-none": "none", "A1-diagnostics": "check-mandatory",
            "A2-packet-loop": "brief-mandatory"}.get(row["arm"], row["arm"])


def load(paths):
    rows, seen = [], set()
    for path in paths:
        data = Path(path).read_bytes()
        legacy_cohort = "legacy-file:" + hashlib.sha256(data).hexdigest()
        for line_number, line in enumerate(data.decode("utf-8").splitlines(), 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
                if not isinstance(row, dict):
                    raise ValueError("receipt must be an object")
                for key in ("task", "arm", "model"):
                    if not isinstance(row.get(key), str) or not row[key].strip():
                        raise ValueError(f"missing or invalid {key}")
                if nonnegative(row.get("rep"), integer=True) is None:
                    raise ValueError("rep must be a nonnegative integer")
                # Old successful receipts predate the configurable cap (40).
                # An old exception receipt omitted the cap: keep it unknown.
                row.setdefault("max_turns", None if row.get("error") else 40)
                cap = row["max_turns"]
                if cap is not None and (nonnegative(cap, integer=True) is None or cap == 0):
                    raise ValueError("max_turns must be positive or null")
                cohort = row.get("cohort_id", legacy_cohort)
                if not isinstance(cohort, str) or not cohort.strip():
                    raise ValueError("cohort_id must be nonempty")
                row["_cohort"] = cohort
                if row.get("ripr_version") is not None and not isinstance(row["ripr_version"], str):
                    raise ValueError("ripr_version must be a string or null")
                if "usage" in row and token_count(row) is not None and row.get("tokens") != token_count(row):
                    raise ValueError("tokens disagrees with terminal usage components")
                for key in ("task_sha256", "protocol_sha256"):
                    value = row.get(key)
                    if value is not None and (not isinstance(value, str) or len(value) != 64
                                               or any(c not in "0123456789abcdef" for c in value)):
                        raise ValueError(f"invalid {key}")
                identity = (cohort, row["model"], cap, cfgname(row), row["task"], row["rep"])
                if identity in seen:
                    raise ValueError(f"duplicate attempt {identity}; use distinct cohort IDs for new trials")
                seen.add(identity)
                rows.append(row)
            except (ValueError, TypeError) as error:
                raise ValueError(f"{path}:{line_number}: {error}") from error
    if not rows:
        raise ValueError("no receipts; an empty experiment is not a result")
    return rows


def escape(row):
    """Measured escape fraction, not a guessed 100% for an unscored failure."""
    n = nonnegative(row.get("mutants"), integer=True)
    k = nonnegative(row.get("killed"), integer=True)
    survivors = row.get("survived")
    if (row.get("valid") is not True or row.get("src_unchanged") is not True
            or row.get("score_error") or row.get("error") or not n or k is None or k > n
            or not isinstance(survivors, list)
            or any(type(i) is not int or not 0 <= i < n for i in survivors)
            or len(set(survivors)) != len(survivors) or len(survivors) != n - k):
        return None
    return (n - k) / n


def success(row):
    return completed(row) and escape(row) == 0.0


def metric(rows, field):
    values = []
    for row in rows:
        if field == "tokens":
            value = token_count(row)
        elif field in TOKEN_FIELDS:
            usage = row.get("usage", row)
            observed = "usage" not in row or row.get("result_observed") is True
            value = nonnegative(usage.get(field), integer=True) if observed and isinstance(usage, dict) else None
        else:
            value = nonnegative(row.get(field))
        values.append(value)
    measured = [v for v in values if v is not None]
    return dict(measured_runs=len(measured), missing_runs=len(rows) - len(measured),
                observed_total=sum(measured) if measured else None,
                mean=st.mean(measured) if len(measured) == len(rows) and measured else None)


def mean(values):
    return st.mean(values) if values else None


def ratio(numerator, denominator):
    return numerator / denominator if numerator is not None and denominator not in (None, 0) else None


def paired(rows, controls):
    arm = {(r["task"], r["rep"]): r for r in rows}
    base = {(r["task"], r["rep"]): r for r in controls}
    keys = sorted(arm.keys() & base.keys())
    pairs, mismatches = [], []
    for key in keys:
        a, b = arm[key], base[key]
        # Equal names/denominators alone do not establish equal task contents.
        if any(a.get(k) != b.get(k) for k in ("task_sha256", "protocol_sha256", "mutants")):
            mismatches.append(list(key))
        else:
            pairs.append((a, b))
    costs = [(token_count(a), token_count(b)) for a, b in pairs]
    measured_costs = [(a, b) for a, b in costs if a is not None and b is not None]
    quality = [(escape(a), escape(b)) for a, b in pairs]
    measured_quality = [(a, b) for a, b in quality if a is not None and b is not None]
    full = bool(pairs) and len(pairs) == len(rows) == len(controls)
    cost_complete = full and len(measured_costs) == len(pairs)
    quality_complete = full and len(measured_quality) == len(pairs)
    return dict(pairs=len(pairs), unmatched_runs=[list(k) for k in sorted(arm.keys() - base.keys())],
                unmatched_control_runs=[list(k) for k in sorted(base.keys() - arm.keys())],
                subject_mismatches=mismatches, complete_pairing=full,
                task_protocol_identity_matched=bool(pairs) and all(a.get("task_sha256") and a.get("protocol_sha256") for a, _ in pairs),
                tokens_measured_pairs=len(measured_costs), quality_measured_pairs=len(measured_quality),
                observed_token_delta_mean=mean([a - b for a, b in measured_costs]),
                tokens_vs_none_x=ratio(sum(a for a, _ in measured_costs), sum(b for _, b in measured_costs)) if cost_complete else None,
                fix_rate_vs_none=mean([int(success(a)) - int(success(b)) for a, b in pairs]) if full else None,
                escape_rate_vs_none=mean([a - b for a, b in measured_quality]) if quality_complete else None)


def build_report(rows):
    groups = collections.defaultdict(list)
    for row in rows:
        groups[(row["_cohort"], row["model"], row["max_turns"], cfgname(row))].append(row)
    table = []
    for (cohort, model, cap, config), runs in sorted(groups.items(), key=lambda item: repr(item[0])):
        versions = {r["ripr_version"] for r in runs if r.get("ripr_version")}
        if len(versions) > 1:
            raise ValueError(f"mixed ripr versions in cohort {cohort}, {config}; split the cohort")
        n = len(runs)
        fixed = sum(success(r) for r in runs)
        scores = [escape(r) for r in runs]
        measured_scores = [s for s in scores if s is not None]
        metrics = {field: metric(runs, field) for field in
                   ("tokens", "cost_usd", "agent_wall_s", "turns", "tool_calls", "ripr_calls", *TOKEN_FIELDS)}
        row = dict(report_schema_version=2, cohort=cohort, ripr=next(iter(versions), None), model=model,
                   max_turns=cap, config=config, runs=n, successful_runs=fixed,
                   completed_runs=sum(completed(r) for r in runs),
                   harness_error_runs=sum(bool(r.get("error")) for r in runs),
                   unscored_runs=n - len(measured_scores), fix_rate=fixed / n,
                   fix_ci95=wilson(fixed, n), escape_rate=mean(measured_scores),
                   escape_measured_runs=len(measured_scores), metrics=metrics,
                   tokens=metrics["tokens"]["mean"], cost_usd=metrics["cost_usd"]["mean"],
                   cost_source="sdk_estimate", paired=None,
                   tokens_vs_none_x=None, fix_rate_vs_none=None)
        for field in ("tokens", "cost_usd"):
            m = metrics[field]
            row[field + "_per_success"] = (m["observed_total"] / fixed
                                           if fixed and m["missing_runs"] == 0 else None)
        if config != "none" and cap is not None:
            row["paired"] = paired(runs, groups.get((cohort, model, cap, "none"), []))
            row["tokens_vs_none_x"] = row["paired"]["tokens_vs_none_x"]
            row["fix_rate_vs_none"] = row["paired"]["fix_rate_vs_none"]
        table.append(row)
    return table


def cell(value, fmt=".3g"):
    return "not measured" if value is None else format(value, fmt)


def render(table):
    cohorts = {name: f"C{i + 1}" for i, name in enumerate(sorted({r["cohort"] for r in table}))}
    def label(row):
        return f"{cohorts[row['cohort']]} / {row['model']} / {row['max_turns']} / {row['config']}"

    lines = ["# Agent cohort measurement report", "",
             "Descriptive only: token volume is not billed cost or evidence of quality improvement.",
             "Cost per success includes all attempts; incomplete cost stays not measured.",
             "Full versions, cache components, observed totals and unmatched keys are in the JSON report.", ""]
    lines += [f"{alias}: `{name}`" for name, alias in cohorts.items()]
    lines += ["", "| Cohort / model / turn cap / config | Attempts | Complete | Success | Scored | Escape | Tokens mean [n/N] | SDK $ mean [n/N] | Tokens/success |",
              "|---|---:|---:|---:|---:|---:|---:|---:|---:|"]
    for row in table:
        tokens, cost = row["metrics"]["tokens"], row["metrics"]["cost_usd"]
        n = row["runs"]
        lines.append(f"| {label(row)} | {n} | {row['completed_runs']} | {row['successful_runs']} | "
                     f"{row['escape_measured_runs']}/{n} | {cell(row['escape_rate'], '.1%')} | "
                     f"{cell(tokens['mean'])} [{tokens['measured_runs']}/{n}] | "
                     f"{cell(cost['mean'])} [{cost['measured_runs']}/{n}] | {cell(row['tokens_per_success'])} |")
    lines += ["", "## Paired comparisons against none", "",
              "Hashes matched means recorded task/protocol digests agree, not independent attestation.", "",
              "| Group | Pairs | Unmatched arm/control | Subject mismatches | Hashes matched | Token/quality pairs | Token x none | Success delta | Escape delta |",
              "|---|---:|---:|---:|---|---:|---:|---:|---:|"]
    for row in table:
        p = row["paired"]
        if p is not None:
            lines.append(f"| {label(row)} | {p['pairs']} | {len(p['unmatched_runs'])}/{len(p['unmatched_control_runs'])} | "
                         f"{len(p['subject_mismatches'])} | {p['task_protocol_identity_matched']} | "
                         f"{p['tokens_measured_pairs']}/{p['quality_measured_pairs']} | {cell(p['tokens_vs_none_x'])} | "
                         f"{cell(p['fix_rate_vs_none'], '+.1%')} | {cell(p['escape_rate_vs_none'], '+.1%')} |")
    return "\n".join(lines) + "\n"


def main(out, paths):
    table = build_report(load(paths))
    markdown = render(table)
    Path(out + ".json").write_text(json.dumps(table, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    Path(out + ".md").write_text(markdown, encoding="utf-8")
    print(markdown)


if __name__ == "__main__":
    try:
        if len(sys.argv) < 3:
            raise ValueError("usage: scoreboard.py OUT_PREFIX receipts.jsonl [...]")
        main(sys.argv[1], sys.argv[2:])
    except (OSError, ValueError) as error:
        print(f"scoreboard: {error}", file=sys.stderr)
        sys.exit(2)
