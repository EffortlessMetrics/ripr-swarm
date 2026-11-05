# Agent cohort: with and without ripr (#12902)

Question: do agents given ripr leave fewer escaped defects, in fewer steps, than agents without it?

## Protocol

Each task is a small Rust crate with two commits: a base, and a "feature" commit that changes `src/` and adds a thin test file. A headless Claude agent (`claude -p`, narrow tool grant: file tools, `cargo test`, `ripr`, read-only git) is told to strengthen the tests under `tests/` (no `src/` edits) so a later bug in the changed behavior would fail them.

Arms (same brief, plus one paragraph):

- `A0-none`: `ripr` is shadowed by a failing shim.
- `A1-diagnostics`: told to run `ripr check --base HEAD~1` first.
- `A2-packet-loop`: told to run `ripr agent brief --base HEAD~1 --json` first and follow its `next` commands.

Hidden oracle: the agent's final `tests/` are run on the pristine code (must pass) and on each hidden mutant of `src/lib.rs`. Escaped defect rate = surviving mutants / mutants. A mutant that times out or fails a test counts as killed. Per run we record turns, tool calls, ripr calls, tokens, cost, wall time (`receipts/`).

Tasks: 8 hand-written single-function tasks with hand-written mutants (each validated killable by a reference suite) and 3 larger multi-function tasks (`calendar`, `expr`, `textfmt`) whose mutants are generated mechanically (`harness/mutate.py`) and kept only if a Sonnet-written oracle suite kills them (`harness/oracle.py`).

## Result (2026-11-04, ripr 0.10.0 installed binary, 99 runs, no invalid runs)

Escaped-defect rate, mean over runs (tasks × reps):

| Model | Arm | Runs | Escape rate | Runs with any escape | Turns | Tokens | Wall s |
|---|---|---|---|---|---|---|---|
| Haiku | none | 22 | 0.4% | 3 | 5.2 | 202k | 30 |
| Haiku | `ripr check` | 22 | 0.7% | 4 | 6.6 | 280k | 34 |
| Haiku | `ripr agent brief` | 22 | 0.7% | 4 | 13.5 | 712k | 60 |
| Sonnet | none | 11 | 1.4% | 2 | 5.0 | 197k | 24 |
| Sonnet | `ripr check` | 11 | 0.2% | 1 | 6.4 | 266k | 29 |
| Sonnet | `ripr agent brief` | 11 | 1.7% | 3 | 5.6 | 250k | 29 |

**Finding: no measurable lift, because the tasks have no headroom.** Unaided agents already kill over 98% of hidden mutants. The differences between arms are within run-to-run noise (a handful of survivors, mostly the same mutants in `expr` and `calendar`). For Haiku the brief loop cost 3.5× the tokens and 2× the wall time with the same escape rate. This does not show that ripr fails to help; it shows these tasks cannot detect help.

## Second result: real code, config matrix, current main (2026-11-04, ripr 0.11.0 @ 7d04028)

Three published crates already vendored in the local cargo registry (`strsim`, `shlex`, `percent-encoding`; 325-756 lines, doctests off) with 44-50 mechanical mutants each, filtered by two Sonnet-written oracle suites. 36 runs: tasks × {none, `ripr check`, `ripr agent brief`} × {Haiku, Sonnet} × 2 reps, all mandatory use. Receipts: `receipts/real-pilot-main-0.11.0-2026-11-04.jsonl`; scoreboard: `scoreboard/real-pilot-2026-11-04.md`.

- `shlex` and `percent-encoding` still saturate: every run kills every mutant, in every config.
- `strsim` (756 lines) has headroom. Mean mutants escaped, out of 46, over 4 runs per config (Haiku and Sonnet pooled): none 6.5, `ripr check` 7.5, `ripr agent brief` 9.5. Per-run range is 3 to 13. With 4 runs per cell, none of these gaps is distinguishable from noise, and the direction is not in ripr's favor.
- Tokens: brief loop 2.2× control for Haiku, about 1× for Sonnet.

So even with headroom, no config of ripr 0.11.0 beat an unaided agent in this pilot. The instrument now separates good from bad runs (escape 3 to 13), so more reps per cell can answer it.

## Third result: full config matrix on `strsim` (2026-11-04, ripr 0.11.0 @ 7d04028)

