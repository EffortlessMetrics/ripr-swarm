# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0009: add opaque fixture-builder example corpus baseline

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0026 output(language): RustAdapter tags each Finding with language=rust; check.json gains the additive optional language field

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
schema 0.2: dedup assertion text into finding-level assertion_texts map (#1035)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
content-addressed-probe-ids-#1053

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
platform-stable content-addressed ids (#1053): normalize owner path separators in fp8

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0090: adds all-no-path advisory disclosure line to human output when all findings are no-path/unknown

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0090: adds all-no-path advisory disclosure line to human output when all findings are no-path/unknown

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
additive: add related_tests_total cap field (mirrors repo-exposure pattern)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
additive: add related_tests_total cap field (mirrors repo-exposure pattern)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
additive: add related_tests_total cap field (mirrors repo-exposure pattern)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
add relation_reason and relation_confidence fields to related_test JSON output

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0109: cap headline confidence by weakest stage's per-stage Confidence (#1219 part D)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
P2: honest no-static-path messaging (RIPR-SPEC-0113)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Honesty (dogfood anyhow Chain::len): suppress the all-no-static-path note when a finding has reach=yes — a reaching test IS a static test path; the note must not contradict the finding's own reach evidence (RIPR-SPEC-0090)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
bound default human output to start-here triage; human-full preserves exhaustive evidence

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
changed semantic heads use canonical parser expressions and content-addressed probe identities

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
#2103: additive changed_files_by_language field and changed_rust_files now Rust-only count

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
#2567: default human render no longer prints a 'Hidden: 0 lower-priority finding(s) omitted' block when nothing was omitted; the format pointers now sit under a 'More:' heading. Formatting-only drift; no evidence, class, or JSON change.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2598: default human output now exposes bounded explain and context follow-up commands for the selected finding.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
Issue #2659: finding navigation commands now preserve the analyzed root, diff or artifact scope and shell-safe identity.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: publish typed analysis outcome in human and JSON output.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0147: align fixture outputs with the typed incomplete-outcome and unquoted human outcome contract.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0023: classification hint added to digest (#2614)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0151: rebless check JSON for the additive source_currentness field; classifications, stages, confidence, counts, and recorded coordinates remain unchanged.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (3)

Reason:
RIPR-SPEC-0122: discriminator evidence line no longer says yes on findings that are not exposed

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (4)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (5)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (6)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/human.txt`

## Pending — opaque_fixture_builder (7)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — opaque_fixture_builder (8)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)
RIPR-SPEC-0009: literal-free related tests name the changed boundary instead of claiming opaque fixtures

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (9)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (11)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (12)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (9)

Reason:
RIPR-SPEC-0122: #5471 the stub route carries the finding probe family as --kind, and seams of that kind are tried first; refusals name that seam

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — opaque_fixture_builder (13)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless opaque_fixture_builder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
