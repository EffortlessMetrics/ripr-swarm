# Golden Output Changes

## Pending — rust_helper_expectation_self_computed (1)

Reason:
RIPR-SPEC-0197: new helper-computed expectation fixture (#7714)

Command:
`cargo xtask goldens bless rust_helper_expectation_self_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## 2026-10-26 — RIPR-SPEC-0118 / #10192

- `human.txt`: name kind and reason on the hidden base-deleted static-unknown row. The unchanged renderer was exercised with a hidden-finding JSON object identical to the incoming main pin; classification, JSON and full-human expectations are unchanged.
