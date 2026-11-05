# Agent discrimination benchmark (A/B harness)

Measures whether ripr-supplied context helps an LLM agent write a test that
**discriminates** a recorded behavior change — a test that passes when the
change is reverted and fails on the changed behavior — and what context bytes
that help costs. The objective scorer is `eval.sh`: a two-phase evaluator
that flips the crate to HEAD~1 (test must pass) and back to HEAD (test must
fail), scoring `DISCRIMINATES: yes` only on both exact outcomes.

This is **not** a product-quality or coverage corpus, and it is not a
mutation-testing engine. Benchmark claims are limited to the recorded
scenario revision (deterministically reproduced commit SHAs), the pinned
prompt template and context captures, the evaluator in this directory, the
recorded ripr binary version (seed study: ripr 0.11.0), and the runner class.
Nothing here measures general ripr accuracy, upstream crate quality, or
model-free difficulty.

## Scenario identity

`scenarios/<name>/{original,changed}/` hold the exact content of the two
commits under study; `make-scenario-crates.sh` materializes them as fresh
two-commit git repos with pinned identity and fails closed unless the rebuilt
HEADs equal the recorded seed-study SHAs (verified at encode time on the
recording host):

```text
inserted-guard     86625387aaf5908ada16a691fe11b2e1616f86ad
flipped-comparison 006aaf48af8a8e8582056d85ad87d12e9690682d
silent-return      ed7af52a019f78987958eaf4f0190d99e0a2a8b0
```

`contexts/<scenario>/` are the recorded Nov 2-4 context captures (byte counts
pinned in `scenarios/<name>/scenario.json`); `build-contexts.sh` regenerates
per-run contexts against a run-owned clone. The captured ripr/tool-note files
carry placeholders in place of the study host's machine paths at the
repository's request (`results/seed-study/README.md` documents the mapping
and the byte-count semantics); they are evidence, not paths to run against.
The per-run protocol is in `RUNBOOK.md`.

## Validity (issue #13113)

The Nov 2-4 seed-study harness shared per-scenario crates across concurrent
runs, left cargo off the harness PATH, swallowed cargo's true exit codes, and
wrote records with zeroed structured fields. This encoding fixes all four:
per-run cloned crates (one writer, run-namespaced scratch, never shared temp
space — the scripts refuse shared temp roots such as drive-letter Temp
directories, `/tmp`, and `/var/tmp`, and refuse
destinations under any Cargo workspace manifest, where cargo would claim the
materialized package), step-zero toolchain
PATH normalization in `eval.sh`/`build-contexts.sh` and the prompt, true cargo
exit codes captured in `run_phase`, and `record-run.sh` (validated structured
fields, atomic report writes, `summary.csv` derived from the reports).

## Recorded first calibration: results/seed-study/

The Nov 2-4 study ran **9 solver runs** (3 scenarios × 3 variants). **5 of 9
run records are retained** in `results/seed-study/` (+ `summary.csv`); the
other 4 records (the three `ripr-context` variant cells and
`silent-return-vanilla`) were not written before the harness was reworked and
are not recoverable — this recording loss is exactly defect 4 of issue
#13113, and `record-run.sh` exists so future runs cannot lose rows. Do not
average over the missing cells. Provenance and the encode-time path
sanitization are documented in `results/seed-study/README.md`.

Retained records (all `rounds: 1`, all `discriminates: yes`):

| run_id | variant | context_bytes |
|---|---|---|
| inserted-guard-vanilla-r01 | vanilla | 1178 |
| inserted-guard-ripr-tool-r01 | ripr-tool | 5004 |
| flipped-comparison-vanilla-r01 | vanilla | 1099 |
| flipped-comparison-ripr-tool-r01 | ripr-tool | 4254 |
| silent-return-ripr-tool-r02 | ripr-tool | 4794 |

## Caveats (small n, easy difficulty)

- n = 3 scenarios and 5 retained records: no variance or significance claim
  is possible; treat every number as a recorded observation of this revision,
  template, and evaluator, not a distribution.
- **Uplift is unmeasurable at this difficulty** (recorded finding): every
  retained cell — including both vanilla cells — already produced a
  discriminating test in 1 round, so the study contains no evidence that
  ripr context improves discrimination; it only records what ripr context
  costs in bytes at tasks vanilla already solves. Scenario hardening so that
  vanilla sometimes fails is a separate, not-implemented work item.
- The evaluator enforces direction only (pass on original, fail on changed);
  it does not judge test quality beyond that, and static `exposed` /
  `weakly_exposed` classifications from ripr are advisory inputs, never
  real mutation outcomes.

## Follow-ups (not implemented here)

- xtask runner integration (a `cargo xtask` driver for the per-run protocol).
- N-growth to >= 12 hardened scenarios where vanilla does not trivially
  succeed, so the context-cost/uplift tradeoff becomes measurable.
- Recovering the 4 unretained seed-study records is not possible; do not
  reconstruct them.

## Evaluator controls

`eval.sh` requires a discriminating test to flip phases (negative control: a
non-discriminating test scores `DISCRIMINATES: no`, exit 1; positive control:
the recorded study test scores `DISCRIMINATES: yes`, exit 0). Rerun both per
RUNBOOK.md before trusting new results.
