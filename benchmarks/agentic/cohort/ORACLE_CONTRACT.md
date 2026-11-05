# Runtime quality behind cohort efficiency (#12902, #12912)

A token-efficiency claim must preserve useful test outcomes. A nonzero Cargo exit
alone does not show that a test caught a wrong implementation. This contract owns
the independent native quality measurements consumed by the cohort; it does not
change RIPR's static classifications or run models in ordinary CI.

## One execution owner

`workspace.py` builds task crates and delegates execution to `scoring.py`.
`lib.py` keeps catalog loading and the existing public harness imports.
`run_one.py`, reference qualification in `oracle.py`, and the report's verified
quality checks reuse that owner. There is no second success rule in a renderer.

`score_protocol` is `cargo-libtest-pass-fail-pass-v1`. Its scope is small trusted
Rust tasks using stock libtest, the `cargo test --tests` target selection and the
frozen task's declared source/configuration. Doctests, custom harnesses, hostile
submitted code, arbitrary repository graphs and language-wide mutation accuracy
are not qualified by this protocol.

## Fresh grading, fixed submitted tests

The evaluator rebuilds the task from its own inputs in a unique temporary tree.
It compares production files, Cargo.toml, build.rs, .cargo and declared auxiliary
files with the agent checkout; a committed production edit cannot redefine the
pristine implementation. It transfers only regular files under tests/, not the
agent's Cargo configuration, build output, Git state or executable environment.
Symlinks/special files and transfers above 1,000 files or 10 MiB are refused with
a specific cause. Removing all test files cannot inherit the task's weak tests.

The exact transferred tests are archived before execution. Task, production and
test digests are retained; `final_tests_sha256` binds the collection archive.
During grading, input movement invalidates the observation rather than silently
continuing against a changed subject. The original agent checkout is not mutated.
This is reproducible evaluator isolation, **not an operating-system sandbox**.
Only trusted, bounded tasks/tests may be executed by this opt-in harness.

## Native phase observations

Each execution first runs an offline compile-only Cargo test phase with compiler
JSON messages. Only a successful compile proceeds to stock libtest execution.
Both phases share the configured timeout. Raw stdout/stderr byte hashes, sizes,
bounded tails, phase, exit, elapsed time and test counts are retained.

A nonempty complete libtest summary and matching native exit establish `passed`
or `test_failed`. Otherwise the observation remains one of `compile_error`,
`build_error`, `timeout`, `cleanup_error`, `process_error`, `incomplete`,
`zero_tests` or `input_changed`. Actual compiler error messages distinguish a
compiler rejection from dependency resolution, tool failure or a build timeout.
A successful compile is not a test pass; ignored-only suites are not execution.

Owned process trees are terminated on timeout, not just the Cargo parent. The
POSIX process group and Windows task-tree routes are distinct. A cleanup failure
is visible and cannot become a caught mutant. Raw byte counts are not changed by
decoding replacement characters for the bounded diagnostic tails.

## Pristine / mutant / pristine controls

The initial pristine tests must pass with nonzero execution. For every declared
mutant, preserve its index and mutation digest, run it, restore production bytes,
and run pristine again with the exact transferred tests. A catch requires an
actual mutant test failure and a restored pass matching the initial suite/test
counts. A survivor requires a complete mutant pass and the matching restoration.

Timeouts, build/compile/process failures, zero/partial test output, input changes
and failed restoration never count as catches. A failed restoration stops later
execution and records the remaining intended mutants as not run. Malformed,
no-op and duplicate mutations remain explicit unresolved rows in a frozen task;
they do not shrink its denominator or improve its apparent score.

`quality_status: complete` requires a nonempty, fully accounted mutant population
with only caught or survived results. Collection retains the entire score object.
The report independently checks the internal outcome/exit/count/identity joins;
a summary label or a hash alone cannot supply missing execution evidence.
These consistency checks do not authenticate external or fabricated receipts.

## Reference qualification is distinct from scoring a frozen task

Qualification deliberately chooses a future mutant set. It may retain a candidate
with pass/fail/pass evidence, discard an executed survivor, or explicitly exclude
a compiler-rejected candidate with recorded compiler errors and valid restoration.
All candidate outcomes are retained in a qualification JSON alongside the selected
list. Timeouts, missing execution, tooling failures and malformed mutation data
make qualification incomplete, preserving the previous selected list for repair.
An empty selected list is not a valid reference oracle.

Once the task is frozen, every selected mutant remains in its intended denominator;
no arm-specific exclusions are allowed. The selected list must be bound to every
arm by the common task identity. Model-generated reference suites can still bias
the list toward easily detected defects; this implementation does not remove that
bias or establish general test adequacy.

## Proof routes and honest limits

`selftest.py` exercises accounting, collection boundaries, real Git reconstruction,
controlled process failure paths and native child-tree cleanup without a model.
Five named Cargo controls cover an exact boundary test, a smoke test, zero tests,
a compiler-rejected mutant and a process-exit mutant. Local missing Cargo is an
explicit skip. `--require-native`, invoked by the Rust integration test, requires
all five identities and rejects any skip or missing tool.

```sh
python3 -I benchmarks/agentic/cohort/harness/selftest.py
cargo test -p xtask --test agent_cohort_selftest
```

A passing instrument test is not a new live-model cohort, a validated provider
integration, a second-provider comparison, a public support claim or evidence
that RIPR saves tokens. Historical receipts lacking this protocol remain reported
observations, never silently promoted to verified quality.
