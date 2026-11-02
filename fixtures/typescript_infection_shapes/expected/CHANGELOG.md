# Golden Output Changes

## Pending — typescript_infection_shapes (1)

Reason:
RIPR-SPEC-0027 (#10704): an unmodeled TypeScript change shape (literal return, fallback/default literal, single numeric operand, identity no-op, comment-only edit) reads infection_unknown with an infection_evidence_unknown stop reason and no repair packet instead of weakly_exposed/exposed decided by oracle strength alone; a changed line under an if arm no related test enters reads reachable_unrevealed with infection no

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (2)

Reason:
RIPR-SPEC-0027 (#10704): an unmodeled TypeScript change shape (literal return, fallback/default literal, identity no-op, comment-only edit) reads infection_unknown with an infection_evidence_unknown stop reason and no repair packet instead of weakly_exposed/exposed decided by oracle strength alone; a changed line under an if arm no related test enters reads reachable_unrevealed with infection no

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (3)

Reason:
RIPR-SPEC-0027 (#10704): an unmodeled TypeScript change shape (literal return, fallback/default literal, identity no-op, comment-only edit) reads infection_unknown with an infection_evidence_unknown stop reason and no repair packet instead of weakly_exposed/exposed decided by oracle strength alone; a changed line under an if arm no related test enters reads reachable_unrevealed with infection no

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (4)

Reason:
RIPR-SPEC-0027 (#10704): an unmodeled TypeScript change shape (literal return, fallback/default literal, identity no-op, comment-only edit) reads infection_unknown with an infection_evidence_unknown stop reason and no repair packet instead of weakly_exposed/exposed decided by oracle strength alone; a changed line under an if arm no related test enters reads reachable_unrevealed with infection no

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (5)

Reason:
RIPR-SPEC-0027 (#10704): an unmodeled TypeScript change shape (literal return, fallback/default literal, identity no-op, comment-only edit) reads infection_unknown with an infection_evidence_unknown stop reason and no repair packet instead of weakly_exposed/exposed decided by oracle strength alone; a changed line under an if arm no related test enters reads reachable_unrevealed with infection no

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (6)

Reason:
RIPR-SPEC-0027 (#10704 review): the identity no-op is no longer claimed behavior-neutral, comment detection requires whitespace before //, an unmodeled shape inside an untaken arm reads infection no (branch_not_entered); fixture typescript_infection_shapes re-blessed

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (7)

Reason:
RIPR-SPEC-0122 (#10704): main's Why-unrevealed wording for the untaken-arm case now reads 'no assertion tells the old and new behavior apart'; fixture typescript_infection_shapes human.txt re-rendered after merging origin/main (formatting-only)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (8)

Reason:
RIPR-SPEC-0026: additive confidence_basis and Why capped line for TypeScript/JavaScript preview findings (#10700)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (9)

Reason:
RIPR-SPEC-0027: the trailing-comment edit in this fixture is a no-op and now emits no probe (#10706); it was infection_unknown comment_only

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (10)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (11)

Reason:
RIPR-SPEC-0088: #11619 drop the seam-based qualifier from the repair-packet advisory

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (12)

Reason:
RIPR-SPEC-0246: plain wording for TypeScript preview check text (#11619)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (13)

Reason:
RIPR-SPEC-0246: re-render after merging main; plain wording for TypeScript preview check text (#11619)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (14)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (15)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (16)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
