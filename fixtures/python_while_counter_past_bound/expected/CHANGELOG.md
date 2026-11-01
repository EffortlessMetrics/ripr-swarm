# Golden Output Changes

## Pending — python_while_counter_past_bound (1)

Reason:
RIPR-SPEC-0028: issue 9733 native old and edited countdown(-2) both return [], so the empty-list oracle cannot discriminate the rebound boundary

Command:
`cargo xtask goldens bless python_while_counter_past_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_while_counter_past_bound (2)

Reason:
RIPR-SPEC-0028: retain the full human projection required by the honesty corpus's class and oracle invariants.

Command:
`cp target/ripr/fixtures/python_while_counter_past_bound/human-full.txt fixtures/python_while_counter_past_bound/expected/human-full.txt`

Updated:
- `expected/human-full.txt`, retained from the same native golden run as entry (1); production is unchanged between that run and this evidence-only addition. No fresh golden execution is claimed by this copy.

## Pending — python_while_counter_past_bound (3)

Reason:
RIPR-SPEC-0028: refresh compact human wrapping and explanatory legend after main output changes; JSON verdict and full human evidence remain unchanged.

Command:
`cargo xtask goldens bless python_while_counter_past_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_while_counter_past_bound (4)

Reason:
RIPR-SPEC-0028 refresh human summary wording to current renderer

Command:
`cargo xtask goldens bless python_while_counter_past_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_while_counter_past_bound (5)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_while_counter_past_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
