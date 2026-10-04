# Fixture Corpus: python-verdict-corpus

Spec: RIPR-SPEC-0238 (per-language corpora), with cases drawn from
RIPR-SPEC-0028 and the RIPR-SPEC-0233 acceptance examples.

## Given

Five small authored Python projects under `subjects/`, our own code under
this repository's license, one per test library or library feature:

- `authored-py-pytest-pricing`: pytest asserts, `parametrize`, `raises`,
  `capsys`, `isinstance`, `!=`, a package re-export and an owner no test references.
- `authored-py-pytest-spec0233`: one owner per RIPR-SPEC-0233 acceptance
  example (examples 9, 10 and the `lambda: 0` half of 33 are left out: 9 has
  no realistic owner, 10 needs numpy, and `sorted(x, key=lambda: 0)` raises
  at runtime).
- `authored-py-pytest-fixtures`: conftest fixtures, a parametrized fixture,
  `tmp_path`, `monkeypatch`, `capsys` on stderr.
- `authored-py-unittest-accounts`: `assertEqual`, `assertTrue`,
  `assertRaises`, `mock.assert_called_once_with`, a same-file mixin.
- `authored-py-hypothesis-stats`: Hypothesis `@given` properties, `@example`
  boundaries, `assume`, a reference-implementation oracle and a no-assertion
  property (derandomized in `tests/conftest.py`).

Each case is a one-line edit under `cases/`, labeled with what the project's
own tests discriminate: the edit (and, for a rewrite, each listed mutant on
top) was applied to a scratch copy and the case's test command run under
Python 3.11.15, pytest 9.1.1 and Hypothesis 6.168.3.

## When

`cargo xtask verdict-corpus check --language python` applies each edit to a
run-owned copy, runs `ripr check --json` (Python is enabled by project-marker
detection; no subject carries a `ripr.toml`), and projects the anchored
findings to one verdict.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent, and the report must equal `expected/report.json` and
`expected/report.md`. Where RIPR-SPEC-0233 names a verdict that differs from
today's ("today X"), the case keeps today's verdict in its labeling
observation and its reasoning names the spec's.

## Must Not

- Run mutation testing, pytest, Hypothesis, or network access.
- Treat the rates as a population estimate for Python code.

## Refreshing

When a ripr change moves a verdict, `check` fails and names the first
differing line. Re-run the case's mutants against the stored subject before
re-blessing with
`cargo xtask verdict-corpus report --language python --out fixtures/python-verdict-corpus/expected`.
