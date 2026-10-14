# Fixture: rust_async_fn_owner

Spec: RIPR-SPEC-0001

## Given

An `async fn` owner changes its predicate from `>=` to `>`. A real
`#[tokio::test]` awaits `fetch_limit(100, 50)` and asserts the result equals 90.

## When

The analyzer reads the fixture's exact production diff.

## Then

Owner and test discovery are retained. RIPR-SPEC-0197 admits the
`#[tokio::test]` body (#5040): `tokio` is the extern crate in the test's module
under the same shadowing rule as `serial_test`, the attribute takes no
argument that swaps the runtime path or changes what a panic does, and the
macro runs the async body to completion and fails the test when it panics. The
assertion reads the async owner's value through one `.await` (here through
`let result = fetch_limit(100, 50).await;`), so the exact-value oracle is
strong and Observe/Discriminate are Yes/Yes.

Classification is `weakly_exposed`, not `exposed`: input 100/50 is not
equality, so it cannot discriminate `>` from `>=`, and the finding names the
missing `capacity == threshold` discriminator. A companion with input 50/50
reads `exposed` on its own.

`tokio_test_regains_the_exact_oracle_but_not_the_boundary` in
`crates/ripr/tests/owner_pin_execution.rs` holds the positive, the boundary
companion, and controls that get no oracle credit: a rebound `tokio` path, a
custom runtime path (`crate = ..`) and an unpolled async block.

## Executed runtime and removal controls

The actual fixture manifest and Tokio macro/runtime were used, without a
substitute executor: rustc 1.95.0, Tokio 1.53.1 and tokio-macros 2.7.2, resolved
offline. All six subjects compiled, listed one test and executed one test.

| Predicate | Original equality | Equality replaced by `let _ = result` |
|---|---|---|
| Current `>` | pass | pass |
| Previous boundary `>=` | pass | pass |
| Inverted `<=` | fails, 100 vs 90 | pass |

The assertion really executes and detects the inverted predicate. The surviving
previous-boundary mutant confirms the separate missing-boundary limitation.
Runtime observations are the independent witness; the static admission does
not read them.

## Must Not

- Report `exposed` while propagation or the exact boundary is unestablished
- Lose the async owner or the indexed test
- Restore strong credit by trusting a Tokio-like spelling, custom/rebound
  attribute, disabled/raw-CFG context, or unpolled future
