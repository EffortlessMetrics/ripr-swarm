# Independent real-extractor acceptance oracle for #8600

Complementary evidence for the fuzz/regression campaign's existing claim; this
branch does not implement or take ownership of the typed-output repair. Frozen
production source: `806147b2184d1ff703a883abf68215228a7d2bef`.

The executable oracle is `crates/ripr/tests/call_budget_acceptance.rs`.
It invokes the real CLI through Cargo's absolute binary path, asserts exact source
bytes and parser-produced per-test facts in both production file-fact caches,
then inspects diff JSON and repo exposure JSON. No fabricated `CallFact`, duplicate
production scanner, or test-only production instrumentation is used.

```sh
cargo test --locked -p ripr --test call_budget_acceptance -- --nocapture
RIPR_REPO_EXPOSURE_LATENCY_TRACE=1 RIPR_8600_REQUIRE_TYPED=1 \
  cargo test --locked -p ripr --test call_budget_acceptance -- --nocapture
```

Use `RIPR_8600_EVIDENCE_DIR` to select a new, nonexistent evidence directory.
Otherwise each run retains a unique directory beneath the system temporary root.
The runner preserves its fixtures, caches and raw output; it performs no cleanup.
On the recorded Linux host, commands used `timeout 120s`, `-j 2`,
`PATH=/workspace/.cargo/bin:$PATH`, `CARGO_HOME=/workspace/.cargo`, and
`RUSTUP_HOME=/workspace/.rustup`. The initial locked binary build used `-j 2`;
the first integration-test build took 1m58s. No hosted checks were requested.

## Observed on the frozen source

The characterization run passed: 20 cases, 80 CLI invocations, 9.10s. Both paths
received separate empty caches for the first invocation and populated caches for
the second. All target findings/seams were present, exact source facts matched
between paths, and cold/warm finding/seam outputs matched. The future-mode rerun
also completed all 80 CLI invocations before its acceptance assertion failed;
its traces prove all 20 warm repo invocations hit the cache.

| Control | Exact observation |
| --- | --- |
| 255 / 256 / 257 distinct names | 255 / 256 / 256 retained, on body-only and declaration-sharing lines |
| Owner first versus last | First retained at every boundary; last dropped at 257, including inline declaration case |
| Declaration accounting | `checks` consumes one slot if its declaration shares the budgeted line |
| 2,000 repetitions | Three retained names on inline line: `checks`, `h0`, `weight` |
| Dropped owner reach | Diff still derives `direct_owner_call`; repo uses weaker `same_test_file`; both classify as reachable-unrevealed |
| Complete slice table | Missing empty-input discriminator is present |
| Noise at 255 / 256 / 257 in same slice test | Unknown activation in diff; no missing-input discriminator; no typed budget reason |
| Saturation in unrelated test/line | Complete slice table's missing-input discriminator remains |
| Independent numeric positive | Diff `exposed`, repo `strongly_gripped`, no budget reason |
| Independent slice positive beside saturated table | Diff `infection_unknown`, repo `activation_unknown`, despite exact positive assertion |

Every generated source compiled with `rustc --edition=2024 --test` (18 as metadata, two as native test binaries).
The numeric and slice positive fixture suites also ran successfully as native
Rust tests. Fixture-only mutations `9 * base` to `8 * base` and `== 0` to `!= 0`
each made its positive suite fail (exit 101). These are real test discrimination
witnesses, not claims that ripr runs mutants or establishes mutation adequacy.

## Future acceptance is deliberately red

`RIPR_8600_REQUIRE_TYPED=1` failed (exit 101, 8.89s):

- The exactly-256 and 257-name slice cases lack `rust_call_fact_budget` in the
  finding and repo limitation output. The under-256 twin must not acquire it.
- The independently discriminating slice positive remains downgraded. The
  numeric positive already survives; it must continue to survive.

The future reason's spelling follows the proposal in issue comment 6250351864;
coordinate any accepted spelling change with the campaign owner. The oracle
requires limitation actionability and no repair command for budget-dependent
negative evidence. It permits canonical grouping IDs on a limitation; those IDs
are not an actionable repair gap. Current saturated repo seams carry generic
`activation_boundary_input_unresolved` / `propagation_static_unknown` reasons and
`canonical_item_kind: limitation`, with no repair command. Their raw grip can be
`reachable_unrevealed`, while the diff says `infection_unknown`: retain that
surface difference, rather than calling it output parity or an actionable gap.

The missing reason and slice-positive under-credit are verified limitations.
No new actionable false-promotion soundness defect was established by these
controls. The under-credit example may need a separate campaign decision; this
branch supplies evidence without repairing it.

## Proof limits and retained evidence

The slice reader's eager-table check rejects additional actual calls independently
of saturation. Thus a real saturated negative is production reachability/output
evidence, not proof that removing a particular budget guard changes the result.
Nested declarations also failed to produce a useful under-budget slice witness
in a bounded exploratory check; no claim rests on that exploration. Retain #8678's
synthetic guard-removal controls for guard discrimination, especially the two
loan paths; this oracle does not replace or independently mutation-qualify them.
Exactly 256 proves possible truncation, not that a call was dropped. The 257-name
prefix controls do prove a dropped owner. Source-local positive proof does not
establish whole-program completeness, release acceptance, or merge readiness.

`observations.json` contains exact retained names, source hashes, classifications,
and relevant repo canonical limitation records. `native-validation.json` retains
compiler/test/mutant outcomes and input hashes. `receipt.json` binds the executed
CLI, harness, source revision and results. `artifact-manifest.json` hashes every
member of `raw-evidence.tar.gz`, which contains all final input sources, patches,
raw outputs, JSON caches and logs; binaries are excluded. Early harness premise
failures are preserved and clearly distinguished from product failures.

Changes are only this test and fixture evidence. No changelog is needed: no
production behavior or public contract is changed. No PR, CI request, Ready,
merge or release action is part of this evidence handoff.
