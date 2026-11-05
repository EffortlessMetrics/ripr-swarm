# Golden Output Changes

## Native capture — 2026-11-05

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: retain the independently reproduced
shadowed-reader counterexample as exactly one non-promoting finding with
unknown reach and no actionable repair or receipt command.

Commands:

- `ripr check --root fixtures/python_module_string_reader_package_shadow/input --diff fixtures/python_module_string_reader_package_shadow/diff.patch --mode fast --json`
- `ripr check --root fixtures/python_module_string_reader_package_shadow/input --diff fixtures/python_module_string_reader_package_shadow/diff.patch --mode fast --legend`
- `ripr check --root fixtures/python_module_string_reader_package_shadow/input --diff fixtures/python_module_string_reader_package_shadow/diff.patch --mode fast --format human-full`

These commands ran from a fixture capture checkout using the verified staged
native artifact. Exact source head: `24337644300ff9614f8eefea3ad569c9e325802d`.
Source tree: `08dca52574eea51e177b020272b6c388cdf9821d`.
Workflow run: `42006889189`; artifact: `13405426654`.
Binary SHA256: `a89c534fbd50abd14b2eca695493e3e7253c9bca9a41d97196c9eda17ed2657c`.
Capture used `RAYON_NUM_THREADS=1` and one fresh cache per fixture, shared by
the three surfaces. Only the repository Linux fixture normalizations from
`xtask/src/reports/fixtures.rs` at `49b9ad25e0aa45e22eca9dcc4028ca1c5252817d` were applied:
remove the optional canonical-next-action and language-completeness members,
normalize the root closer, and replace the checkout prefix with `<cwd>/`.
Source escapes, source-subject digests and remaining renderer bytes were
preserved. Human text has one final newline.

This records direct native capture. No `cargo xtask goldens bless`, fixture
gate, evidence-promotion-honesty gate, pytest or mutation execution is claimed.

Updated:

- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
