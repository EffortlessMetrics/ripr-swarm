# RIPR-SPEC-0368: Changed contract attributes and verifier-erased ghost items are disclosed, never probed

Status: proposed

Owner: product / analysis

Created: 2026-10-31

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0021](../adr/0021-static-and-executed-assurance-axes.md)

Linked plan:

- #10745

Linked issues:

- #10749

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory; a changed contract or
  ghost line gets a named, non-actionable disclosure instead of silence or a
  test-gap repair start. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/`, `.ripr/traceability.d/`,
  `docs/spec-index.d/` and `fixtures/rust-verdict-corpus/spec-scopes/`.
- Additive output: one new `analysis_outcome` limitation kind,
  `verification_only_change`, that may ride on a complete outcome.

## Problem

Code verified with Kani, Prusti, Creusot or Flux carries text only a verifier
reads.

```rust
#[requires(x > 0)]            // contract: a verifier checks it, a test never does
pub fn f(x: i32) -> i32 { x + 1 }

#[logic]                      // Creusot erases the whole item from a normal build
pub fn spec(x: i32) -> i32 { if x > 3 { x } else { 0 } }
```

Two failures were measured with ripr 0.11.0:

1. A diff that edits only a contract attribute line (`#[requires]`,
   `#[ensures]`, `#[kani::requires]`, `#[variant]`, `#[invariant]`, Flux
   `#[spec]`) is dropped by the changed-line filter (every line starting with
   `#` is ignored). The result is `no_behavioral_candidates` with
   `limitations: []`: the contract change is silent.
2. An edit to a Creusot `#[logic]` function or to `proof_assert!`,
   `snapshot!`, `ghost!` or `pearlite!` text produces probes, some of them
   actionable "Start here ... add a co-located test" findings. No test can
   reach or observe erased code, so that repair signal is wrong, and a wrong
   actionable signal is worse than a missed advisory.

## Behavior

A changed line is **verification-only** when it is one of:

- a contract attribute: `requires`, `ensures`, `modifies` or `decreases`
  (bare, or through `kani::`, `prusti_contracts::`, `creusot_contracts::`,
  `creusot_std::` or `flux_rs::`), or `invariant`, `variant` or `spec`, which
  user code often defines itself and so count when spelled through one of those
  paths or bare in a file that names a verifier crate (`creusot_contracts`,
  `creusot_std`, `prusti_contracts`, `kani`, `flux_rs`) in code, with its
  multi-line span. A file that names the runtime `contracts` crate (its
  attributes expand to `debug_assert!`, which a test can observe) never
  matches bare, and `contracts::` is not a recognised path;
- a Creusot `#[logic]` item (through `creusot_contracts::`/`creusot_std::`, or
  bare in a file that names one of them), any arguments, from the attribute
  through the item's closing `}` or `;`;
- a statement-position invocation of `proof_assert!`, `pearlite!`,
  `prusti_assert!`, `prusti_assert_eq!`, `prusti_assert_ne!`,
  `prusti_assume!` or `body_invariant!`, and of `snapshot!` or `ghost!` in a
  file that names `creusot_contracts` or `creusot_std` in code (comments and
  strings do not count).

Verification-only lines, on either side of the diff:

1. seed no probe, so they can never become a finding, an actionable gap or a
   repair start;
2. are counted into one aggregate typed limitation,
   `verification_only_change`, produced at `probe_generation` (the wire stage
   token; human output renders that stage as "change point detection"), naming the line
   count, the file count and the files (bounded), with the recovery "re-run the
   verifier (for example cargo kani, cargo creusot or cargo prusti) on the
   affected items; ripr does not evaluate contracts, proofs or ghost code";
3. do not make the analysis incomplete: the limitation is a disclosure about the
   shape of the change (like `eol_only_churn`), so a complete outcome may carry
   it, a contract-only diff stays `no_behavioral_candidates`, and the
   incomplete-scope hedges stay off when it is the only limitation.

