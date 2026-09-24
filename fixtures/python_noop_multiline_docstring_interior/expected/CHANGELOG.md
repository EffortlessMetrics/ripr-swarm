# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0028 / #1289: AST-backed multi-line docstring interior changes emit no probe.

Command:
`cargo xtask goldens bless python_noop_multiline_docstring_interior --reason "RIPR-SPEC-0028 / #1289: AST-backed multi-line docstring interior changes emit no probe"`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_noop_multiline_docstring_interior (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless python_noop_multiline_docstring_interior --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
