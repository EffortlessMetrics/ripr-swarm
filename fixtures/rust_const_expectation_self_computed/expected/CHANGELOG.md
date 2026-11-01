# Golden Output Changes

## Pending — rust_const_expectation_self_computed (1)

Reason:
RIPR-SPEC-0035: new const-expectation fixture for the evidence-promotion honesty corpus (#7260)

Command:
`cargo xtask goldens bless rust_const_expectation_self_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## 2026-10-26 — RIPR-SPEC-0118 / #10192 receiving composition

- `human.txt`: name kind and reason on every listed hidden row, preserving currentness and the selected finding. JSON and registered full-human expectations are unchanged.
- Proof: this fixture passes on exact main `aeeeddacd1e24f3b04e713572717e7b172966431`; native receiving producer `a3946a7f1002ae6f069cd18fb4ed45454667b326` emitted the intended presentation before the expectation repair. No verdict or supported-repair claim is added.

## Pending — rust_const_expectation_self_computed (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless rust_const_expectation_self_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_const_expectation_self_computed (3)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless rust_const_expectation_self_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_const_expectation_self_computed (4)

Reason:
RIPR-SPEC-0122: #10691 plain wording for the default digest's missing-assertion entry and the catch-all change type

Command:
`cargo xtask goldens bless rust_const_expectation_self_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_const_expectation_self_computed (5)

Reason:
RIPR-SPEC-0122 plain wording for the check screen's whole-seam/untied-spot refusals (#11619)
RIPR-SPEC-0152: base-side findings (base_deleted, moved_or_renamed) carry the not-a-candidate-edit-target refusal as their next step instead of test advice (#11905)

Command:
`cargo xtask goldens bless rust_const_expectation_self_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
