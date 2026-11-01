# RIPR-SPEC-0367: A proof harness is neither a test nor a production seam; ripr does not evaluate it

Status: proposed

Owner: product / analysis

Created: 2026-10-31

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0021](../adr/0021-static-and-executed-assurance-axes.md)

Linked plan:

- #10745 (program: change-to-guard workflows across formal verification and
  modern test frameworks)

Linked issues:

- #10686, #10749

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This spec adds a source role and a typed limitation; it adds
  no credit and no verdict. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` (via
  `policy/doc-artifacts.d/`) and `.ripr/traceability.d/`.
- Adds the additive `static_limit_kind` value `proof_harness_not_evaluated`
  (`docs/OUTPUT_SCHEMA.md`, `docs/STATIC_LIMITS.md`,
  `policy/output_contracts.txt`). The `proof_harness` function source role is
  internal analysis state; it is not serialized.

## Problem

Rust code verified with Kani carries functions that are not `#[test]` and not
product behavior: `#[kani::proof]` and `#[kani::proof_for_contract(..)]`
harnesses. Today ripr scores them as production source. Measured on real
verified crates, a commit that only rewrites proofs (zerocopy `855c2378`)
reports 31 behavior gaps, a proof-only macro commit (verify-rust-std
`099ad08587a`) reports 20, and a changed callee whose only caller is a harness
is told to "add a co-located test" as if no verification existed.

Crediting the harness instead would be a false-confidence surface: a harness
is universally quantified over `kani::any()`, bounded by unwinding and stub
assumptions ripr cannot read, and its pass is a verifier run, not a static
fact. The honest position is narrower and fail-closed: the harness is a
distinct source role that earns nothing, and the run says it did not evaluate
it. Real verification remains a separate, later authority
([ADR 0021](../adr/0021-static-and-executed-assurance-axes.md)).

## Behavior

### The proof-harness role

A function carrying an exact proof-harness attribute has the source role
`proof_harness`:

- `#[kani::proof]`;
- `#[kani::proof_for_contract(..)]`;
- either path introduced by `#[cfg_attr(<predicate>, ..)]`.

Matching is exact on the written path (leading `::` and whitespace are
ignored). A lookalike such as `#[my::kani::proof]`, an unimported `#[proof]`,
or another `kani::` attribute (`unwind`, `stub`, `requires`) does not match.
The `cfg_attr` predicate is not evaluated: the role records what the function
is, not whether any build compiles it.

A function with `proof_harness` role:

1. registers no executable test and is never added to the test selector
   denominator, and its assertions earn no oracle credit. The proof-harness
   attribute paths are a separate list from the executable-test attribute
   paths and are never added to it;
2. is not a production seam: lines changed inside it seed no production probe;
3. is not a test change for test-edit disclosure.

A function that also carries a test attribute (`#[test]` plus
`#[kani::proof]`) keeps its existing test-role handling; the harness role never
displaces a test. A function in a test-required (`cfg(test)`) region keeps its
test-evidence role.

### Disclosure

- **Edit.** When added lines fall inside a `proof_harness` function, the run
  carries one typed limitation per changed file whose detail names
  `proof_harness_not_evaluated`. The run is partial, never a clean zero-finding
  result. Removed-only lines (a deleted harness) are not reconstructed from the
  old source and stay a named limit below.
- **Owner called by a harness.** A `no_static_path` finding whose owner is
  called by a `proof_harness` function, and for which no test path or other
  reach witness exists, carries `static_limit_kind = proof_harness_not_evaluated`.
  Its next step names the harness and its location, says ripr does not evaluate
  proofs, and offers the verifier run or a test; it does not say "add a
  co-located test" as though no verification existed. The owner is named only
  when exactly one non-evidence function carries its name, so a common name
  cannot point at a harness for different code.
- **No change of verdict.** The class, related tests and RIPR stages are
  unchanged. A traced test path, a macro or transitive reach witness, or any
  existing limitation wins; the harness limitation is the last fallback.

### Claim boundary

