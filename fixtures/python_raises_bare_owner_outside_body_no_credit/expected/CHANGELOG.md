# Golden Output Changes

## Pending — python_raises_bare_owner_outside_body_no_credit (3)

Reason:
The verify line is platform-rendered: `rooted_pytest_action` returns None on
Windows, so the Windows-blessed text carried the unverified suggested command.
Hosted (non-Windows) output renders the rooted Bash action, matching the other
Python fixtures. No finding, class, or command content changed.

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_raises_bare_owner_outside_body_no_credit (1)

Reason:
RIPR-SPEC-0108: bare owner named only outside the exception body stays weakly_exposed (#14267)

Command:
`cargo xtask goldens bless python_raises_bare_owner_outside_body_no_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_raises_bare_owner_outside_body_no_credit (2)

Reason:
RIPR-SPEC-0108: bare owner named only outside the exception body stays weakly_exposed (#14267)

Command:
`cargo xtask goldens bless python_raises_bare_owner_outside_body_no_credit --reason "..."`

Updated:
- `expected/human-full.txt`
