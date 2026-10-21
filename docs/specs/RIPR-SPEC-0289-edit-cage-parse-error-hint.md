# RIPR-SPEC-0289: Edit-cage selected-file parse refusal gives conditional restart guidance

Status: proposed

Owner: agent-loop

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8991 (actionable guidance for the edit-cage unparseable refusal)

Linked PRs:

- #8993

Support-tier impact:

- None. Refusal narration adds no analyzer claim. See
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Registered in `policy/doc-artifacts.toml` and `.ripr/traceability.toml`.
- No schema, manifest, envelope field, command or refusal-decision change.

## Problem

The inline-region observer parses the entire selected file with an analysis
nesting budget. Its `unparseable` reason can mean malformed tests, malformed
production with unchanged tests, or syntactically valid source beyond that
budget. It establishes neither a newly written test nor a test-local syntax
error. Recovery guidance must help users restart without claiming such a cause.

## Behavior

### Rule 1: preserve the observation's uncertainty

An `OutsideInlineTestRegion` violation whose region detail reports
`unparseable` leads recovery narration with:

> the selected file could not be parsed for the inline test region (syntax errors or an analysis limit); if there is a syntax error, fix it, then start a new attempt.

Syntax advice is conditional. Other reasons sharing this violation kind,
including `production_edit`, do not gain the hint.

### Rule 2: retain bounded recovery

The existing refused-path listing, original region-observation detail,
terminal-attempt statement and CRLF-aware undo route follow unchanged. The
hint does not authorize continuing the terminal attempt or changing the cage.

### Rule 3: test the actual consumers

`repair_after_cage_recovery_lines` owns the narration. The after phase emits
it on stderr, records it as sentences in `last_after_refusal.reason`, and serializes it in
the JSON failure envelope's `narration`. `agent status` projects the durable
refusal. Test these consumers through actual after-phase executions rather
than assuming parity from a fabricated verdict alone.

## Required Evidence

- Malformed added test, malformed production-only source with the existing
  inline module byte-identical, and valid over-budget source all produce the
  conditional hint through actual after-phase stderr, JSON and recording.
- Raw Rust parsing accepts the over-budget control without syntax errors.
- An ordinary production edit produces `production_edit`, retains recovery,
  and has no parse hint on those surfaces.
- A valid inserted discriminating test compiles and passes alongside the
  existing fixture test, then completes with a compliant verdict and no
  recorded refusal.
- Unit tests pin the literal first line and unchanged recovery detail.

## Non-Goals

- Changing observer reasons, refusal admission, cage policy, attempt state,
  retry budget, receipt authority, CRLF handling or the undo route.
- Diagnosing whether an observed parse refusal is a test or production error,
  or proving whether a syntax error exists.
- Changing other refusal families or stored violation reasons.

## Acceptance Examples

1. A malformed added test gets conditional syntax advice and a fresh-attempt
   route, without attributing the error to a particular test.
2. Malformed production with unchanged tests gets the same honest hint.
3. Valid production with 300 nested blocks gets analysis-limit guidance even
   though raw Rust parsing reports zero syntax errors.
4. A parseable production edit gets its existing refusal without that hint.
5. A valid inserted boundary test remains admissible and completes normally.

## Test Mapping

- `crates/ripr/src/cli/commands/agent.rs`:
  `unparseable_selected_file_refusal_leads_with_the_plain_hint`,
  `other_inline_region_refusals_do_not_lead_with_the_parse_hint`.
- `crates/ripr/tests/cli_smoke/selected_file_parse_refusal.rs`:
  `selected_file_parse_refusal_reaches_actual_consumers`.

## Implementation Mapping

- `crates/ripr/src/cli/commands/agent.rs`:
  `repair_after_cage_recovery_lines` owns narration.
- `crates/ripr/src/edit_cage/mod.rs`:
  `refusal_is_unparseable_selected_file` detects the producer's reason beside
  `evaluate_edit_cage` without assigning a test-local cause.
- `docs/REPAIR_ATTEMPT.md`: terminal-attempt recovery contract.

## Metrics

- `unit_test_pass_rate`: mapped tests pass. Target: 100 percent.
