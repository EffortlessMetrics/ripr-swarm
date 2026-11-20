# Golden Output Changes

## Pending — rust_multi_site_helper_reach (1)

Reason:
RIPR-SPEC-0159/#14918: a helper the reached entry calls at several sites carries reach through a reach-only hop

Command:
`cargo xtask goldens bless rust_multi_site_helper_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_multi_site_helper_reach (2)

Reason:
RIPR-SPEC-0159/#14874: identify the starts_with conjunct on the changed length comparison while keeping infection unknown.

Command:
`cargo xtask goldens bless rust_multi_site_helper_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
