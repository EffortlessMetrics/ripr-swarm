# Golden Output Changes

## Pending — typescript_namespace_inactive_pick (1)

Reason:
RIPR-SPEC-0243 #10477: a skipped runner member renamed off a require namespace cannot register; exact NoStaticPath honesty guard

Command:
`cargo xtask goldens bless typescript_namespace_inactive_pick --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
