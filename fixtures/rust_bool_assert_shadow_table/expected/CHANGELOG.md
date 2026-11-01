# Golden Output Changes

## Pending — rust_bool_assert_shadow_table (1)

Reason:
RIPR-SPEC-0353: new honesty fixture, a shadowed assert_eq! keeps the bool chain infection_unknown (#10386)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_bool_assert_shadow_table (2)

Reason:
RIPR-SPEC-0353: add human-full output to the honesty fixture, a shadowed assert_eq! keeps the bool chain infection_unknown (#10386)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_bool_assert_shadow_table (3)

Reason:
RIPR-SPEC-0353: re-bless for the #10426 evidence legend line after merging main (#9619)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## 2026-10-26 — RIPR-SPEC-0118 / #10192 receiving composition

- `human.txt`: name kind and reason on every listed hidden row, preserving currentness and the selected finding. JSON and registered full-human expectations are unchanged.
- Proof: this fixture passes on exact main `aeeeddacd1e24f3b04e713572717e7b172966431`; native receiving producer `a3946a7f1002ae6f069cd18fb4ed45454667b326` emitted the intended presentation before the expectation repair. No verdict or supported-repair claim is added.

## Pending — rust_bool_assert_shadow_table (4)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_bool_assert_shadow_table (5)

Reason:
RIPR-SPEC-0122: clarify test reach inspection before changing tests (#8387)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_bool_assert_shadow_table (6)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_bool_assert_shadow_table (7)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_table --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
