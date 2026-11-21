#!/usr/bin/env python3
"""Bounded TypeScript real-mutant calibration for ripr (#12901).

For each mutant in typescript-calibration.json: copy the authored subject to
a scratch tree, apply the real source edit, run the subject's own node:test
suite (runtime outcome: caught when the suite fails, missed when it passes),
then run `ripr check --diff` on a synthetic one-hunk diff of the same edit and
read the classification on the mutated line. Unknown / weak / absent ripr
verdicts are recorded as unscored; they are never counted as successes and
never rewritten as runtime outcomes.

Usage:
  python benchmarks/mutation_spot_check/run_typescript_calibration.py \
      --ripr target/debug/ripr --node node
Writes typescript-calibration-receipt.{json,md} beside this script by default.
"""
import argparse, json, shutil, subprocess, sys, tempfile
from pathlib import Path

GRIP = {
    "exposed": "strongly_gripped",
    "reachable_unrevealed": "ungripped",
    "no_static_path": "ungripped",
    "weakly_exposed": "weakly_gripped",
    "infection_unknown": "activation_unknown",
    "propagation_unknown": "opaque",
    "static_unknown": "opaque",
}
SCORED_GRIPS = {"strongly_gripped", "ungripped"}


def run(cmd, cwd, timeout=180):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def copy_subject(src: Path, dst: Path):
    shutil.copytree(src, dst, ignore=shutil.ignore_patterns(
        "node_modules", "__pycache__", ".pytest_cache", "*.pyc"))


def verdict_for(check: dict, rel_file: str, line: int):
    """Return (token, grip, disposition). Mirrors synthetic_diff::read_verdict."""
    complete = check.get("analysis_outcome", {}).get("analysis_complete") is True
    matched = []
    for f in check.get("findings", []) or []:
        probe = f.get("probe", {}) or {}
        pf = str(probe.get("file", "")).removeprefix("./")
        if pf == rel_file and probe.get("line") == line:
            matched.append(f)
    if not matched:
        return "", "", ("analysis_incomplete" if not complete else "no_finding_on_mutated_lines")
    if not complete:
        return "", "", "analysis_incomplete"
    pairs = {(f.get("classification", ""), (f.get("probe", {}) or {}).get("owner", "")) for f in matched}
    if len(pairs) != 1:
        return "", "", "ambiguous_verdict"
    token, _owner = next(iter(pairs))
    if not token:
        return "", "", "ambiguous_verdict"
    return token, GRIP.get(token, token), "canonical_precise"


