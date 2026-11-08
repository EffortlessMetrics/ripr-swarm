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
- #13736

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
- every line of a file that out-of-line `mod` declarations, each guarded by
  exactly `#[cfg(kani)]`, resolve to (`m.rs`, `m/mod.rs` or a literal
  `#[path]`), and of every file below it in the module tree (#13736). The
  declaration-to-file edge is resolved by the module-composition owner
  (`facts/role_composition.rs`), the same pass that makes the child of an
  out-of-line `#[cfg(test)] mod tests;` test-only; exclusive
  `#[cfg(kani)]` / `#[cfg(not(kani))]` alternatives stay production; a file both declared and
  included is Kani-only only when both contexts are. A guard is read for its
  own declaration only (keyed by file, line and declaration text), never for
  a sibling declared on the same line. Ambiguous, cyclic or unresolved edges
  answer "not Kani-only", and so does every module edge whose child could
  have an owner the parser producer does not capture
  (`facts/invisible_owners.rs`): a `mod name;` inside an inline module or a
  macro token tree, in a lexical-fallback or non-UTF-8 file, a
  macro-generated `mod $name;`, a non-literal or `cfg_attr` `#[path]`, or an
  unresolved `include!` in the child's package (except an inline-module
  include of a literal `.rs` file of the same package whose text declares no
  `mod name;`, since that text is already scanned). The check compares, per
  file, the masked `mod name;` and `#[path]` text of the source the index
  holds (HEAD bytes for a dirty path in committed mode) with that file's
  captured declarations, so counts in different files never cancel. Every
  indexed file of the package must be parsed (no lexical fallback). A `.rs`
  file the workspace discovery authority lists for the package but the index
  did not take in (a build script, an example, a package-narrowed file) is
  read through the committed-source snapshot and masked: any `mod name;` in
  it, or a `#[path]` literal naming the child's file, refuses the verdict
  (`build.rs` owning `src/build_commit_record.rs` through `#[path]` is this
  shape), and unreadable or unaccountable text refuses the whole package.
  Entries discovery neither lists nor enters but rustc compiles when named
  (directories with a discovery-ignored name such as `target` or `fixtures`,
  nested package directories, symlinks and other non-regular entries) are
  found by listing the package tree without following links. A `mod <name>`
  token naming such an entry from a file at or above its parent directory
  refuses; so does a `#[path]` literal that is absolute, not a `.rs` file,
  leaves the package, or names such an entry, unless every occurrence is a
  captured declaration whose exact target is a regular file under real
  directories with no `mod <name>;` or `#[path]` of its own (an inert leaf,
  the shape of `tests/kani_guard_transition_inputs.rs` reaching
  `tests/fixtures/kani_guard_transitions/*.rs`).
  Function source roles are not
  changed: a helper there stays a plain function, and the file seeds no probe
  because it is verification source.

Fail closed: `cfg(not(kani))`, `cfg(any(kani, ..))`, several cfg attributes
or a `cfg_attr` on an out-of-line declaration, a module merely *named* like a
proof module (`mod kani_proofs;` without the gate), a `cfg(kani)`-gated
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
- A plain helper `fn` changed inside the file of an out-of-line
  `#[cfg(kani)] mod proofs;` (and inside an inline `#[cfg(kani)] mod`, and in
  a file nested below the out-of-line module) seeds no production probe and is
  disclosed as `verification_only_change`; the same edit under
  `#[cfg(not(kani))]`, `#[cfg(any(kani, feature = "x"))]` or an ungated
  `mod kani_proofs;` keeps its production probe and no disclosure.

## Known Limits

- Macro-invoked harnesses (`kani::proof` produced by a macro) are not
  recognised lexically and keep prior behavior.
- `cfg(kani)` on a function, impl or `use` is not treated as verification-only.
- The out-of-line Kani-only verdict is computed on the new side. A diff that
  adds the `#[cfg(kani)]` gate to a parent declaration while also editing the
  child treats the child's removed lines as verification-only too, although
  that code was production before the change; a diff that removes the gate
  keeps the child's probes. A child file the diff deletes outright has no
  new-side entry and falls back to the whole-file lexical handling above.
- The invisible-owner check covers only the child's own package (nested
  packages excluded): a `#[path]` in another package that names the child,
  and a module declaration produced by a proc macro or a macro defined in
  another crate (no `mod` text in this package), are not seen. `#[path]`
  literals are matched by final file name, not by resolved path, which only
  over-refuses; a symlinked path that names the child under another file name
  is not matched.
- Discovery skips `target`, `fixtures` and other ignored directories and
  symlinked entries, like the run itself; files there are neither indexed nor
  counted. They are refused by name and location instead: any visible
  `mod <name>` that could resolve to one, or `#[path]` naming one, refuses
  unless it is an exactly resolved inert leaf. This over-refuses (a module
  named like an ignored directory beside one), and the tree listing reads
  working-tree directory entries, so in committed mode an extra worktree-only
  entry can add a refusal but never remove one.
- A file outside the index (an editor refresh over open files, Instant mode,
  a build script) is judged by its masked text alone: a declaration it holds
  of the module name refuses even when that declaration would resolve
  elsewhere, which only over-refuses.
- Cost: only packages holding a `#[cfg(kani)]` module edge are examined, once
  per composition pass: one discovery listing of the package, one snapshot
  read and masked pass per discovered file the index does not hold, one masked
  pass over each indexed file that could hide a declaration (`$` with `mod`,
  `cfg_attr` with `path`, `#[path`), and one per file that mentions the
  module or child file name, plus one listing of the package tree (not
  entering ignored, nested-package or symlinked directories). Further edges
  reuse the cached results, but nothing is cached across composition passes:
  each LSP refresh or analysis run that recomposes the index repeats the
  per-package discovery listing, tree listing and masked passes.
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
- `crates/ripr/src/analysis/facts/role_composition.rs`: resolves exact
  `#[cfg(kani)]` out-of-line module edges into `RustIndex::cfg_kani_module_files`
  (#13736); `verification_erased::in_cfg_kani_module` reads it for
  `probes/diff.rs` and the aggregate count.

## Metrics

- `unit_test_pass_rate`.

## Known limit

A file the diff deletes outright is not in the new-side index, so `old_source`
returns `None` and the removed side is not classified here. That case seeds no
probe at all: the diff parser counts deleted files and the run discloses
"deletion-only diffs produce no probes" instead (measured with a diff that
deletes a harness-only file: 0 probes, 0 findings). What it lacks is the
`verification_only_change` limitation kind for that case. Tracked in #12013.
