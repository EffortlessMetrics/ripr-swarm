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

## Pending — typescript_rejects_reexport_shadow (3)

Reason:
Combined RIPR-SPEC-0243 rule 3 (#6798): a local expect helper fed by a relation-only re-export is not the runner matcher API, so no assertion extracts through it; the leaf-era exact-value credit on the return probe was a name coincidence and both probes now read weakly_exposed with an unknown oracle. Pinned by reexport_does_not_bind_the_local_rejection_assertion and the corpus row ts_reexported_expect_cannot_credit_local_rejection_helper.

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (4)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (5)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (6)

Reason:
RIPR-SPEC-0002: #7756 labelled human lines carry no terminal period (incl. shared-guidance paths); #7752/#7753/#7755 grammar forms adopted

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (7)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (8)

Reason:
RIPR-SPEC-0088: retain the named limitation and give a terminal manual observation check instead of asking for internal packet fields (#8630); verdict, JSON and packet eligibility unchanged.

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (9)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
