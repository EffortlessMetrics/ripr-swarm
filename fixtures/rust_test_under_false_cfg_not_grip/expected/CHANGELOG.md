# Golden Output Changes

## Pending — rust_test_under_false_cfg_not_grip (1)

Reason:
RIPR-SPEC-0153: new fixture; a #[test] under a never-true cfg is not discovered and cannot grip the change (#6293)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (2)

Reason:
RIPR-SPEC-0153: main now renders workspace-relative finding paths (./src/lib.rs) and adds canonical-gap fields; static_unknown verdict and reach unchanged (#6293)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (3)

Reason:
RIPR-SPEC-0153: fixture input now matches diff.patch (cents / 9, was / 10), so the changed expression is a mapped return_value probe; verdict moves from static_unknown to no_static_path with propagation yes, still reach no and no test named (#7043)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (4)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (5)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (6)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (7)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_test_under_false_cfg_not_grip (8)

Reason:
RIPR-SPEC-0122: no_static_path Weakness list no longer restates the Evidence block (#9033)

Command:
`cargo xtask goldens bless rust_test_under_false_cfg_not_grip --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
