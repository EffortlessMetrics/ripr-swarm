# Golden Output Changes

## Pending — verification_prelude_glob_plain_import (1)

Reason:
RIPR-SPEC-0369: a plain import of the crate name keeps a verification-prelude glob foreign (#14116)

Command:
`cargo xtask goldens bless verification_prelude_glob_plain_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
