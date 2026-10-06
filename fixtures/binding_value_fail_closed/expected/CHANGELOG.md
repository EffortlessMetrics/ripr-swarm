# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0158: new fixture pinning the bounded value-transfer behavior (family matrix observes exact boundaries; unsupported chains fail closed by name)

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0158: review round 2 — operand resolution hoisted per probe (provenance text unchanged in content but regenerated), the starts_with hunk made a real behavior change, and quote-aware splitting/char escapes/dedup refinements

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0158: scoped re-bless for this fixture only - exact operands compare canonical renderings directly and the literal-case provenance renders explicitly

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (3)

Reason:
RIPR-SPEC-0122: discriminator evidence line no longer says yes on findings that are not exposed

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (4)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (5)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (6)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/human.txt`

## Pending — binding_value_fail_closed (7)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — binding_value_fail_closed (8)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (9)

Reason:
RIPR-SPEC-0197 bool-owner pin: assert!(owner(..)) on a bool owner pins its whole result, so its strength relative to the tail predicate is strong; kind stays relational_check and every class is unchanged

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (11)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — binding_value_fail_closed (12)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless binding_value_fail_closed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
