# ripr A/B harness — RUNBOOK (encoded, validity-fixed)

Measures whether ripr context helps an LLM agent write a test that
**discriminates** a behavior change: a test that **passes** when the change is
reverted to the original behavior and **fails** on the changed behavior.

Three scenario crates × three context variants. One solver run = one agent
attempting one crate clone under one variant. The executor (this agent or a
solver-executor) materializes a run-owned crate clone, fills the prompt
template, runs the solver, runs `eval.sh` (the sole discriminator oracle),
then records the run with `record-run.sh`.

This encoding applies the measurement-validity fixes from issue #13113 over
the Nov 2-4 seed-study harness: **per-run cloned crates** (never a shared
scenario crate), **toolchain PATH normalization**, **true cargo exit codes**
in the evaluator, **populated structured record fields**, and **atomic
report+summary writes**. Do not weaken any of them to save a step.

## Layout

```text
benchmarks/agent-discrimination/
  README.md                    claims-limited framing and claim boundary
  RUNBOOK.md                   this protocol
  eval.sh                      the evaluator (sole discriminator oracle)
  record-run.sh                run-record writer (validates fields, atomic
                               report + derived summary.csv)
  make-scenario-crates.sh      materializes the 3 scenario crates as fresh
                               2-commit git repos under a destination you pass
  build-contexts.sh            regenerates {vanilla,ripr-context,tool-note}.txt
                               for one run-owned clone
  scenarios/<scenario>/        original/ and changed/ content trees + scenario.json
  contexts/<scenario>/         RECORDED seed-study context captures (evidence;
                               regenerate per run with build-contexts.sh)
  results/seed-study/          RECORDED first calibration (see README.md)
  scratch/                     gitignored; not used by the protocol — clones
                               live outside any Cargo workspace checkout
  logs/                        gitignored; eval phase logs
```

Each scenario crate is a git repo with EXACTLY two commits:
commit 1 = ORIGINAL behavior, commit 2 = the CHANGE under study.
`make-scenario-crates.sh` pins the commit identity (author
`agent-bench <agent-bench@local>` and timestamps) and **fails closed** unless
the rebuilt HEADs equal the recorded seed-study SHAs:

```text
inserted-guard     86625387aaf5908ada16a691fe11b2e1616f86ad  "Return zero for negative amounts in total_after_fee"
flipped-comparison 006aaf48af8a8e8582056d85ad87d12e9690682d  "Require age over 65 for the senior discount"
silent-return      ed7af52a019f78987958eaf4f0190d99e0a2a8b0  "Return early for cash-on-delivery captures"
```

The crates' existing unit tests (`#[cfg(test)]` in src/lib.rs) and integration
tests (tests/baseline.rs) pass on BOTH commits: they do not discriminate.

## Variants

| variant        | context input                                          |
|----------------|--------------------------------------------------------|
| vanilla        | `vanilla.txt`   (diff HEAD~1..HEAD + full changed src) |
| ripr-context   | `ripr-context.txt` (real ripr check/explain output)    |
| ripr-tool      | `tool-note.txt` (solver runs ripr itself)              |

## Step zero: toolchain PATH (issue #13113)

Every seed-study solver burned a failed round locating cargo. Before any
solver or evaluator step:

1. Resolve the toolchain yourself: `command -v cargo || export PATH="$HOME/.cargo/bin:$PATH"`.
   Record the resolved absolute cargo path (the seed-study host used the
   user profile's cargo bin directory).
2. Put the resolved absolute cargo path in the solver prompt (template below,
   `<CARGO_BIN>`), telling the solver to use that cargo.
3. `eval.sh` and `build-contexts.sh` perform the same normalization at step
   zero and print `TOOLCHAIN: cargo at <path>` in their output.

## Per-run procedure (executor)

Run id format: `<scenario>-<variant>-r<NN>`, e.g. `flipped-comparison-vanilla-r01`.

1. **Materialize a run-owned crate clone** (validity fix for the seed-study
   shared-crate collisions — never reuse or share a materialized crate, and
   never materialize into shared temp space). The scratch root must be
   **outside the ripr checkout and any other Cargo workspace**: cargo
   resolves the nearest ancestor `[workspace]` and rejects the materialized
   package ("not a member of the workspace"), breaking every cargo step —
   both scripts refuse such destinations with exit 2 and name the ancestor
   manifest:

   ```bash
   SCRATCH=<run-namespaced-dir-outside-checkouts>/<run_id>   # one per run
   bash <benchmark-dir>/make-scenario-crates.sh "$SCRATCH/crates"
   ```

   The script exits nonzero if the rebuilt SHAs are not the recorded
   scenario identity above; do not benchmark against a mismatched crate.

2. **Build the run's context files** (paths inside point at this run's clone
   only):

   ```bash
   bash <benchmark-dir>/build-contexts.sh "$SCRATCH/crates/<scenario>" \
        "<RIPR_PATH>" "$SCRATCH/contexts" <scenario> <run_id>
   ```

   `<RIPR_PATH>` = absolute path of the ripr binary used for the run (record
   its `--version`; the seed-study capture was ripr 0.11.0). For the
   `ripr-context` variant, capture the context **before** the solver runs so
   the solver cannot see prior-run output.