Text that merely carries a verifier word stays ordinary runtime code and is
probed as before: `#[check(ghost)]` and `#[trusted]` functions, Prusti `#[pure]`
functions (compiled in a normal build), a contract
attribute followed by code on the same line, a ghost macro inside a `let`
initialiser or beside other tokens, and an attribute spelled through any other
crate path.

Kani proof harnesses, `#[cfg(kani)]` modules and `#[cfg_attr(kani, ..)]` contracts
are verification-only too; see [RIPR-SPEC-0373](RIPR-SPEC-0373-kani-verification-artifacts-are-verification-only.md).

## Required Evidence

- A diff of only `#[requires]`/`#[ensures]` edits yields
  `no_behavioral_candidates`, zero probes and one `verification_only_change`
  limitation.
- A diff of only a `#[logic]` body or a `proof_assert!`/`snapshot!`/`ghost!`
  edit yields zero findings and the same limitation, never an actionable gap.
- A mixed diff keeps the runtime-line finding and adds the limitation.
- Negative controls (`#[check(ghost)]`, `#[trusted]`, `#[pure]`, a same-line
  runtime token, a foreign attribute path) keep their ordinary findings and no
  limitation.

## Known Limits

- Matching is lexical on comment- and string-masked source. A `#[logic]` item
  or ghost macro hidden inside another macro's token tree, or produced by a
  macro, is not recognised and keeps its prior behavior.
- A contract attribute sharing its line with code is not recognised; that
  line is dropped by the existing `#` filter exactly as before.
- Verus (`verus!{}`, `spec fn`, `proof fn`) and Flux refinement signatures
  other than `#[spec]` are out of scope until a dialect spec lands.
- Prusti `#[pure]` bodies are runtime code and are deliberately not erased.
- ripr reads no clause text. Whether a changed contract is stronger, weaker or
  equivalent is the verifier's judgment (see #10749 for guard-weakening
  re-evaluation).

## Non-Goals

- No verifier execution, no claim a proof holds, no specification-adequacy
  score, and no new credit anywhere: a contract or ghost line is never an
  oracle or a discriminator.
- No new verdict name and no change to the actionability flip.
- No suppression of any runtime-code probe.

## Acceptance Examples

1. `-#[requires(x > 0)]` / `+#[requires(x > 1)]` alone: `no_behavioral_candidates`,
   no findings, limitation `verification_only_change` with the verifier recovery.
2. `-pub fn spec(..) { if x > 3 .. }` / `+.. x > 4 ..` under `#[logic]` alone:
   no findings, the same limitation; no "add a co-located test" start.
3. `proof_assert!(x > 0)` changed to `proof_assert!(x > 1)` inside a runtime
   function: no probe on that line; the runtime lines of the function keep theirs.
4. `#[check(ghost)] fn f() { a + b }` changed to `a - b`: an ordinary finding and
   no limitation.
5. `#[requires(x > 1)]` edited together with the body `x + 1` to `x + 2`: the body
   probe is reported as usual and the limitation is added.

## Test Mapping

See `.ripr/traceability.d/verification-erased-edits-0368.toml`.

## Implementation Mapping

- `crates/ripr/src/analysis/probes/verification_erased.rs`: recognition.
- `crates/ripr/src/analysis/probes/diff.rs`: probe suppression.
- `crates/ripr/src/analysis/language/rust/mod.rs`: the aggregate limitation.
- `crates/ripr/src/analysis_outcome.rs`: `VerificationOnlyChange` and the
  advisory-disclosure rule shared with `EolOnlyChurn`.

## Metrics

- `unit_test_pass_rate`.

Known limit: the disclosure is produced where changed files are probed, so a
changed file that is absent from the working tree (it already carries
`changed_file_absent_from_worktree`) or that is not a probe-seeding source role
is not scanned for verification-only lines.
