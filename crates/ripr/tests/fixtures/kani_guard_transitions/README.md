# Fixed-guard Kani boundary inputs

Owner: #12598, under #10746 and #10745. Consumers being built: #12596,
#12512 / #10687, #12597 and #10688. Assurance vocabulary: RIPR-SPEC-0135.

These are **authored development inputs**, not retained native executions,
held-out cases, or evidence that RIPR's semantic producer is implemented.
Ordinary Rust tests execute the fixture implementations; `cfg(kani)` proof
bodies are excluded from that build. No native Kani outcome is committed here.

## Accepted fixture requirement

For every `(n, max)` in `u8 x u8`, `fits` must report whether used capacity
`n` is strictly less than capacity `max`. Equality must be false, including
`(0, 0)` and `(255, 255)`. This requirement is authored independently of an
analyzer output. It is not an inference about an arbitrary user's intent.

The `<` to `<=` change differs on exactly 256 equality inputs in a domain of
65,536 pairs. The assertion is byte-identical across the strong correct and
wrong implementations. That makes the wrong variant a meaningful semantic
challenge rather than a deliberately false specification of unchanged code.

## Controlled variants

| Source | Implementation | Functional assertion/domain | Purpose |
| --- | --- | --- | --- |
| `strong.rs` | strict `<` | direct equality; full u8 domain | Correct baseline; exact source for restoration |
| `wrong.rs` | inclusive `<=` | same complete guard bytes | Known-wrong implementation under frozen guard |
| `weak.rs` | strict `<` | discarded return; `assert!(true)` | Guard-only loss of the explicit functional obligation |
| `weak_wrong.rs` | inclusive `<=` | same weak guard bytes | Wrong implementation not constrained by that assertion |
| `narrow_wrong.rs` | inclusive `<=` | same equality, preceding `assume(n < max)` | Nonempty domain excludes all equality faults |
| `narrow_correct.rs` | strict `<` | identical partial-domain guard | Legitimate partial-spec control, not an automatic defect |
| `false_domain.rs` | strict `<` | preceding `assume(false)` | Empty-domain source control, not a native status claim |

A discarded result can still participate in meaningful safety checking. These
fixtures concern the explicit return-value obligation only. Partial contracts
are legitimate; narrowing is a regression only relative to a still-applicable
stronger requirement. No cover outcome is inferred: these guards contain no
`cover!`. Native SUCCESS, UNREACHABLE and other states must be observed, not
manufactured from this table.

## Ordinary qualification

From repository root:

```sh
cargo test --locked -p ripr --test kani_guard_transition_inputs
```

The test target has no non-standard-library imports. It can also be compiled
as a focused standalone Rust test executable, without building the analyzer:

```sh
rustc --edition=2024 --test --check-cfg 'cfg(kani)' \
  crates/ripr/tests/kani_guard_transition_inputs.rs \
  -o /tmp/ripr-kani-guard-transition-inputs
/tmp/ripr-kani-guard-transition-inputs
```

Choose a task-owned output location on the actual host; `/tmp` is the Unix
example, not a Windows path. Both commands are qualification instructions,
not a claim they were executed when these files were authored.

The five tests pin the complete baseline text, exact one-dimensional changes,
whole-guard and implementation byte equality, all 65,536 inputs, the 256
meaningful faults, the 32,640-input partial domain, and exact source restoration.
The same `.rs` files supply compile-time modules and source-text inputs. A
source-only comparison does not execute a proof body; the ordinary behavioral
checks do not invoke a solver or the production RIPR analyzer.

## Native and installed continuation

Materialize one variant at a time as `src/lib.rs` of a task-owned isolated
Cargo library, preserving all bytes. Keep the package/target/tool configuration
identical across the baseline/mutant pair; select the exact ordinary
`proofs::pins_boundary` harness through a pinned supported Kani version.
The standalone fixture functions need no dependency downloads themselves.

Use the existing authorized external execution route, a fresh result path for
each run and bounded cleanup. Retain source/configuration/selection identity,
actual exit and native export/log bytes. Interrupted runs must not reuse a
prior apparently complete JSON export. Compilation errors, missing harnesses,
timeouts and unsupported features are not behavioral fault detection.

For the fixed strong guard, require a genuine native failure on the wrong
implementation's intended assertion. Separately retain weak/narrowed/empty
observations without treating a passing verifier as accepted intent. Restore
`strong.rs` byte-for-byte and obtain applicable fresh evidence; byte restoration
alone does not demonstrate a new verification run.

After #12596/#12512/#12597 are admitted, use one identified installed RIPR
artifact on these same transitions. Assert actual affected owner/guard subjects,
source currentness and separate native/static projection through public routes.
Unknown semantics remain visible. Do not replace these independent expectations
with analyzer-generated golden answers. Add wrong-owner, configuration, helper,
relocation and stale-source controls at that production boundary.

The native and installed stages remain open in #12598. Fixture success alone
cannot complete #10746 or #10745, promote a support tier, authorize a merge,
reduce verification selection or count as real-repository personal adoption.

## Authored source commitments (SHA-256)

These hashes identify input bytes only; they authenticate no execution.

| Source | SHA-256 |
| --- | --- |
| `strong.rs` | `64fa5fd8248c1043ea6247caab18e00218793ac5cac608fc52f41ebf9dd26824` |
| `wrong.rs` | `270cfe020ecc7edceff2ba52b8ca7cf456e7e78e111d7f2a8a9f409fc7730348` |
| `weak.rs` | `42513d2b4e93e1d9fe850cf4882ed1eda7fa8622030283e0c73351751d46597e` |
| `weak_wrong.rs` | `bff959fbb300e0274c6007388f847c17e169960ae69e2ffc294e5e8b8136bfb7` |
| `narrow_wrong.rs` | `467d2a82032024b9daa90c6ccc4b9db766acf071aaf60711c7232b454c93c81c` |
| `narrow_correct.rs` | `399e47f05a23fc8066e45e448e52dd528d29f4ff9416c97ddb5ab28a0cb163de` |
| `false_domain.rs` | `f648c9f81cfb7148c1100c771e84f80113fa7cd3b3d481bb7bf80655d00a0701` |
