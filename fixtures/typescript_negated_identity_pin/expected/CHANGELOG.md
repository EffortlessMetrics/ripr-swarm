# Golden Output Changes

## Pending — typescript_negated_identity_pin (1)

Reason:
RIPR-SPEC-0246 rule 1 (#8712): pin that a negated identity matcher cannot pin a copy-for-identity change to exposed

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
