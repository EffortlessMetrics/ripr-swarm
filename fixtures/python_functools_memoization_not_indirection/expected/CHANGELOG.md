# Golden Output Changes

## Pending — python_functools_memoization_not_indirection (1)

Reason:
RIPR-SPEC-0028: functools lru_cache is not decorator_indirection; credited exact twin and smoke-gap twin

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (2)

Reason:
RIPR-SPEC-0028: align fixture hunk with source line numbers

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (3)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (4)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0140: adopt intended config_identity hash from #6777 missed by its re-bless sweep (#6818)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (5)

Reason:
RIPR-SPEC-0002: merge of origin/main onto the #5996 workspace-relative location owner — the expected files regenerate with main's triage/wording updates and the shared root-relative location form (issue #5996)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (6)

Reason:
RIPR-SPEC-0224: record test_assertion_admission evidence for assertion-free related tests (#5571)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (7)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (8)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (9)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (10)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (11)

Reason:
RIPR-SPEC-0002: #7756 labelled human lines carry no terminal period (incl. shared-guidance paths); #7752/#7753/#7755 grammar forms adopted

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (12)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (13)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (14)

Reason:
RIPR-SPEC-0224: unclassified-kind oracle not graded against this probe (#8388)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (15)

Reason:
RIPR-SPEC-0224: a row with no selected assertion reads 'uses no oracle' (#8388)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (16)

Reason:
RIPR-SPEC-0122: non-Rust repair-start refusal names the Rust-only seam inventory and the manual route (#9102)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (17)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (18)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (19)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (20)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_functools_memoization_not_indirection (21)

Reason:
RIPR-SPEC-0224: Python no_assertion row miss on an extraction-complete assertion-free call-bearing test (#5491)

Command:
`cargo xtask goldens bless python_functools_memoization_not_indirection --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
