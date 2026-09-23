# Golden Output Changes

## Pending — python_src_layout_package_import (1)

Reason:
RIPR-SPEC-0028: src-layout module identity; a package-name import of a src/ owner credits direct, a same-named function from another module stays orthogonal

Command:
`cargo xtask goldens bless python_src_layout_package_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
