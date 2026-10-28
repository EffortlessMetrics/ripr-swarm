# RIPR-SPEC-0365: Capability manifest cell identity join

Status: accepted

Owner: xtask / repository policy

Linked issue: #10065, follow-up 1; authority boundary: #9294.

## Problem

Product capabilities in `metrics/capabilities.toml` and semantic support cells
in `CAPABILITY_CELLS` are separate authorities. Without an explicit identity
join, a manifest can name a nonexistent cell or omit a registered cell without
the repository policy check noticing. Capability names are not an identity
mapping and must not become a heuristic for semantic support.

## Behavior

Each manifest entry may declare one `cells = [...]` array of string IDs.
`cargo xtask check-capabilities` first preserves the existing manifest and
capability-matrix validation. Only a violation-free manifest proceeds to the
registry join. Repeated `cells` fields and malformed arrays fail validation.

The join obtains the default-feature registry through the bounded subprocess
`cargo run --locked -p ripr --quiet -- capabilities --json`. Process failure,
timeout or invalid JSON cannot produce a pass. The producer envelope must have
kind `ripr_language_capabilities`, schema version `0.1`, digest scope
`whole_table`, and an explicitly present null `filtered_by`. The `cells` array
must be nonempty; missing or blank IDs and duplicate registry IDs fail.

For each manifest claim, the exact string must be a registered ID. For each
registered ID, at least one manifest entry must claim it. There is no trimming,
substring match, alias, axis fallback or inferred relation to a capability name.
The producer derives IDs as `language/framework/mode/family`; the join consumes
the producer's IDs and does not independently decide those semantic axes.

Omission and `cells = []` both claim no cells. They do not disable the global
registered-minus-claimed completeness check. Repeating an ID within one entry
fails; sharing a registered ID between distinct entries is valid. Redistributing
valid IDs between entries is not semantic ownership validation.

Product status remains owned by the manifest. Cell support state, tier ceiling,
limits and proof references remain owned by the runtime registry. Build
availability is separate from both. Membership neither promotes support nor
implies that an adapter is compiled or enabled; an absent cell remains
`not_proven` under the registry contract.

## Required Evidence

- Nonempty positive joins for both inline and multiline arrays.
- Missing, altered-axis and near-match identities must fail exact membership;
  a newly registered unclaimed cell must fail completeness.
- Omitted and empty arrays must retain completeness, with shared membership
  accepted and repeated fields or per-entry claims rejected.
- Malformed, duplicate, empty or filtered registry input must fail closed.
- A product-status/semantic-support mismatch must not become a join failure or
  support promotion.
- Run the real `check-capabilities` route against the compiled producer and
  retain its native outcome separately from synthetic unit-fixture evidence.
  Producer-envelope changes must update the guard and fixture/removal controls
  together. Unit fixtures alone do not establish subprocess execution coverage.

## Non-Goals

- Migrating semantic support cells into the product-capability manifest.
- Validating semantic ownership from capability names, or prohibiting shared
  membership of a valid ID.
- Changing support tiers, support states, adapter availability or analysis.
- Inventorying non-default feature builds or qualifying every platform.
- Treating saved JSON, source inspection or a passing unit test as actual
  compiled-producer or hosted qualification evidence.

## Acceptance Examples

1. Given two registered IDs and exact inline or multiline claims for both, the
   join passes.
2. Given a claim whose language, framework, mode or family differs from its
   registered identity, the claim is missing and the original cell is unclaimed.
   A substring, added whitespace or empty string cannot satisfy the identity.
3. Given one additional registered cell without a claim, completeness fails.
4. Given registered cells and only omitted or empty claim arrays, every cell
   remains unclaimed. An unrelated entry without `cells` is valid when other
   entries already cover the registry.
5. Given a shared valid ID in two entries, the join passes. Repeating the ID
   within one entry or repeating the `cells` field fails.
6. Given a filtered, malformed, empty or duplicate-ID registry, the join fails.
7. Given a product entry marked stable that claims a registered Perl cell with
   `not_proven` support and unavailable adapter, identity membership can pass
   without changing or equating those separate states.

## Test Mapping

All eight automated tests are in `xtask/src/policy/capability_cells/tests.rs`:

- Example 1: `explicit_inline_and_multiline_cells_parse_and_join`.
- Example 2: `near_identity_cannot_claim_a_registered_cell`.
- Example 3: `a_new_registry_cell_needs_an_explicit_claim`.
- Example 4: `optional_per_entry_does_not_disable_global_completeness`.
- Example 5: `duplicate_claim_in_one_entry_is_rejected_but_shared_membership_is_valid`
  and `malformed_cell_fields_fail_parsing`.
- Example 6: `incomplete_or_malformed_registry_cannot_pass_the_join`.
- Example 7: `product_status_and_semantic_support_remain_separate`.

The tests call the actual manifest parser and join helper with synthetic
registries. The real producer/subprocess route is checked separately by
`cargo xtask check-capabilities`; retained CLI controls keep their own source
and input identities. The traceability fragment registers the exact test
symbols, but a traceability-check pass does not prove those symbols ran.

## Implementation Mapping

- `xtask/src/types.rs::Capability` owns the optional claim array.
- `xtask/src/main.rs::assign_capability_array` parses it and rejects repeated
  fields; `check_capabilities` retains the earlier-validation guard.
- `xtask/src/policy/capability_cells.rs::check_capability_cells` owns the bounded
  producer invocation; `cell_join_violations` owns envelope and exact-set checks.
- `crates/ripr/src/domain/language_capability.rs` owns the semantic table and
  identity construction; `crates/ripr/src/output/language_capabilities.rs`
  owns its JSON envelope. This join does not replace those owners.
- `metrics/capabilities.toml` supplies explicit claims;
  `docs/CAPABILITY_MATRIX.md` and `docs/CI.md` describe the separate authorities
  and policy route.

## Metrics

- `unit_test_pass_rate` for the eight named discriminating tests, preserving
  executed, failed and ignored counts.
- Exact registered and claimed identity counts in retained compiled-registry
  checks. Counts alone do not establish identity equality or support truth.
