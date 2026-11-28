# Golden Output Changes

## Pending — javascript_jest_cjs_unexported_class

Reason:
RIPR-SPEC-0243: a require of a class name the module does not export is not related (#16493)

Command:
`cargo xtask goldens bless javascript_jest_cjs_unexported_class --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
