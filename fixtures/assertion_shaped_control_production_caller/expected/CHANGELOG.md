# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0133: assertion-shaped owner guidance fixtures (new fixture)

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
#2567: default human render no longer prints a 'Hidden: 0 lower-priority finding(s) omitted' block when nothing was omitted; the format pointers now sit under a 'More:' heading. Formatting-only drift; no evidence, class, or JSON change.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2598: default human output now exposes bounded explain and context follow-up commands for the selected finding.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2659: finding navigation commands now preserve the analyzed root, diff or artifact scope and shell-safe identity.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: publish typed analysis outcome in human and JSON output.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: align fixture outputs with the typed incomplete-outcome and unquoted human outcome contract.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0023: classification hint added to digest (#2614)

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0151: rebless check JSON for the additive source_currentness field; classifications, stages, confidence, counts, and recorded coordinates remain unchanged.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`

## Pending

Reason:
RIPR-SPEC-0159: the validate_score -> check_score_invariants chain is genuinely resolvable under the typed transfer (unique callee, single caller and site, parameter binding), so the helper-owned probe legitimately relates its test; the SPEC-0133 assertion-shaped guidance story is unchanged

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0162: the propagation_unknown why-hint stops asserting the propagation the class marks unknown, and unknown-class limitation prose renders under the Analyzer limit label (human-only; the shared decision-layer text is unchanged)

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (3)

Reason:
RIPR-SPEC-0001: reach is weak when every related test is linked only by file or name-token proximity (weak_token_substring); class unchanged, reach stage yes->weak

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (4)

Reason:
RIPR-SPEC-0133: test-name token relations now need whole words and a specific token, so the helper-chain relation (reach yes) is no longer shadowed by a weak name match; class unchanged

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (5)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (6)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (7)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (8)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — assertion_shaped_control_production_caller (9)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (11)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (12)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — assertion_shaped_control_production_caller (13)

Reason:
RIPR-SPEC-0001 unresolved boundary input (#6674): the operand splitter cannot cut 'let clamped = if value >= 0' cleanly, so the boundary is unresolved (infection_unknown) instead of a garbled missing discriminator; no owner_shape line, standard guidance for the emitted class

Command:
`cargo xtask goldens bless assertion_shaped_control_production_caller --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
