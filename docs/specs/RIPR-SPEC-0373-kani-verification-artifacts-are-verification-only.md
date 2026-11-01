# RIPR-SPEC-0373: Kani proof harnesses, `cfg(kani)` modules and `cfg_attr(kani, ..)` contracts are verification-only on either side of a diff

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

- No tier change. Rust static analysis stays advisory; a diff that only adds,
  edits or deletes Kani verification artifacts gets the existing named,
  non-actionable `verification_only_change` disclosure instead of production
  probes. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/`, `.ripr/traceability.d/`,
  `docs/spec-index.d/` and the four verdict corpora's `spec-scopes/`.
- No new output field, limitation kind or cache shape: the recognised lines
  flow into the `verification_only_change` limitation of
  [RIPR-SPEC-0368](RIPR-SPEC-0368-verification-erased-edits-disclose-not-probe.md).

## Problem

[RIPR-SPEC-0367](RIPR-SPEC-0367-proof-harness-is-not-a-test-or-production-seam.md)
gives a Kani harness its own source role, but the role is assigned from the
new-side index. A harness (or a whole `#[cfg(kani)] mod proofs`) that a diff
only *deletes* has no new-side function, so its removed lines were looked up by
`new_side_line` against whatever production function sits there and were probed
as deleted production behavior. The same commit's added `cfg_attr(kani,
kani::requires(..), ..)` contracts were not recognised (SPEC-0368 matched only
bare `#[requires]`-style attributes).

Measured on zerocopy `855c2378` ("Use function contracts instead of proofs",
no production code changed): ripr at `912d98a5c1` still emitted 29 findings
(5 `weakly_exposed`, 2 `infection_unknown`, 22 `static_unknown`), all deleted
proof lines pooled at one new-side line.

## Behavior

Recognition is lexical on comment- and string-masked source and is computed
from the text of each side, so it does not depend on a new-side index. These
are **Kani verification artifacts**, and their lines are verification-only
(SPEC-0368 semantics: no probe, counted into `verification_only_change`):

- a function carrying an exact proof-harness attribute (`kani::proof` or
  `kani::proof_for_contract`, written directly; the same path set as
  SPEC-0367. A `cfg_attr`-introduced harness gates only the attribute, so the
  body stays ordinary code), from the first attribute of its attribute group
  through the item's closing `}`;
- an item-level `#[cfg(kani)]` (exact predicate) on a `mod`, from the attribute
  group through the module's closing `}`, including everything inside it;
- a `#[cfg_attr(kani, ..)]` whose every introduced attribute is a Kani
  contract (`kani::requires`, `ensures`, `modifies`, `decreases`), with its
  multi-line span.

Fail closed: `cfg(not(kani))`, `cfg(any(kani, ..))`, a `cfg(kani)`-gated
function or impl, a `cfg_attr`-introduced harness, a `cfg_attr` introducing anything else, another crate's
`proof` path, and an item sharing its closing line with other code are not
matched and keep prior behavior.

A diff whose changed Rust lines are all such artifacts therefore seeds no
probe, yields no finding and discloses `verification_only_change`
(`no_behavioral_candidates` when nothing else changed). A mixed diff keeps the
findings of its runtime lines.

## Required Evidence

- The toy conversion diff (contracts and nested `proof_for_contract` harness
  added, a `#[cfg(kani)] mod proofs` deleted) reports zero findings and one
  `verification_only_change` limitation; before this change it reported
  findings on the deleted lines.
- A deleted `#[kani::proof]` harness in an ungated module is disclosed the
  same way.
- Negative controls (a deleted ungated module without a harness attribute,
  `cfg(not(kani))`, a production edit beside a deleted harness) keep their
  findings and add no limitation.

## Known Limits

- Macro-invoked harnesses (`kani::proof` produced by a macro) are not
  recognised lexically and keep prior behavior.
- `cfg(kani)` on a function, impl or `use` is not treated as verification-only.
- ripr does not run Kani or evaluate any proof; the disclosure is not a
  coverage or sufficiency claim.

## Non-Goals

- No verifier execution, no claim a harness passes, and no credit anywhere: a
  harness line is never an oracle or a discriminator.
- No new verdict, no change to the actionability flip.

## Acceptance Examples

1. zerocopy `855c2378` shape in a toy repo: `+#[cfg_attr(kani,
   kani::requires(align > 0), kani::ensures(..))]` on `round_down`, a nested
   `+#[cfg(kani)] #[kani::proof_for_contract(round_down)] fn proof() { .. }`,
   and `-#[cfg(kani)] mod proofs { #[kani::proof] fn prove_round_down() { .. } }`
   alone: zero probes and findings, limitation `verification_only_change`
   (plus SPEC-0367's `proof_harness_not_evaluated` for the added harness).
2. The same deletion with `n / align` changed to `n / align * align` in the
   production body: one finding on the production line, plus the limitation.
3. Deleting `#[cfg(not(kani))] mod m { #[test] fn t() {..} }`: still probed.

## Test Mapping

See `.ripr/traceability.d/kani-verification-artifacts-0373.toml`.

## Implementation Mapping

- `crates/ripr/src/analysis/probes/verification_erased.rs`: recognition
  (`kani_item_extents`, `is_kani_contract_cfg_attr`), reused by
  `probes/diff.rs` for both added and removed lines and by
  `language/rust/mod.rs` for the aggregate disclosure.

## Metrics

- `unit_test_pass_rate`.

## Known limit

A file the diff deletes outright is not in the new-side index, so `old_source`
returns `None` and the removed side is not classified here. That case seeds no
probe at all: the diff parser counts deleted files and the run discloses
"deletion-only diffs produce no probes" instead (measured with a diff that
deletes a harness-only file: 0 probes, 0 findings). What it lacks is the
`verification_only_change` limitation kind for that case. Tracked in #12013.
