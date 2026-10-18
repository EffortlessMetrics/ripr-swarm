# Golden Output Changes

## Pending — macro_site_scoped_rest_limit (1)

Reason:
RIPR-SPEC-0240: scoped deferred definitions retain uncertainty (#8265)

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (2)

Reason:
RIPR-SPEC-0240 (#8265): preserve the foreign-glob scoped-site witness after #8388 trusts std/core. Use builtin alloc with an explicit extern declaration; inspected native output retains two static_unknown findings, both limitation codes, the unscoped alloc glob and no gap IDs. The extra declaration moves the changed bound to line 4.

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
