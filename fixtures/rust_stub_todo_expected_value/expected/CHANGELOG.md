# Golden Output Changes

## Pending — rust_stub_todo_expected_value (1)

Reason:
RIPR-SPEC-0231: rule 11 - an unfilled placeholder expected value pins nothing; a stub todo!() expected value earns no strong exact-value credit (#14810)

Command:
`cargo xtask goldens bless rust_stub_todo_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_stub_todo_expected_value (2)

Reason:
RIPR-SPEC-0231: rule 11 - an unfilled placeholder expected value pins nothing; a stub todo!() expected value earns no strong exact-value credit (#14810)

Command:
`cargo xtask goldens bless rust_stub_todo_expected_value --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — rust_stub_todo_expected_value (3)

Reason:
RIPR-SPEC-0231: rule 11 - an unfilled placeholder expected value pins nothing; a stub todo!() expected value earns no strong exact-value credit and the finding discloses the uncredited assertion (#14810)

Command:
`cargo xtask goldens bless rust_stub_todo_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
