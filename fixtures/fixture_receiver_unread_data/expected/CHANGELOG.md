# Golden Output Changes

## Pending — fixture_receiver_unread_data (1)

Reason:
RIPR-SPEC-0240: cfg-pruned tuples and raw competing receiver methods retain fixture opacity; native old/new assertions pass while the pre-repair CLI falsely exposed both findings.

Command:
`cargo xtask goldens bless fixture_receiver_unread_data --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — fixture_receiver_unread_data (2)

Reason:
RIPR-SPEC-0240: retain both unread fixture refusals in JSON and complete human output for the independent honesty assertions.

Command:
`cargo xtask goldens bless fixture_receiver_unread_data --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — fixture_receiver_unread_data (3)

Reason:
RIPR-SPEC-0240: human summary wording re-blessed after merging origin/main (formatting only; no classification change)

Command:
`cargo xtask goldens bless fixture_receiver_unread_data --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — fixture_receiver_unread_data (4)

Reason:
RIPR-SPEC-0122: plain wording for seam and state token in the check digest

Command:
`cargo xtask goldens bless fixture_receiver_unread_data --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — fixture_receiver_unread_data (5)

Reason:
RIPR-SPEC-0122: plain wording for the Evidence stage name and the stage and state legend in the check digest (#10692)

Command:
`cargo xtask goldens bless fixture_receiver_unread_data --reason "..."`

Updated:
- `expected/human.txt`