Static only. The role and the limitation say a harness exists and is not
evaluated. They do not say it passed, was run, covers the changed line, or is
sufficient, and they never raise a class toward `exposed`. Contract attributes
(`requires`, `ensures`, `modifies`), stubs, `verus!` bodies and other
dialects are out of scope here.

## Required Evidence

- Role table: `#[kani::proof]`, `cfg_attr(kani, kani::proof)` and the lexical
  fallback map to `proof_harness`; a lookalike stays production; a test
  attribute keeps `test_attribute`.
- The attribute helper accepts exactly the proof paths and never defines a
  test.
- CLI controls: an owner called only by a harness is `no_static_path` with the
  typed limit and a proof-aware next step; a harness-only edit yields no
  finding and a run limitation; a lookalike path stays production; a real test
  beside the harness decides the verdict and no proof limit appears.
- One authored verdict-corpus case labels the owner-called-by-harness shape
  against cargo-test mutants.

## Known Limits

- A deleted harness (removed lines only) yields no disclosure; deletion is silent.
- `cfg_attr(feature = "kani", kani::proof)` (a string-literal feature predicate)
  is not recognized and the function stays production.
- Contract attribute edits, `kani::stub`, `cfg(kani)` production arms, Verus,
  Prusti and Creusot items are not recognized by this spec.
- A harness imported under another name (`use kani::proof as p`) is not
  recognized and stays production.
- Name-based owner matching: a harness calling a same-named method of another
  type is not distinguished when the owner name is unique in the index.

## Non-Goals

- Crediting a harness assertion, `kani::any` domain or contract as a
  discriminator.
- Running or importing Kani results.
- New verdict names or any change to the actionability decision.
- A verification-only outcome kind or contract-change probes.

## Acceptance Examples

1. A public function `limit` is changed, no test calls it, and a
   `#[kani::proof]` harness calls it: ripr reports `no_static_path` with
   `proof_harness_not_evaluated` and a next step that names the harness and
   says ripr does not evaluate proofs.
2. Only the body of the `#[kani::proof]` harness is edited: ripr reports no
   production finding and the run carries a `proof_harness_not_evaluated`
   limitation.
3. The same harness written as `#[my::kani::proof]`: the function stays
   production source and its edit is probed as before.
4. A `#[test]` that asserts on `limit` exists beside the harness: the finding
   follows the test, not the harness, and no proof limitation appears.

## Test Mapping

- `crates/ripr/src/analysis/facts/test_styles/tests.rs`:
  `function_role_table_maps_every_producer_path_to_one_typed_role`,
  `proof_harness_attribute_paths_are_exact_and_never_tests`.
- `crates/ripr/tests/proof_harness_limit.rs`:
  `owner_called_by_a_proof_harness_is_limited_not_credited`,
  `harness_only_edit_seeds_no_probe_and_names_the_limitation`,
  `lookalike_attribute_path_is_not_a_proof_harness`,
  `a_test_path_wins_over_the_harness`.
- `crates/ripr/src/domain/limit_guidance.rs`:
  `static_limit_kinds_all_is_a_gap_free_ordinal_sequence`.
- `fixtures/rust-verdict-corpus/cases/proof-harness-calls-owner-no-test.json`.

## Implementation Mapping

- `crates/ripr/src/analysis/facts/test_styles.rs`: the exact proof-harness
  attribute list and the role assignment.
- `crates/ripr/src/analysis/facts/model.rs`: `FunctionSourceRole::ProofHarness`.
- `crates/ripr/src/analysis/language/rust/proof_harness.rs`: the run
  limitation, the harness pointer and the finding disclosure.
- `crates/ripr/src/analysis/language/rust/mod.rs`: wiring after the reach
  witnesses and in the changed-file loop.
- `crates/ripr/src/domain/language.rs`, `crates/ripr/src/domain/limit_guidance.rs`:
  the `proof_harness_not_evaluated` limit kind, gloss and next step.
- `crates/ripr/src/analysis/test_changes.rs`: a harness edit is not a test edit.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
