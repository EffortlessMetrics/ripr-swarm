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
RIPR-SPEC-0122: carry the --kind error_path suffix this golden missed; the stub route always carries --kind per spec and 110 blessed siblings

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (4)

Reason:
RIPR-SPEC-0122 #7394: attributable shared CLI evidence and compact empty section spacing preserve every complete finding record; library legacy bytes remain unchanged. This refresh changes presentation only, with no analyzer, JSON, oracle or recommendation change.

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (5)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (6)

Reason:
RIPR-SPEC-0002: #7756 labelled human lines carry no terminal period (incl. shared-guidance paths); #7752/#7753/#7755 grammar forms adopted

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (7)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_adversarial_shared_error_variant_proximity (8)

Reason:
RIPR-SPEC-0122: printed agent stub --at line is now the seam line the stub targets (#8444); the old line triggered nearest-gap substitution

Command:
`cargo xtask goldens bless rust_adversarial_shared_error_variant_proximity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
