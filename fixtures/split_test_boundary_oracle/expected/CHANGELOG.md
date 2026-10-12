# Golden Output Changes

## Pending — split_test_boundary_oracle (1)

Reason:
RIPR-SPEC-0186: split-test boundary input and far oracle must not read exposed; names same_test_pairing_missing

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (2)

Reason:
RIPR-SPEC-0186: refresh human analysis-outcome line to findings-below wording from #4777; pairing class unchanged

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (3)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - related-test and observed-value caps, digest missing-discriminator and related-test totals, Hidden block names omitted findings by file:line (class) with the all-base-side distinction (#4320); RIPR-SPEC-0152: all-base-side runs name base-side evidence instead of a lower-priority framing

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (4)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/human.txt`

## Pending — split_test_boundary_oracle (5)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — split_test_boundary_oracle (6)

Reason:
RIPR-SPEC-0197 (#4478) composition with #4828: the return_value probe reads exposed through the owner-return pin (assert_eq!(gate(100), true)), while the predicate probe keeps same_test_pairing_missing and stays weakly_exposed

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (7)

Reason:
RIPR-SPEC-0197/#5027 and RIPR-SPEC-0186: describe the missing admitted boundary-call oracle without asserting that every failed pairing comes from different tests. Classification, confidence, strength and all stage states are unchanged.

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (8)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — split_test_boundary_oracle (9)

Reason:
RIPR-SPEC-0224: related tests ripr examined stay listed and each names why it misses the change (#5344); verdicts unchanged

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (11)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (12)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (13)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — split_test_boundary_oracle (14)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless split_test_boundary_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
