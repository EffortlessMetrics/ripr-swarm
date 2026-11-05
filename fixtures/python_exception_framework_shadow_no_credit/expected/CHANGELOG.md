# Golden Output Changes

## Pending — python_exception_framework_shadow_no_credit (1)

Reason:
RIPR-SPEC-0238 / RIPR-SPEC-0108: pin the #12952 framework-identity refusal
where a local `pytest.py` context accepts both the old and new exceptions.

Operation:
Captured the existing correct `weakly_exposed` output from the native
baseline at `866c447`, whose Python analysis subtree matches base `255a20f`.
Used the fixture runner's relative root/diff arguments and Linux output
normalization. This was a baseline native capture, not `cargo xtask goldens
bless` or execution of the repaired candidate. The unrepaired `f2abc8d`
native binary wrongly reports `exposed` on the same fixture; direct CPython
execution confirms the unchanged test passes on both old and new sources.

Command shapes (each with the same fresh absolute cache directory and
`RAYON_NUM_THREADS=1`):

- `<baseline-ripr> check --root fixtures/python_exception_framework_shadow_no_credit/input --diff fixtures/python_exception_framework_shadow_no_credit/diff.patch --mode fast --json`
- `<baseline-ripr> check --root fixtures/python_exception_framework_shadow_no_credit/input --diff fixtures/python_exception_framework_shadow_no_credit/diff.patch --mode fast --legend`
- `<baseline-ripr> check --root fixtures/python_exception_framework_shadow_no_credit/input --diff fixtures/python_exception_framework_shadow_no_credit/diff.patch --mode fast --format human-full`

Required follow-up:
Run the fixture, golden comparison and evidence-promotion-honesty check on
the rebuilt repaired head. These baseline captures do not establish that
the repair compiles or passes those gates.

Updated:

- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
