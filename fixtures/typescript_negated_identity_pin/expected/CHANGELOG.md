# Golden Output Changes

## Pending — typescript_negated_identity_pin (1)

Reason:
RIPR-SPEC-0246 rule 1 (#8712): pin that a negated identity matcher cannot pin a copy-for-identity change to exposed

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_negated_identity_pin (2)

Reason:
RIPR-SPEC-0122 / #8729 (#8856): preserve the producer-owned NoCanonicalGap refusal advisory for this preview finding; no TypeScript verdict, JSON, or repair eligibility change

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
