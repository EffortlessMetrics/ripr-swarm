# Golden Output Changes

## Pending — verification_prelude_glob_plain_import (1)

Reason:
RIPR-SPEC-0369: a plain import of the crate name keeps a verification-prelude glob foreign (#14116)

Command:
`cargo xtask goldens bless verification_prelude_glob_plain_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — verification_prelude_glob_plain_import (2)

Reason:
RIPR-SPEC-0369: trim trailing blank context line from diff.patch (#14116)

Command:
`cargo xtask goldens bless verification_prelude_glob_plain_import --reason "..."`

Updated:
- `expected/check.json`

## Pending — verification_prelude_glob_plain_import (3)

Reason:
RIPR-SPEC-0369: a foreign glob cannot rebind a core prelude macro (rustc E0659, #16060); glob-dependent fixtures now call vec!

Command:
`cargo xtask goldens bless verification_prelude_glob_plain_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
