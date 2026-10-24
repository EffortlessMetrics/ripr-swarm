# Golden Output Changes

## Pending — rust_path_keyword_affinity (1)

Reason:
RIPR-SPEC-0172: new fixture, a path keyword no longer relates an unrelated assertion test (#9171)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (2)

Reason:
RIPR-SPEC-0172: new fixture, a path or declaration keyword (crate, union, macro_rules!) no longer relates an unrelated assertion test while a real identifier still does (#9171)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (3)

Reason:
RIPR-SPEC-0172:_fixture_diff_context_trim_and_main_merge_(#9171)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
