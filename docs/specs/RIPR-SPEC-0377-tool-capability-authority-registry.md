# RIPR-SPEC-0377: Versioned tool-capability authority registry schema and report-only parser

Status: proposed

Owner: xtask / repository policy

Created: 2026-11-02

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- #2349

Linked issues:

- #2355 — schema and report-only parser (this spec)
- #2349 — parent capability-authority registry
- #2348 — program

Linked PRs:

Support-tier impact:

- None. The parser is report-only and does not change product support claims.
  See [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Adds `cargo xtask check-tool-capabilities` (`ci_enforced=false`, not in precommit).
- Adds committed seed registry `.ripr/tool-capabilities.toml`.
- Does not change existing CI lanes, precommit, or blocking enforcement.

## Problem

Capability and claim authority lived in issue prose. Without a versioned
registry, control semantics, lifecycle, destination ownership, and evidence
class can be overloaded into one field or treated as GitHub-mutable truth.
Later inventory (#2356) and blocking enforcement (#2358) need a typed offline
schema first.

## Behavior

`.ripr/tool-capabilities.toml` is the authored registry. `cargo xtask
check-tool-capabilities` parses it with a closed vocabulary and writes
deterministic JSON plus a bounded human report under `target/ripr/reports/`.

The parser:

1. Requires `schema_version = 1` and `kind = "ripr.tool_capabilities"`.
2. Keeps capability ID, claim ID, kind, decision kind, control semantics, and
   migration state as separate fields.
3. Names missing state-required fields precisely (`dual_run_expires` for
   `dual-running`, execution receipts for cutover/retirement).
4. Treats `dual_run_expires` as hard invalidation against an injectable
   `as_of`. The expiry date is the last valid day (`as_of == dual_run_expires`
   still passes; `as_of > dual_run_expires` reports expired), matching
   no-panic and clippy-exception `expires` ledgers. `review_after` /
   `review_by` are warning-only and do not expire a dual run.
5. Requires `control_semantics = "deny"` to name a `test:` or `receipt:`
   negative proof and a `required_claim_id`. Documentation in `rationale_refs`
   is not execution evidence.
6. Rejects metadata-only or report-only data as parity, currentness,
   required-route, cutover, or retirement evidence.
7. Validates destination/consumer issue references as `owner/repo#n` syntax
   only. It does not access the network or GitHub.
8. Fails unknown schema, kind, state, decision kind, and control semantics,
   and duplicate capability or claim IDs.

The command is not wired into precommit or required CI. Existing commands and
CI behavior stay unchanged.

## Required Evidence

- A valid registry, including the committed seed, parses with empty diagnostics
  for an `as_of` on or before any `dual_run_expires`.
- Unknown vocabularies, duplicate IDs, missing state-required fields, deny
  controls without execution proof, metadata-only blocking evidence, and an
  expired dual run are reported.
- Identical `(registry, as_of)` inputs render byte-stable JSON with
  `network_access: "none"`.

## Non-Goals

- Complete command or policy inventory (#2356).
- Generated authority or progress views (#2357).
- Live GitHub reconciliation (#2365).
- Policy assurance matrix (#2366).
- Blocking enforcement or CI wiring (#2358).
- Cargo-intent ingestion.

## Acceptance Examples

1. Given the committed seed and `--as-of 2026-11-02`, the parser reports pass
   and includes generic, ripr-specific, hybrid, and at least one dual-running
   example.
2. Given `kind = "magic"` or `schema_version = 2`, the parser reports a schema
   diagnostic naming the unknown value.
3. Given two capabilities with the same `id` or `claim_id`, uniqueness fails.
4. Given `state = "dual-running"` without `dual_run_expires`, the diagnostic
   names `dual_run_expires`.
5. Given `control_semantics = "deny"` with only `docs/CLIPPY_POLICY.md` as
   negative proof, deny honesty fails.
6. Given `control_semantics = "metadata-only"` and `state = "cutover-ready"`,
   evidence honesty fails.
7. Given `dual_run_expires = "2026-01-01"` and `--as-of 2026-11-02`, the dual
   run is reported expired. Given `dual_run_expires = "2026-11-02"` and
   `--as-of 2026-11-02`, it still passes; `--as-of 2026-11-03` reports expired.

## Test Mapping

- `xtask/src/tool_capabilities/tests.rs`:
  `valid_seed_registry_parses_without_diagnostics`,
  `unknown_schema_version_fails`,
  `unknown_kind_fails`,
  `unknown_state_fails`,
  `unknown_decision_kind_fails`,
  `unknown_control_semantics_fails`,
  `duplicate_capability_ids_fail`,
  `duplicate_claim_ids_fail`,
  `dual_running_names_missing_dual_run_expires`,
  `deny_without_negative_proof_and_required_claim_fails`,
  `deny_with_docs_path_as_negative_proof_fails`,
  `metadata_only_cannot_satisfy_parity_or_cutover`,
  `expired_dual_run_reported_for_injected_as_of`,
  `same_dual_run_is_valid_on_expiry_date`,
  `dual_run_expires_the_day_after_expiry_date`,
  `output_is_byte_stable_for_identical_inputs`,
  `parser_source_has_no_network_or_github_client`,
  `issue_refs_are_syntax_only`,
  `review_after_is_not_hard_expiry`.

## Implementation Mapping

- `.ripr/tool-capabilities.toml` — seed registry.
- `xtask/src/tool_capabilities.rs` — schema, parser, report, CLI.
- `xtask/src/command.rs` and `xtask/src/dispatch.rs` — report-only command
  registration (`ci_enforced=false`).

## Metrics

- `unit_test_pass_rate`: the xtask tests above are required Rust tests for this
  schema; they do not measure runtime mutation or CI enforcement.
