# Golden Output Changes

## Pending — python_exception_stale_type_no_credit (1)

Reason:
RIPR-SPEC-0238 / RIPR-SPEC-0108: pin the #13622 stale-exception-type refusal where pytest.raises(ValueError, match) follows a ValueError to TypeError raise change.

Command:
`cargo xtask goldens bless python_exception_stale_type_no_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
