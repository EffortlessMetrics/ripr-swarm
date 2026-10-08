# Golden Output Changes

## Pending — gap_kept_ignored_test (1)

Reason:
RIPR-SPEC-0240: paired fixture for gap withholding on analyzer-limit assertion refusals

Command:
`cargo xtask goldens bless gap_kept_ignored_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_kept_ignored_test (2)

Reason:
RIPR-SPEC-0240: related tests carry #5424 per-test miss reasons

Command:
`cargo xtask goldens bless gap_kept_ignored_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_kept_ignored_test (3)

Reason:
RIPR-SPEC-0240: rebased onto main, where finding paths render workspace-relative and gaps carry canonical_gap_id

Command:
`cargo xtask goldens bless gap_kept_ignored_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_kept_ignored_test (4)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless gap_kept_ignored_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_kept_ignored_test (5)

Reason:
RIPR-SPEC-0240: a non-limit assertion refusal no longer offers a static-limit reading (#6903)

Command:
`cargo xtask goldens bless gap_kept_ignored_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_kept_ignored_test (6)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless gap_kept_ignored_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
