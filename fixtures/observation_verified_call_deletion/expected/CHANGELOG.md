# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0094: CallDeletion probe with result_key token_match does NOT emit observation_unverified; initial golden for the anti-over-correction proof fixture

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Re-author with strong whole-object/exact-value persisted-effect observer so the verified-effect control genuinely demonstrates exposed; pairs with reveal.rs effect_observer_confirms fix (#1216)

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
bound default human output to start-here triage; human-full preserves exhaustive evidence

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
changed call heads use one canonical parser expression while verified observation remains exposed

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
#2103: additive changed_files_by_language field and changed_rust_files now Rust-only count

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2598: default human output now exposes bounded explain and context follow-up commands for the selected finding.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2659: finding navigation commands now preserve the analyzed root, diff or artifact scope and shell-safe identity.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0076: raise exposed default severity from info to warning so the strongest finding class is not quieter than weaker classes

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: publish typed analysis outcome in human and JSON output.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: align fixture outputs with the typed incomplete-outcome and unquoted human outcome contract.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0151: rebless check JSON for the additive source_currentness field; classifications, stages, confidence, counts, and recorded coordinates remain unchanged.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_call_deletion (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_call_deletion (3)

Reason:
RIPR-SPEC-0122: exposed findings default to info severity

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_call_deletion (4)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_call_deletion (5)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/human.txt`

## Pending — observation_verified_call_deletion (6)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — observation_verified_call_deletion (7)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_call_deletion (8)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: review fix #5268 - quoted string/char literals are now byte-encoded in the Rust gap discriminator key so distinct predicates never share one identity; golden-drift.json shows zero semantic flips on the re-run

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_call_deletion (9)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