def main() -> int:
    ap = argparse.ArgumentParser()
    here = Path(__file__).resolve().parent
    root = here.parents[1]
    ap.add_argument("--repo-root", default=str(root))
    ap.add_argument("--manifest", default=str(here / "typescript-calibration.json"))
    ap.add_argument("--ripr", required=True)
    ap.add_argument("--node", default="node")
    ap.add_argument("--out-json", default=str(here / "typescript-calibration-receipt.json"))
    ap.add_argument("--out-md", default=str(here / "typescript-calibration-receipt.md"))
    ap.add_argument("--scratch", default="")
    args = ap.parse_args()

    root = Path(args.repo_root).resolve()
    manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    subjects = {s["name"]: s for s in manifest["subjects"]}
    _, ripr_version = run([args.ripr, "--version"], root)
    _, node_version = run([args.node, "--version"], root)

    scratch_root = Path(args.scratch) if args.scratch else Path(tempfile.mkdtemp(prefix="ripr-ts-cal-"))
    scratch_root.mkdir(parents=True, exist_ok=True)
    rows, baselines = [], {}
    try:
        for name, subj in subjects.items():
            src = root / subj["path"]
            dst = scratch_root / f"baseline-{name}"
            copy_subject(src, dst)
            code, out = run([args.node, *subj["test_command"]], dst)
            baselines[name] = {"exit_code": code, "passed": code == 0,
                               "summary_tail": out.strip().splitlines()[-1] if out.strip() else ""}
            if code != 0:
                print(f"baseline FAILED for {name}:\n{out[-2000:]}", file=sys.stderr)

        for m in manifest["mutants"]:
            subj = subjects[m["subject"]]
            src = root / subj["path"]
            work = scratch_root / f"mutant-{m['id']}"
            copy_subject(src, work)
            target = work / m["file"]
            original_text = target.read_text(encoding="utf-8")
            assert original_text.count(m["original"]) == 1, f"{m['id']}: original not unique"
            mutated_text = original_text.replace(m["original"], m["mutated"])
            line_no = original_text[:original_text.index(m["original"])].count("\n") + 1
            target.write_text(mutated_text, encoding="utf-8")

            # Runtime outcome against the subject's real tests.
            if not baselines[m["subject"]]["passed"]:
                runtime, test_exit = "unscoreable", None
            else:
                test_exit, _out = run([args.node, *subj["test_command"]], work)
                runtime = "missed" if test_exit == 0 else "caught"

            # ripr verdict on the same edit (synthetic one-hunk diff).
            diff = (f"diff --git a/{m['file']} b/{m['file']}\n--- a/{m['file']}\n"
                    f"+++ b/{m['file']}\n@@ -{line_no},1 +{line_no},1 @@\n"
                    f"-{m['original']}\n+{m['mutated']}\n")
            diff_path = work / "mutant.diff"
            diff_path.write_text(diff, encoding="utf-8")
            (work / "ripr.toml").write_text('[languages]\nenabled = ["rust", "typescript"]\n', encoding="utf-8")
            token, grip, disposition = "", "", "ripr_check_failed"
            code, out = run([args.ripr, "check", "--diff", str(diff_path), "--root", str(work),
                             "--format", "json", "--quiet"], work)
            if code == 0:
                try:
                    token, grip, disposition = verdict_for(json.loads(out), m["file"], line_no)
                except json.JSONDecodeError:
                    disposition = "ripr_check_failed"
            scored = runtime in ("caught", "missed") and grip in SCORED_GRIPS and disposition == "canonical_precise"
            if scored and grip == "strongly_gripped":
                agreement = "agree" if runtime == "caught" else "overclaim"
            elif scored and grip == "ungripped":
                agreement = "agree" if runtime == "missed" else "false_gap"
            else:
                agreement = "unscored"
            rows.append({
                "id": m["id"], "subject": m["subject"], "file": m["file"], "line": line_no,
                "operator_family": m["operator_family"], "mutator": m["mutator"],
                "original": m["original"].strip(), "mutated": m["mutated"].strip(),
                "runtime_outcome": runtime, "test_exit": test_exit,
                "ripr_classification": token, "grip_class": grip,
                "disposition": disposition if scored else (disposition if disposition != "canonical_precise" else "unscored_grip"),
                "scored": scored, "agreement": agreement,
            })
            print(f"{m['id']}: runtime={runtime} ripr={token or disposition} grip={grip or '-'} {agreement}")
    finally:
        if not args.scratch:
            shutil.rmtree(scratch_root, ignore_errors=True)

    scored_rows = [r for r in rows if r["scored"]]
    tp = sum(1 for r in scored_rows if r["grip_class"] == "strongly_gripped" and r["runtime_outcome"] == "caught")
    fp = sum(1 for r in scored_rows if r["grip_class"] == "strongly_gripped" and r["runtime_outcome"] == "missed")
    fn = sum(1 for r in scored_rows if r["grip_class"] == "ungripped" and r["runtime_outcome"] == "caught")
    tn = sum(1 for r in scored_rows if r["grip_class"] == "ungripped" and r["runtime_outcome"] == "missed")
    missed_all = [r for r in rows if r["runtime_outcome"] == "missed"]
    flagged_missed = sum(1 for r in missed_all if r["grip_class"] in
                         ("ungripped", "weakly_gripped", "activation_unknown", "opaque"))
    def rate(n, d): return (n / d) if d else None
    summary = {
        "mutants_total": len(rows),
        "operators": sorted({r["operator_family"] for r in rows}),
        "subjects": sorted({r["subject"] for r in rows}),
        "runtime_caught": sum(1 for r in rows if r["runtime_outcome"] == "caught"),
        "runtime_missed": len(missed_all),
        "runtime_unscoreable": sum(1 for r in rows if r["runtime_outcome"] == "unscoreable"),
        "scored_claim_pairs": len(scored_rows),
        "unscored": len(rows) - len(scored_rows),
        "confusion_scored_only": {"strong_caught": tp, "strong_missed_overclaim": fp,
                                  "ungripped_caught_false_gap": fn, "ungripped_missed": tn},
        "discriminator_precision": rate(tp, tp + fp),
        "discriminator_precision_counts": f"{tp}/{tp + fp}",
        "missed_recall_strict": rate(tn, tn + fp),
        "missed_recall_strict_counts": f"{tn}/{tn + fp}",
        "caught_recall": rate(tp, tp + fn),
        "caught_recall_counts": f"{tp}/{tp + fn}",
        "flagged_or_unknown_share_of_missed": rate(flagged_missed, len(missed_all)),
        "flagged_or_unknown_counts": f"{flagged_missed}/{len(missed_all)}",
        "unknowns_are_successes": False,
    }
    receipt = {
        "schema_version": "ripr-typescript-mutant-calibration-receipt-v1",
        "issue": 12901, "kind": "bounded_typescript_real_mutant_calibration",
        "agreement_method": "synthetic_diff_verdict",
        "claim_boundary": "Authored in-repo TypeScript subjects only; advisory calibration, not suite adequacy or a population estimate. Unknown/weak ripr verdicts are unscored, never successes.",
        "environment": {"ripr": ripr_version.strip(), "node": node_version.strip()},
        "baselines": baselines, "summary": summary, "mutants": rows,
    }
    Path(args.out_json).write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    lines = ["# TypeScript real-mutant calibration receipt (#12901)", "",
             "Bounded slice over authored in-repo TypeScript subjects. Runtime outcomes come from the",
             "subjects' own node:test suites; ripr verdicts come from `ripr check --diff`. Unknown or",
             "weak verdicts are unscored, never successes.", "",
             f"- ripr: `{receipt['environment']['ripr']}`; node: `{receipt['environment']['node']}`",
             f"- Mutants: {summary['mutants_total']} across operators {', '.join(summary['operators'])}",
             f"- Runtime: caught {summary['runtime_caught']}, missed {summary['runtime_missed']}, unscoreable {summary['runtime_unscoreable']}",
             f"- Scored claim pairs: {summary['scored_claim_pairs']} (unscored {summary['unscored']})",
             f"- Discriminator precision: {summary['discriminator_precision']} ({summary['discriminator_precision_counts']})",
             f"- Missed recall (strict, ungripped of scored missed): {summary['missed_recall_strict']} ({summary['missed_recall_strict_counts']})",
             f"- Caught recall (strong of scored caught): {summary['caught_recall']} ({summary['caught_recall_counts']})",
             f"- Flagged-or-unknown share of missed: {summary['flagged_or_unknown_share_of_missed']} ({summary['flagged_or_unknown_counts']})", "",
             "| Mutant | Operator | Runtime | ripr | Grip | Agreement |", "| --- | --- | --- | --- | --- | --- |"]
    for r in rows:
        lines.append(f"| {r['id']} | {r['operator_family']} | {r['runtime_outcome']} | {r['ripr_classification'] or r['disposition']} | {r['grip_class'] or '-'} | {r['agreement']} |")
    Path(args.out_md).write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
