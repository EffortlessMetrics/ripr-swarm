# Agent cohort: useful outcomes per total effort

Question: do agents given RIPR leave fewer escaped defects, in fewer steps and
with less total effort, than agents without it?

The [scoring/accounting core](CORE_CONTRACT.md) owns native quality, costs and
paired report decisions. This directory adds opt-in agent collection, reference
qualification, frozen tasks and retained historical observations. It does not
claim that RIPR saves tokens. Product static classifications are unchanged.

## Run the instrument before a paid cohort

Python 3.11+ and Git are required; native grading also requires Cargo/Rust.
From the repository root, run the cheap deterministic proof:

```sh
python3 -I benchmarks/agentic/cohort/harness/selftest.py --require-native
cargo test -p xtask --test agent_cohort_selftest
```

These commands launch no model and make no network requests. The strict selftest
requires all six named native Cargo controls and zero skips. Without Cargo,
the non-strict selftest can exercise the other boundaries but reports native
skips explicitly; that is not native qualification.

Validate the authored references without a model:

```sh
cd benchmarks/agentic/cohort/harness
python3 validate.py
```

The default selection contains only tasks with authored references. Explicitly
selecting a task without one, incomplete scoring, zero mutations or an unknown
task returns nonzero. Reference qualification and its selection bias are described
in [the oracle contract](ORACLE_CONTRACT.md).

## Opt-in collection

Collection needs the `claude` executable and an identified `ripr` on PATH, or
`COHORT_RIPR_BIN` pointing to the selected binary. Run only in a disposable,
secret-free environment: the narrow tool grant is not a security sandbox, and
Cargo executes agent-written code. No isolation or redaction is implied.

From the harness directory, the following explicitly consumes provider usage:

```sh
# One task, three arms, repetitions 1 and 2, one concurrent worker.
python3 sweep.py shipping none,check-mandatory,brief-mandatory haiku 1 3 out.jsonl 1
python3 scoreboard.py out out.jsonl
python3 agg.py out.jsonl
```

`scoreboard.py` writes JSON and Markdown; `agg.py` prints the same canonical
report. Neither drops failed attempts or supplies missing costs as zero. The
model alias in this example is not a pinned model snapshot or a recommendation.

`CONFIGS` in `run_one.py` contains none/check/brief/MCP surfaces, optional or
mandatory use and output flags. `COHORT_MAX_TURNS` sets the actual turn cap.
Every sweep generates a fresh cohort ID and each run has a unique directory.
Set `COHORT_ID` only to join files from one deliberately frozen experiment.
Reconcile the intended matrix: the current report counts supplied receipts,
not an independently retained plan of every scheduled attempt.

## Task and evidence boundaries

The first-party Rust tasks contain a base/change pair, thin tests, selected hidden
mutations and authored references. Larger tasks use filtered mechanical mutations.
Optional real-code tasks load only the exact crate versions already present in
the local Cargo registry; unavailable sources do not become fabricated tasks.
The frozen population and environment must be checked before launching a cohort.

The evaluator reconstructs source/configuration from the task and grades the
exact archived submitted tests. A catch requires nonempty pristine/mutant/restored
execution controls; compilation, timeout, process, empty-test and incomplete
outcomes remain unresolved. A cheaper failed attempt is not a useful result.
Costs distinguish token components, SDK estimates, agent time and scoring time.
See [collection/report receipts](MEASUREMENT_CONTRACT.md) for those joins and
[the core contract](CORE_CONTRACT.md) for the shared decisions.

## Historical results and remaining acceptance

[Historical observations](HISTORICAL_RESULTS.md) preserve the original numerical
tables and identify their unqualified scorer provenance. Raw receipts, selected
mutation lists and original generated scoreboards remain unchanged. Reading them
through a newer reporter does not establish verified quality or a new experiment.

#12902 and #12912 retain the broader work: frozen model/executable identities,
representative harder tasks, randomized order, a planned-attempt denominator,
a second provider, Python/TypeScript cohorts and scoreboard integration. A new
qualified cohort needs its own retained observations and predeclared quality and
budget criteria. Passing instrument tests alone does not establish effectiveness.