3. **Fill the solver prompt** from the template below:
   `<CRATE_PATH>` = `$SCRATCH/crates/<scenario>`,
   `<CARGO_BIN>` = the resolved absolute cargo path from step zero,
   `<CONTEXT_FILE>` = the variant's file from `$SCRATCH/contexts/<scenario>/`.

4. **Run the solver** with the filled prompt. Keep its transcript; observe
   the number of test-production rounds it needed (>= 1).

5. **Evaluate** with the test function name the solver reports:

   ```bash
   bash <benchmark-dir>/eval.sh "$SCRATCH/crates/<scenario>" <TEST_NAME> \
        2>&1 | tee "$SCRATCH/eval-output.txt"
   ```

   Exit code 0 and `DISCRIMINATES: yes` = discriminating test.
   Anything else: keep the verbatim output; the score is `no`.

6. **Record the run** (structured fields are validated; `context_bytes` is
   measured from the actual context file — for `ripr-tool` pass the saved
   solver ripr output file; reports and summary are written atomically):

   ```bash
   bash <benchmark-dir>/record-run.sh \
        --results-dir <benchmark-dir>/results/<study-or-scratch-dir> \
        --run-id <run_id> --scenario <scenario> --variant <variant> \
        --context-file <the variant context file or saved ripr output> \
        --rounds <observed rounds> --test-name <TEST_NAME> \
        --discriminates <yes|no> \
        --eval-output "$SCRATCH/eval-output.txt" \
        --final-test "$SCRATCH/crates/<scenario>/tests/discriminate.rs" \
        --notes "<anything anomalous>"
   ```

   `record-run.sh` rebuilds `summary.csv` from every report in the results
   directory on each write, so the summary cannot lose rows.

7. **Clean up the run**: delete `$SCRATCH` (the clone is run-owned; no reset
   protocol is needed because nothing is shared). Keep the transcript, the
   eval output, and any saved ripr output with the run's records.

## Solver prompt template

```text
You are working in the Rust crate at <CRATE_PATH>.
First read this context file: <CONTEXT_FILE>

A change was made to this crate (HEAD~1 -> HEAD). The context file describes
the change: either the raw diff plus current source, or ripr analyzer output,
or instructions for running the ripr analyzer yourself.

Toolchain: use the cargo binary at <CARGO_BIN> (it is already on this
shell's PATH as `cargo`).

The crate's existing tests pass both before and after the change: they do NOT
discriminate it.

Your task: add ONE new test that discriminates the change.

Requirements:
1. Create a NEW file <CRATE_PATH>/tests/discriminate.rs containing exactly one
   test function. Do not modify anything under src/. Do not modify existing
   tests. Do not commit; leave the file untracked.
2. Discrimination direction (this is how your test is scored): when the change
   is reverted to the ORIGINAL (pre-change) behavior, your test must PASS; on
   the current (changed) behavior, it must FAIL. Pin the original behavior
   with a concrete input at the exact boundary or branch the change affects.
3. Use only the crate's public API and std. No new dependencies.
4. Check your work: run `cargo test <your_test_name>` inside <CRATE_PATH>.
   On the current code the test is EXPECTED to fail with an assertion
   failure. A compile error means the test is wrong; fix it.
5. When done, report:
   - the exact test function name (as cargo prints it),
   - the concrete input value(s) the test uses,
   - the final full source of tests/discriminate.rs.
```

For the `ripr-tool` variant, additionally tell the solver: "You may run the
ripr analyzer as described in the context file; save any ripr output you use
to `<SCRATCH>/contexts/ripr-run-output-<run_id>.txt`."

## Report fields (written by record-run.sh)

```text
run_id, scenario, variant, context_bytes, rounds  -> summary.csv columns
test_name, final_test_source, eval_output, discriminates, notes -> report body
```

`record-run.sh` fails closed on: empty structured fields, a `context_bytes`
that measures 0, a run-id that does not match `<scenario>-<variant>-rNN`,
non-positive `rounds`, a final test that does not define the named function,
or a `--discriminates` value that contradicts the evaluator's own
`DISCRIMINATES:` line. Do not bypass it with hand-written reports.

## Scoring

`DISCRIMINATES: yes` iff BOTH hold, as executed by `eval.sh`:
1. with the crate's changed files flipped to HEAD~1 (original), the named test
   passes (cargo exit 0 AND an exact `test <name> ... ok` line);
2. restored to HEAD (changed), the same test fails (cargo exit nonzero AND an
   exact `test <name> ... FAILED` line).

A test that passes on both, fails to compile, or fails for a setup reason
scores `no`. The full phase logs are kept in `logs/<crate>-eval-*.log`.

## ripr invocation (per run-owned clone)

```bash
<RIPR_PATH> check --root <CRATE_PATH> --base HEAD~1 --format agent
<RIPR_PATH> explain --root <CRATE_PATH> --base "HEAD~1" --committed <FINDING_ID>
```

`<FINDING_ID>` is the `id=...` value on a finding line of the `check` output.
The `check` output also contains a `next=ripr ...` line nominating the exact
follow-up command — run that (substituting the absolute binary path for
`ripr`). `build-contexts.sh` captures both automatically.
