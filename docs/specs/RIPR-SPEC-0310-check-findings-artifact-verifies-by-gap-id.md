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
   classification, file and line for each gap-identified finding, rejects a
   repeated gap id, and rejects a document carrying `analysis_scope` (a
   partitioned diff) or any non-empty `run_limitations`.
4. The analysis format is part of the input identity, so a findings artifact
   and a repo-exposure artifact are incomparable.
5. `agent verify --gap-id <id>` over two findings artifacts requires positive
   closure evidence: the gap is present and not `exposed` before, and present
   and `exposed` after. A gap absent before, already `exposed` before, absent
   after, or not `exposed` after is a named refusal (exit `3`, empty stdout).
   On success the verify document carries an additive `bound_gap` member.
   Over a `repo-exposure-json` snapshot `--gap-id` is an operational error.

## Required Evidence

- Positive: a TypeScript before/after pair produced by the binary verifies and
  names the gap.
- Binary negatives: a hand-edited artifact fails the commitment; an
  unenveloped `--format json` document is not accepted; a reversed pair is
  incomparable.
- Unit negatives (on the findings rules after the shared envelope check): an
  incomplete analysis, a bounded document, a partial scope, run limitations,
  a duplicate gap id and a malformed finding are each refused. A
  repo-exposure/findings kind mix is refused by the shared input identity
  (the analysis format is part of it).

## Known Limits

- Input identity pins mode and base, not the diff. A gap that disappears
  because the diff shrank (production lines reverted, or a base that moved)
  reads as absent in the after artifact. This slice only makes the snapshots
  trustworthy. Plain `agent verify` and `agent receipt` still compare
  whatever moved; only `agent verify --gap-id` (behavior 5) requires the named
  gap to be present in the after artifact as `exposed`. The gap-bound
  `agent repair` start and after phase landed (C2a/C2b, RIPR-SPEC-0311); the
  gap-bound receipt (C2c) remains a later slice of #9103.
- The outcome parser keys findings by a wider set of gap-id spellings than
  `CheckFindingsFinding::gap_id`; the producer is the only source of these
  artifacts, so the two agree on its output.

## Non-Goals

- The journey flips (D, a later slice of #9103); the gap-bound `agent repair`
  start and attempt landed as C2a/C2b.
- Signing, runtime mutation proof, or Python enablement.
- A gap-bound `agent receipt` (the receipt still binds the pair, not a gap).

## Acceptance Examples

1. On the installed TypeScript head fixture, `ripr check --base <sha> --format
   check-findings-json` before and after pinning the threshold, then `ripr agent
   verify --json --before ... --after ...`, succeeds and names the
   `gap:typescript:` id.
2. Editing one classification in the after artifact fails with `content
   commitment mismatch`.
3. The same pair with `--gap-id` set to the printed gap succeeds and reports
   `bound_gap.closed`; an unknown gap id is refused with exit `3` and empty
   stdout.

## Test Mapping

- `crates/ripr/tests/installed_python_typescript_journeys.rs`:
  `typescript_check_findings_artifacts_verify_by_gap_id_and_reject_forgery`.
- `crates/ripr/src/agent/artifact.rs` tests: `check_findings_gaps_*`,
  `gap_closure_*`, `verify_snapshot_dispatch_routes_only_the_declared_findings_kind`.

## Implementation Mapping

- `crates/ripr/src/output/check_findings.rs`: producer.
- `crates/ripr/src/agent/artifact.rs`: `validate_check_findings_artifact`,
  `validate_verify_snapshot`, `evaluate_gap_closure`, `verify_gap_closure`.
- `crates/ripr/src/cli/commands/agent.rs`: `render_agent_verify` (`--gap-id`).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
