# Golden Output Changes

## Pending — python_rewrite_shifted_bound (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: an integer-equivalent rewrite needs the row where the shifted bound differs, so a lone equality row is weakly_exposed (#10842)

Command:
`cargo xtask goldens bless python_rewrite_shifted_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_rewrite_shifted_bound (2)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_rewrite_shifted_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_rewrite_shifted_bound (2)

Reason:
RIPR-SPEC-0182: Python finding ids move to the versioned v1- namespace (#11368); classification and finding count unchanged; fixture diff and input made consistent so source context is verifiable

Command:
`cargo xtask goldens bless python_rewrite_shifted_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
