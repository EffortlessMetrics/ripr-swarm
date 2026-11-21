# Golden Output Changes

## Pending — cross-surface annotation conformance (#12925)

Add the existing finding projection as `sarif.json` and `github.txt` under
RIPR-SPEC-0008 cross-surface conformance. Class, canonical identity,
configured severity and refusal guidance come from the existing producer;
the check/human goldens and analyzer behavior are unchanged.

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

## Pending — typescript_infection_shapes (17)

Reason:
RIPR-SPEC-0122: drill-in and repair-start command lines carry consumer labels naming their id domain (#12431)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (18)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (19)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_infection_shapes (20)

Reason:
RIPR-SPEC-0122: one blank line, not two, separates the More block from a trailing Note (#14318)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/human.txt`

## Pending — typescript_infection_shapes (21)

Reason:
RIPR-SPEC-0243: say('a', true) fills the changed default loud = true, so it never applies (#15393)

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/sarif.json`
- `expected/github.txt`

## Pending — typescript_infection_shapes (22)

Reason:
RIPR-SPEC-0243 #15393: a filled default's advice asks for a call that omits the argument

Command:
`cargo xtask goldens bless typescript_infection_shapes --reason "..."`

Updated:
- `expected/check.json`
- `expected/sarif.json`
- `expected/github.txt`
