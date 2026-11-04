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

## Limits

- One provider (Anthropic), two models. The "two providers" bar is not met; no other provider is reachable from this environment.
- Mutants for the large tasks are filtered by Sonnet-written suites, so bugs those suites could not kill are excluded; this biases toward bugs that are easy to kill.
- Tasks are first-party and small (under 120 lines). Real-repo, multi-file tasks with cargo-mutants mutants are the next slice.
- Installed ripr is 0.10.0, not main. Reps per cell are 1–2 per task; no significance test is claimed.
- Optional use: when ripr was only offered, agents ran it 0 of 3 times (#13050); the arms above force the first call.

## Run

```sh
cd benchmarks/agentic/cohort/harness
python3 validate.py                      # check every mutant is killable by the reference tests (small tasks)
python3 sweep.py all A0-none,A1-diagnostics,A2-packet-loop haiku 1 3 out.jsonl 4
python3 agg.py out.jsonl
```

Needs `claude` on PATH and `ripr`. Runs create crates under `/tmp/cohort-work`.
