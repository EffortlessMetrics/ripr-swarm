# Golden Output Changes

## Pending — guarded_result_match_owner_observation (1)

Reason:
RIPR-SPEC-0174: new fixture for the guarded Result match producer-owned observation (reduced #13162 expect_response shape); the producer-owned seam credits the guarded_result_match oracle and the propagation-complete probe reads exposed (#3709)

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (3)

Reason:
RIPR-SPEC-0122: human-full carries per-finding drill-in commands (#4379); digest why-line names the incomplete stage; unreached static_unknown asks for a test first

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (4)

Reason:
RIPR-SPEC-0122: exposed findings default to info severity

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (5)

Reason:
RIPR-SPEC-0122: discriminator evidence line no longer says yes on findings that are not exposed

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (6)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (7)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (8)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (9)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — guarded_result_match_owner_observation (10)

Reason:
RIPR-SPEC-0197 (#4478): conditional bare equality cannot independently supply return-value oracle credit; preserve dedicated guarded-match and ErrorPath authority

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (11)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending - #5713 reviewed discarded-matcher calibration

Reason:
Retain the dedicated GuardedResultMatch observer and remove the duplicate generic bare ExactValue fact. The supported terminal guard and its assertions survive.

Producer:
Hosted required run37242804945 at613e800282fbf542435831660e4d23a8fa3a4b5a; immutable artifact11318434713, ZIP SHA2562353af6f2a55a514024683d93fb53d474228bd537e71a77bcd57898400b7b4a7.

Transfer:
Exact guarded producer-byte replacement after complete semantic review. No local build, rerun, normalization, or blanket blessing.

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_owner_observation (12)

Reason:
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless guarded_result_match_owner_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
