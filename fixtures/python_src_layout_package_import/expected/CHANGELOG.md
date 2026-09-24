# Golden Output Changes

## Pending — python_src_layout_package_import (1)

Reason:
RIPR-SPEC-0028: src-layout module identity; a package-name import of a src/ owner credits direct, a same-named function from another module stays orthogonal

Command:
`cargo xtask goldens bless python_src_layout_package_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_src_layout_package_import (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (#4002); this fixture's golden was blessed on a base before that change, so --diff envelopes now omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed.

Command:
`cargo xtask goldens bless python_src_layout_package_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
