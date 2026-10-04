# Golden Output Changes

## Pending — owner_return_pin_identity_traps (1)

Reason:
RIPR-SPEC-0197: initial golden for owner-return pins (#4478)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (2)

Reason:
RIPR-SPEC-0122 / #4520: bounded human check output leads the exposure line with the plain word the summary uses

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (3)

Reason:
RIPR-SPEC-0197 (#4478) acceptance fixture re-rendered in main's format: every identity trap stays non-exposed; per-finding confidence tracks main's current scoring, the 0-exposed contract is unchanged

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (4)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (5)

Reason:
RIPR-SPEC-0122: #5471 check prints the agent stub route only when the stub resolver produces a stub; this route was refused or found no gap, so it is replaced by the refusal reason or removed

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_identity_traps (6)

Reason:
RIPR-SPEC-0122: #5471 the stub route carries the finding probe family as --kind, and seams of that kind are tried first; refusals name that seam

Command:
`cargo xtask goldens bless owner_return_pin_identity_traps --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