`strsim` has the headroom, so the whole matrix ran there: 9 configs plus an MCP variant that passes `base`, Haiku and Sonnet, 5 new runs per cell (7 for the none/check/brief cells that also had the earlier 2 runs). 100 runs, none invalid. Receipts: `receipts/strsim-matrix-main-0.11.0-2026-11-04.jsonl`; scoreboard: `scoreboard/strsim-matrix-2026-11-04.md`. Fix rate (zero escaped mutants) is near 0% in every cell, so the comparison is mean escaped-mutant rate, with a permutation test against no ripr.

| Config | Haiku (none 9.0%) | Sonnet (none 17.7%) |
|---|---|---|
| `ripr check` | 8.1% | 22.7% (p<0.01) |
| `ripr check --json` | 8.3% | 17.0% |
| `ripr check --mode fast` | 18.3% (p=0.03) | 22.6% (p=0.01) |
| `ripr agent brief` | 14.3% | 18.6% |
| `ripr agent brief --max-seams 1` | 14.3% | 19.6% |
| `ripr agent brief --max-seams 10` | 17.4% (p=0.05) | 25.2% (p=0.02) |
| `ripr agent brief`, optional use | 9.1% | 30.0% (p<0.01) |
| MCP, no `base` | 12.6% | 39.6% (p<0.01) |
| MCP, `base: HEAD~1` | 7.0% (p=0.60) | 25.7% (p=0.03) |

Reading it:

- **No config reduced escapes.** Several increased them. The best Haiku configs match no ripr within noise.
- **Noise is large.** The Sonnet brief-optional cell made zero ripr calls (it is effectively a control with an extra paragraph) and still came out 12 points worse than none. Cells have 5-7 runs; treat only the big gaps as signal, and expect individual p-values to move with more runs (about 30 comparisons were made, none corrected).
- **MCP default base is a trap.** With no `base`, `ripr_refresh` returned a completed zero-finding `no_scope` snapshot in 10 of 10 runs (#13096). Passing `base` fixed that for Haiku and cut Sonnet from 39.6% to 25.7%, which is still above the 17.7% control.
- **Fast mode and a large seam cap are the worst output settings;** `--json` is indistinguishable from plain `check`.

## Per-config scoreboard

`harness/scoreboard.py` reports every attempted run, measured-cost coverage,
completion and hidden-oracle outcomes, then compares matched task/replicate pairs
within the same cohort, model and turn budget. Missing usage is not zero; failed
attempts do not disappear. Full RIPR version strings are retained and mixed
versions within a cohort/config are rejected. See the
[measurement contract](MEASUREMENT_CONTRACT.md) for receipt/report v2, pairing,
cache accounting, cost per success and limitations.

Configs live in `CONFIGS` in `harness/run_one.py`: surface (check, brief, MCP),
optional vs mandatory use, output flags, model and turn cap (`COHORT_MAX_TURNS`).
Each sweep creates a fresh cohort ID; set `COHORT_ID` only to split one frozen
experiment across files. Historical tables and receipts above are retained as
original observations, not recomputed or statistically requalified by this change.

## Limits

- Known scorer limits (being repaired in a child PR): a timed-out or otherwise failing mutant run counts as killed, a pristine run that executes zero tests counts as valid, and scoring runs in the agent-edited checkout. Escape rates above are therefore best read as lower bounds with unverified scorer provenance.
- One provider (Anthropic), two models. The "two providers" bar is not met; no other provider is reachable from this environment.
- Mutants for the large and real-code tasks are mechanical (not cargo-mutants, which is not installed here) and filtered by Sonnet-written suites, so bugs those suites could not kill are excluded; this biases toward bugs that are easy to kill.
- First-party tasks are small (under 120 lines). Real-code tasks are single-crate, and only `src/lib.rs` is mutated.
- Installed ripr is 0.10.0, not main. Reps per cell are 1–2 per task; no significance test is claimed.
- Optional use: when ripr was only offered, agents ran it 0 of 3 times (#13050); the arms above force the first call.

## Run

```sh
cd benchmarks/agentic/cohort/harness
python3 validate.py                      # check every mutant is killable by the reference tests (small tasks)
python3 sweep.py all none,check-mandatory,brief-mandatory haiku 1 3 out.jsonl 4
python3 scoreboard.py out out.jsonl
```

Needs `claude` on PATH and `ripr`. Runs create unique trial directories under
`/tmp/cohort-work`; repeated trials do not overwrite prior transcripts or tests.
The old `agg.py` is a historical quick summary, not the measurement-integrity
report. Run the offline accounting tests without any model, Cargo or network:

```sh
python3 -m unittest discover -s benchmarks/agentic/cohort/harness -p 'test_*.py' -v
```

Run that test command from the repository root.
