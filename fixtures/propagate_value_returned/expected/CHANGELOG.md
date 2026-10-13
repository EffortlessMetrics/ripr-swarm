# Golden Output Changes

## Pending — propagate_value_returned (1)

Reason:
initial golden for fix B control: returned call stays weakly_exposed

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (2)

Reason:
re-bless after merging origin/main (#1216): discriminate message now observation_unverified; classification weakly_exposed unchanged

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (3)

Reason:
bound default human output to start-here triage; human-full preserves exhaustive evidence

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (4)

Reason:
changed semantic heads use canonical parser expressions and content-addressed probe identities

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (5)

Reason:
restrict CallDeletion probes to standalone call statements; refresh affected goldens and record intentional output changes

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (6)

Reason:
#2103: additive changed_files_by_language field and changed_rust_files now Rust-only count

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (7)

Reason:
#2567: default human render no longer prints a 'Hidden: 0 lower-priority finding(s) omitted' block when nothing was omitted; the format pointers now sit under a 'More:' heading. Formatting-only drift; no evidence, class, or JSON change.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (8)

Reason:
Issue #2598: default human output now exposes bounded explain and context follow-up commands for the selected finding.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (9)

Reason:
Issue #2659: finding navigation commands now preserve the analyzed root, diff or artifact scope and shell-safe identity.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (10)

Reason:
RIPR-SPEC-0147: publish typed analysis outcome in human and JSON output.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (11)

Reason:
RIPR-SPEC-0147: align fixture outputs with the typed incomplete-outcome and unquoted human outcome contract.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (12)

Reason:
RIPR-SPEC-0023: classification hint added to digest (#2614)

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (13)

Reason:
RIPR-SPEC-0151: rebless check JSON for the additive source_currentness field; classifications, stages, confidence, counts, and recorded coordinates remain unchanged.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`

## Pending — propagate_value_returned (14)

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (15)

Reason:
RIPR-SPEC-0001: #3161 PR-B production witness gate upgrades complete direct-flow propagate evidence and fails incomplete flows closed

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (16)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (17)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (18)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (19)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/human.txt`

## Pending — propagate_value_returned (20)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — propagate_value_returned (21)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — propagate_value_returned (22)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (23)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (24)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (25)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (26)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagate_value_returned (27)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless propagate_value_returned --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
