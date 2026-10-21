# Golden Output Changes
## Pending — rust_constructor_field_observation (1)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (2)

Reason:
RIPR-SPEC-0001: reach is weak when every related test is linked only by file or name-token proximity (weak_token_substring); class unchanged, reach stage yes->weak

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (3)

Reason:
RIPR-SPEC-0122: human-full carries per-finding drill-in commands (#4379); digest why-line names the incomplete stage; unreached static_unknown asks for a test first

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (4)

Reason:
RIPR-SPEC-0122: discriminator evidence line no longer says yes on findings that are not exposed

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (5)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (6)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (7)

Reason:
RIPR-SPEC-0122: #4321 additive per-finding id lines in human-full (drill-in identifiers)

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — rust_constructor_field_observation (8)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/human.txt`
## Pending — rust_constructor_field_observation (9)

Reason:
RIPR-SPEC-0122: #4321 additive per-finding id lines in human-full (drill-in identifiers)

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (10)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — rust_constructor_field_observation (11)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (12)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (13)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (14)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (15)

Reason:
RIPR-SPEC-0159 (#6694/#6672): the tests/lower.rs test that calls the lower_ast entry now relates to private lower_statement through the chain (helper_owner_call, reach yes) instead of weak_token_substring, because the chain relation now outranks token proximity; class unchanged. human.txt and human-full.txt also drop the "ripr found no static test path..." footer note, since that finding now has a static test path

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (16)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (17)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (18)

Reason:
RIPR-SPEC-0003: #7477 gives reason-specific uncertainty-resolution actions without verdict-change promises or unsupported deep/fixture-config routes; classifications, evidence, and decidable guidance unchanged.

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (19)

Reason:
RIPR-SPEC-0003: #7477 conditional assertion inspection and actual configured runner guidance; whole JSON evidence and classifications independently preserved.

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (20)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (21)

Reason:
RIPR-SPEC-0001: output contract changed - 7798 collapse tautological unknown-sink flow row when sink text restates the kind label

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (22)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_constructor_field_observation (23)

Reason:
RIPR-SPEC-0268: human evidence window glosses the unknown-sink row with its meaning (#8744)

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — rust_constructor_field_observation (24)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — rust_constructor_field_observation (25)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down; printed agent stub --at line is now the seam line the stub targets (#8444); the old line triggered nearest-gap substitution

Command:
`cargo xtask goldens bless rust_constructor_field_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
