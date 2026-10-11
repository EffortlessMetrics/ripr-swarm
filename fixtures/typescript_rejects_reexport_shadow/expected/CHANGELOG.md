# Golden Output Changes

## Pending — typescript_rejects_reexport_shadow (1)

Reason:
RIPR-SPEC-0243 #7315: local helper behind relation-only re-export must not gain exact rejected-reason credit; actual runtime accepts both wrong and correct reasons

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_rejects_reexport_shadow (2)

Reason:
RIPR-SPEC-0243 #7315: preserve actual full human projection for relation-only re-export negative control

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
