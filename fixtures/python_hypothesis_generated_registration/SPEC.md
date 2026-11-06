# Fixture: python_hypothesis_generated_registration

Spec: RIPR-SPEC-0028

## Given

A module-level `TestCounter = CounterMachine.TestCase` registration whose
local machine directly inherits the imported Hypothesis RuleBasedStateMachine.
The changed owner is imported by this file. Pytest collection configuration
explicitly selects `tests`.

## When

`cargo xtask fixtures python_hypothesis_generated_registration`

## Then

The diff retains a `no_static_path` finding and a source-bound
`hypothesis_generated_registration` analysis limitation. Selection, activation
and eligible rules remain unresolved. No extracted assertion does not establish
that the generated test has no assertions. Ordinary test evidence remains
independently usable. Generated registrations are excluded from extracted test
counts, and neither count is a native collection or execution count.

## Native observation

On 2026-11-06, Python 3.12.14 on Linux, with the exact dependency pins in
`native/requirements.txt` and the retained pyproject configuration:

- `python -m pytest --collect-only -q tests/test_counter.py`: exit 0, one
  collected node `tests/test_counter.py::TestCounter::runTest`.
- Baseline `return value + 1`, `python -m pytest -q tests/test_counter.py`:
  exit 0, one passed.
- Valid changed owner `return value + 2`, the same test command:
  exit 1, one failed at `CounterMachine.agrees` with `assert 2 == 1`.

The retained input is the changed owner. To repeat the baseline, copy the input
to a disposable workspace and replace only that return with `return value + 1`.
Native output is retained separately under `native/`; it grants no RIPR
sequence, rule, assertion, activation or exact-selector credit.

## Must Not

Synthesize a collected test, generated method, rule/invariant oracle, sequence
input, executed step or executable selector. Analyze state-machine semantics,
unsupported factories, indirect inheritance or alternate runners. Claim release
membership or a support-tier promotion.
