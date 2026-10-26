# Golden Output Changes

## Pending — rust_path_keyword_affinity (1)

Reason:
RIPR-SPEC-0172: new fixture, a path or declaration keyword (crate, union, macro_rules!) no longer relates an unrelated assertion test while a real identifier still does (#9171)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (2)

Reason:
RIPR-SPEC-0172: fixture now changes a pub union declaration line and a macro_rules! line, with unrelated sibling tests declaring their own union/macro, so declaration introducers are exercised (#9171)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (3)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (4)

Reason:
RIPR-SPEC-0122: state only that the finding has no canonical gap, without a discriminator claim (#9244)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (5)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_path_keyword_affinity (3)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless rust_path_keyword_affinity --reason "..."`

Updated:
- `expected/human.txt`
