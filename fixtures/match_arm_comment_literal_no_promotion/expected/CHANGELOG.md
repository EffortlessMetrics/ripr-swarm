# Golden Output Changes

## Pending — match_arm_comment_literal_no_promotion (1)

Reason:
RIPR-SPEC-0108: comment-literal no-promotion witness for 3766 repair

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_comment_literal_no_promotion (2)

Reason:
RIPR-SPEC-0108: record human-full in updated artifacts for 3766 repair

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (3)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (4)

Reason:
RIPR-SPEC-0122: human-full carries per-finding drill-in commands (#4379); digest why-line names the incomplete stage; unreached static_unknown asks for a test first

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (5)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (6)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (7)

Reason:
RIPR-SPEC-0122: #4321 additive per-finding id lines in human-full (drill-in identifiers)

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — match_arm_comment_literal_no_promotion (8)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/human.txt`
## Pending — match_arm_comment_literal_no_promotion (7)

Reason:
RIPR-SPEC-0122: #4321 additive per-finding id lines in human-full (drill-in identifiers)

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (9)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — match_arm_comment_literal_no_promotion (10)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — match_arm_comment_literal_no_promotion (11)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (12)

Reason:
RIPR-SPEC-0224: a matched related test keeps its oracle kind and strength in full output and adds why it still misses; verdicts unchanged

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (13)

Reason:
RIPR-SPEC-0224, #5508: observation_unconfirmed now reads 'ripr could not confirm that this assertion observes the changed behavior'; human-full re-blessed after rebase onto #5424. No verdict change.

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (14)

Reason:
RIPR-SPEC-0224, #5508: an observation_unconfirmed row is labelled 'unconfirmed:' instead of 'misses:' in human-full. No verdict change.

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (15)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (16)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0229 (#5432): related tests that call the owner with an input selecting another arm name the changed arm as the missing input (re-blessed over #5578 wording)
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (17)

Reason:
RIPR-SPEC-0045: review fix #5268 - quoted string/char literals are now byte-encoded in the Rust gap discriminator key so distinct predicates never share one identity; golden-drift.json shows zero semantic flips on the re-run

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (17)

Reason:
RIPR-SPEC-0002: merge of origin/main onto the #5996 workspace-relative location owner — expected files regenerate with main's match-arm verdict updates and the shared root-relative location form (issue #5996)

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (18)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_comment_literal_no_promotion (19)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless match_arm_comment_literal_no_promotion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
