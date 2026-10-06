# Golden Output Changes

## Pending — guarded_result_match_sibling_variant (1)

Reason:
RIPR-SPEC-0175: #3731 review fix F2 — sibling-variant guarded pin must not confirm a variant-carrying probe; new fixture pins the non-promotion (both probes weakly_exposed)

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_sibling_variant (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (3)

Reason:
RIPR-SPEC-0122: human-full carries per-finding drill-in commands (#4379); digest why-line names the incomplete stage; unreached static_unknown asks for a test first

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (4)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (5)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (6)

Reason:
RIPR-SPEC-0122: #4321 additive per-finding id lines in human-full (drill-in identifiers)

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — guarded_result_match_sibling_variant (7)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/human.txt`
## Pending — guarded_result_match_sibling_variant (6)

Reason:
RIPR-SPEC-0122: #4321 additive per-finding id lines in human-full (drill-in identifiers)

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (8)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — guarded_result_match_sibling_variant (9)

Reason:
RIPR-SPEC-0197 (#4478): conditional bare equality cannot independently supply return-value oracle credit; preserve dedicated guarded-match and ErrorPath authority

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (10)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — guarded_result_match_sibling_variant (11)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (12)

Reason:
RIPR-SPEC-0224: exact-head review fixes; examined misses never displace oracle rows, error-variant and field gaps name the missing assertion (#5344)

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (13)

Reason:
RIPR-SPEC-0224: a matched related test keeps its oracle kind and strength in full output and adds why it still misses; verdicts unchanged

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (14)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (15)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_sibling_variant (16)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless guarded_result_match_sibling_variant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
