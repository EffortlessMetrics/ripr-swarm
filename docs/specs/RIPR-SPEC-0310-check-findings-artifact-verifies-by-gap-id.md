# RIPR-SPEC-0310: A producer-identified check-findings artifact verifies movement by gap id

Status: proposed

Owner: product / analysis

Created: 2026-10-22

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0020](../adr/0020-repair-artifacts-carry-producer-identity.md)

Linked plan:

- None yet

Linked issues:

- #9103

Linked PRs:

- None yet

Support-tier impact:

- No tier change. TypeScript and Python stay `preview_advisory_only`; this adds
  a verifiable snapshot kind, not a new finding class or authority.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- Additive output format `check-findings-json`; the check JSON schema version
  is unchanged.

## Problem

`agent verify` and `agent receipt` accepted only `repo-exposure-json`, a Rust
whole-repo inventory. A TypeScript repair has a canonical gap id and a
diff-scoped finding but no snapshot a verifier could trust, so its movement
could not be verified without trusting hand-authored JSON.

## Behavior

1. `ripr check --format check-findings-json` renders the complete (never
   findings-bounded) `check --format json` document with a first top-level
   `artifact` envelope: `kind: "check_findings"`,
   `analysis.format: "check-findings-json"`, the matching `analysis.command`,
   and a `content_sha256` commitment over the exact bytes.
2. `agent verify` and `agent receipt` select the validator from the declared
   `artifact.kind`. Both kinds run one shared identity, commitment and
   Git-provenance validator.
3. The findings kind additionally requires `analysis_outcome.analysis_complete`
   to be `true`, rejects `document_complete: false`, requires a non-empty
   classification, file and line for each gap-identified finding, and rejects a
   repeated gap id.
4. The analysis format is part of the input identity, so a findings artifact
   and a repo-exposure artifact are incomparable.

## Required Evidence

- Positive: a TypeScript before/after pair produced by the binary verifies and
  names the gap.
- Negative: a hand-edited artifact fails the commitment; an unenveloped
  `--format json` document is not accepted; a reversed pair is incomparable;
  an incomplete analysis, a bounded document, a duplicate gap id and a
  repo-exposure/findings kind mix are each refused.

## Non-Goals

- A gap-bound `agent repair` start or attempt (later slices of #9103).
- Signing, runtime mutation proof, or Python enablement.

## Acceptance Examples

1. On the installed TypeScript head fixture, `ripr check --base <sha> --format
   check-findings-json` before and after pinning the threshold, then `ripr agent
   verify --json --before ... --after ...`, succeeds and names the
   `gap:typescript:` id.
2. Editing one classification in the after artifact fails with `content
   commitment mismatch`.

## Test Mapping

- `crates/ripr/tests/installed_python_typescript_journeys.rs`:
  `typescript_check_findings_artifacts_verify_by_gap_id_and_reject_forgery`.
- `crates/ripr/src/agent/artifact.rs` tests: `check_findings_gaps_*`,
  `verify_snapshot_dispatch_routes_only_the_declared_findings_kind`.

## Implementation Mapping

- `crates/ripr/src/output/check_findings.rs`: producer.
- `crates/ripr/src/agent/artifact.rs`: `validate_check_findings_artifact`,
  `validate_verify_snapshot`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
