# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0094 Part B: Mode::Warm assertion does NOT confirm Mode::Frozen arm (qualifier-blind hole); initial golden locking the variant-scope fix

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
bound default human output to start-here triage; human-full preserves exhaustive evidence

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
#2103: additive changed_files_by_language field and changed_rust_files now Rust-only count

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
#2567: default human render no longer prints a 'Hidden: 0 lower-priority finding(s) omitted' block when nothing was omitted; the format pointers now sit under a 'More:' heading. Formatting-only drift; no evidence, class, or JSON change.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2598: default human output now exposes bounded explain and context follow-up commands for the selected finding.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2659: finding navigation commands now preserve the analyzed root, diff or artifact scope and shell-safe identity.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: publish typed analysis outcome in human and JSON output.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: align fixture outputs with the typed incomplete-outcome and unquoted human outcome contract.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0023: classification hint added to digest (#2614)

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0151: rebless check JSON for the additive source_currentness field; classifications, stages, confidence, counts, and recorded coordinates remain unchanged.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (3)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (4)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (5)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/human.txt`

## Pending — match_arm_type_token_blind (6)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — match_arm_type_token_blind (7)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — match_arm_type_token_blind (8)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (9)

Reason:
RIPR-SPEC-0224, #5508: observation_unconfirmed now reads 'ripr could not confirm that this assertion observes the changed behavior', an unknown rather than a claim that the assertion misses. The miss token and every decision field are unchanged.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (11)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0229 (#5432): related tests that call the owner with an input selecting another arm name the changed arm as the missing input (re-blessed over #5578 wording)
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (12)

Reason:
RIPR-SPEC-0002: merge of origin/main onto the #5996 workspace-relative location owner — expected files regenerate with main's match-arm verdict updates and the shared root-relative location form (issue #5996)

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_type_token_blind (13)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless match_arm_type_token_blind --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
