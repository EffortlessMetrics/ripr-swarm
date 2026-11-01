# Golden Output Changes

## Pending — python_scalar_code_replacement (1)

Reason:
RIPR-SPEC-0028: pin the scalar reader code-replacement refusal for python_scalar_code_replacement (#10626)

Command:
`cargo xtask goldens bless python_scalar_code_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_scalar_code_replacement (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_scalar_code_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_scalar_code_replacement (3)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_scalar_code_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
