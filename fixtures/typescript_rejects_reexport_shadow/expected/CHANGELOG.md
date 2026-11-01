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

## Pending — typescript_rejects_reexport_shadow (10)

Reason:
RIPR-SPEC-0224: unclassified-kind oracle not graded against this probe (#8388)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (11)

Reason:
RIPR-SPEC-0224: a row with no selected assertion reads 'uses no oracle' (#8388)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (12)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (13)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (14)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — typescript_rejects_reexport_shadow (15)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (16)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (17)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (18)

Reason:
RIPR-SPEC-0122: state only that the finding has no canonical gap, without a discriminator claim (#9244)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (19)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (16)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/human.txt`

## Pending — typescript_rejects_reexport_shadow (20)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (21)

Reason:
RIPR-SPEC-0088: TypeScript preview check JSON states top-level canonical_gap_id (#10709)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (22)

Reason:
RIPR-SPEC-0027 (#10704): an unmodeled TypeScript change shape (literal return, fallback/default literal, identity no-op, comment-only edit) reads infection_unknown with an infection_evidence_unknown stop reason and no repair packet instead of weakly_exposed/exposed decided by oracle strength alone; a changed line under an if arm no related test enters reads reachable_unrevealed with infection no

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (23)

Reason:
RIPR-SPEC-0026: additive confidence_basis and Why capped line for TypeScript/JavaScript preview findings (#10700)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (24)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (25)

Reason:
RIPR-SPEC-0246: plain wording for TypeScript preview check text (#11619)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (26)

Reason:
RIPR-SPEC-0246: re-render after merging main; plain wording for TypeScript preview check text (#11619)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (27)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_rejects_reexport_shadow (28)

Reason:
RIPR-SPEC-0122 plain wording: TypeScript preview next step, observed check heading and rationale say check/assertion (#12124, #11619)

Command:
`cargo xtask goldens bless typescript_rejects_reexport_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
