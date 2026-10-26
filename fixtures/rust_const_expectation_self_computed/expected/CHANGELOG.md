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
