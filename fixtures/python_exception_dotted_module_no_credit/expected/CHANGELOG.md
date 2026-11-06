# Golden Output Changes

## Pending — python_exception_dotted_module_no_credit (1)

Reason:
RIPR-SPEC-0238 / RIPR-SPEC-0108: preserve the #12952 refusal when
`fx.core.py` cannot supply the imported package module `fx.core`.

Operation:
Captured the existing `weakly_exposed` output from the native baseline at
`866c447`, whose Python analysis subtree matches source ancestor `255a20f`.
Used the fixture runner's relative root/diff arguments and inspected Linux
normalization. These are actual baseline captures, not `cargo xtask goldens
bless` or execution of the new import-identity candidate. The unrepaired
`a237b67` native binary reports `exposed` with exception-variant credit on
this exact input. Independent CPython import checks fail to import `fx.core`
on both versions because no package `fx` exists. No pytest run is claimed.

Command shapes (with an absolute scratch cache and `RAYON_NUM_THREADS=1`):

- `<baseline-ripr> check --root fixtures/python_exception_dotted_module_no_credit/input --diff fixtures/python_exception_dotted_module_no_credit/diff.patch --mode fast --json`
- `<baseline-ripr> check --root fixtures/python_exception_dotted_module_no_credit/input --diff fixtures/python_exception_dotted_module_no_credit/diff.patch --mode fast --legend`
- `<baseline-ripr> check --root fixtures/python_exception_dotted_module_no_credit/input --diff fixtures/python_exception_dotted_module_no_credit/diff.patch --mode fast --format human-full`

Required follow-up:
Compare all three surfaces with the rebuilt import-identity candidate and
run the required fixture, golden and evidence-promotion-honesty gates.
The baseline captures do not establish compilation or those gate results.

Updated:

- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
