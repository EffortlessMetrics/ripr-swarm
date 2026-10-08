# Golden Output Changes

## Pending — rust_adversarial_shared_error_variant_proximity (1)

Reason:
RIPR-SPEC-0108: new false-exposed guard for #7063; a same-file pin of a shared error variant on another owner reads weakly_exposed, never exposed

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (2)

Reason:
RIPR-SPEC-0108: register expected/human-full.txt for the #7063 false-exposed guard so the honesty corpus can pin its class

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (3)

Reason:
RIPR-SPEC-0232: #6306's stub-route render appends --kind error_path to the agent stub drill-in when the resolver yields an error-path stub; this fixture's error_path finding now renders the flag. Formatting-only drift on one drill-in line; finding verdict class unchanged. Same inherited repair as #7143.

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
