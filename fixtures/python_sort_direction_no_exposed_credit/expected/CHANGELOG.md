# Golden Output Changes

## Pending — python_sort_direction_no_exposed_credit (1)

Reason:
RIPR-SPEC-0028: a sort direction flip whose related calls pass no two different elements reads weakly_exposed (#16203)

Command:
`cargo xtask goldens bless python_sort_direction_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_sort_direction_no_exposed_credit (2)

Reason:
RIPR-SPEC-0028: the call-variant gate's messages say only that no related call is seen to pass the separating input

Command:
`cargo xtask goldens bless python_sort_direction_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_sort_direction_no_exposed_credit (3)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless python_sort_direction_no_exposed_credit --reason "..."`

Updated:
- `expected/human.txt`

## Pending — python_sort_direction_no_exposed_credit (4)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless python_sort_direction_no_exposed_credit --reason "..."`

Updated:
- `expected/human-full.txt`
