# Fixture Corpus: python-verdict-corpus

Spec: RIPR-SPEC-0245 (per-language corpora), with cases drawn from
RIPR-SPEC-0028 and the RIPR-SPEC-0238 acceptance examples.

## Given

Small authored Python projects under `subjects/`, our own code under
this repository's license, one per test library or library feature:

- `authored-py-pytest-pricing`: pytest asserts, `parametrize`, `raises`,
  `capsys`, `isinstance`, `!=`, a package re-export and an owner no test references.
- `authored-py-pytest-spec0233`: at least one case per runnable
  RIPR-SPEC-0238 acceptance example, one owner per case; examples 9, 22, 24
  and 34 have two, and example 9's two share one owner and anchor. Example
  9's owner returns the handler it was given, as a registration helper
  does, so the mock the test asserts on is the owner's result
  (`tests/test_handler.py`). Example 10 is in `authored-py-pytest-numpy`,
  and the `lambda: 0` half of 33 is left out because
  `sorted(x, key=lambda: 0)` raises at runtime. Alternate forms within an
  example are not all covered:
  6's assertEqual form, 18's form with the test bodies swapped between the
  names, 19's `Exception, match=''`, 24's third form, 25's no-import form,
  the alternate edits of 26 to 28, 29's `patch('os.getcwd')` and
  `mocker.patch` tests, 31's exact-assertion form, and 34's forms other
  than `exc.value is not None` and `len(str(exc.value)) == 7` (in
  `tests/test_exc_info.py`).
- `authored-py-pytest-fixtures`: conftest fixtures, a parametrized fixture,
  `tmp_path`, `monkeypatch`, `capsys` on stderr.
- `authored-py-unittest-accounts`: `assertEqual`, `assertTrue`,
  `assertRaises`, `mock.assert_called_once_with`, a same-file mixin.
- `authored-py-hypothesis-stats`: Hypothesis `@given` properties, `@example`
  boundaries, `assume`, a reference-implementation oracle and a no-assertion
  property (derandomized in `tests/conftest.py`).
- `authored-py-pytest-numpy`: RIPR-SPEC-0238 example 10, an owner whose
  only test asserts through `np.testing.assert_array_equal`. It is a
  subject of its own so the other subjects' tests still run without
  numpy; its truth was recorded with numpy 2.1.3.
- `authored-py-pytest-blocks`: multi-line replacement blocks, where the
  behavior change is the second line of a block that also rewrites the
  comment above it (#7556).
- `authored-py-pytest-guard-arms`: RIPR-SPEC-0260 examples 1 to 5, one module
  per example so the guard, the observed input and the edited arm are the only
  variables (an `or` guard, a rebound parameter, `True == 1`).
- `authored-py-pytest-self-reach`: RIPR-SPEC-0201 examples 1 to 6, the
  same-class transitive-reach limitation, including the six-hop chain, a class
  of the same name in another module and a test that calls the helper
  directly. Every module and class name is unique across the subject, because
  a shared name makes the analyzer relate the wrong module's tests.
- `authored-py-pytest-shapes`: language shapes. Dataclass defaults, `@property`,
  `@classmethod` and `@staticmethod`; decorator factories and wrappers;
  `asyncio.run` coroutines; list, dict and generator comprehensions and a
  generator function; dunder methods (`__add__`, `__eq__`, `__lt__`,
  `__repr__`, `__str__`, `__bool__`); a context-manager class; `match`
  statements; and relative imports through a package `__init__` re-export.
- `authored-py-pytest-idioms`: test-library idioms beyond the first seven.
  `MagicMock` call expectations, `patch`, `monkeypatch.setenv`, doctests
  collected with `--doctest-modules`, `pytest.approx`, `pytest.raises(match=)`,
  `skip` and non-strict `xfail`, `caplog`, `parametrize` with ids, a Hypothesis
  round-trip property whose oracle calls the changed function, a
  `unittest.TestCase` with `setUp` run under pytest, and an autouse fixture.
- `authored-py-pytest-hostile`: inputs that stress file, name and syntax
  handling. Non-ASCII identifiers and strings, tab indentation, a file with no
  final newline, a 488-character line, a function named like a builtin, a
  conditional definition (live and dead branch), `from m import *` through a
  module whose `__all__` exports one name and omits another, a second module
  without `__all__` whose public name is imported and underscore name is not,
  a test that asserts nothing, a CRLF test file asserting
  through a helper in another file, a module-level lambda, `globals()`
  dispatch, a module constant read by a test class, the same function name in
  two modules, a nested function, and an unrelated unparseable module.
- `authored-py-pytest-route-clients`: a framework-free route registry and
  `TestClient`, with route owners whose request client is a conftest fixture
  over the owner's own app, a fixture over another module's `app` on the same
  path, a client built over a factory call, and a fixture built on a second
  fixture. Each test only asserts the response is not `None`, so every mutant
  survives and the cases score how the client's application is read, not the
  oracle (run under Python 3.13.16 and pytest 9.1.1).
- `authored-py-pytest-literals`, `authored-py-pytest-imports`,
  `authored-py-unittest-almost`, `authored-py-unittest-raises` and
  `authored-py-pytest-conjuncts`: one RIPR-SPEC-0238 rule family each (literal
  comparisons; rival imports and `mocker.patch`; `assertAlmostEqual`; callable
  and bound exception assertions; `and`/`or`/`not` chains). Cases are
  one-line edits or rewrites labeled by real pytest runs (Python 3.13.16,
  pytest 9.1.1, pytest-mock 3.17.0).
- `authored-py-pytest-offsets`: RIPR-SPEC-0238 rule 16's constant offset
  operand (`LIMIT - 1`, `LIMIT + 1`, `threshold - 1` from a parametrize row).
  Where a test calls the owner at the offset boundary value and pins the
  result, the case is credited; where its inputs miss that value, it is a
  gap.
- `authored-py-pytest-channels`: RIPR-SPEC-0238 rule 17's written-channel
  forms: `capsys`/`capfd` output, `write_text` on an owner parameter, and a
  mock parameter's call. Each owner is one channel write; the cases vary
  whether the test's read matches that channel (a read before the owner,
  stderr read as stdout, the other parameter's file, and weak or partial mock
  assertions are gaps; exact reads after the owner are credited).
- `authored-py-pytest-bands`: RIPR-SPEC-0238 rule 21's `approx` band
  abstentions (positional band within and outside the band, spread band,
  non-literal band, longer owner body, method owner, decorated owner). Every
  case reads `static_unknown`; the outside-band edit is the one case whose
  test catches the change.

Each case is a one-line edit under `cases/` (the two `py-blocks-*-second-line`
cases replace a comment and the line under it), labeled with what the project's
own tests discriminate: the edit (and, for a rewrite, each listed mutant on
top) was applied to a scratch copy and the case's test command run under
Python 3.11.15, pytest 9.1.1, Hypothesis 6.168.3 and, for the numpy
subject, numpy 2.1.3.
The newest subjects (shapes, idioms, hostile) were run under Python 3.13.16
and pytest 9.1.1; the idioms subject's `pyproject.toml` adds `--doctest-modules`.
The two no-`__all__` star-import cases were labeled under Python 3.12.14 and
pytest 9.1.1. The public-name edit fails its exact-value assertion; the
underscore-name edit leaves the tests passing, including the assertion that
the underscore name is absent from the star-import namespace.

`authored-py-grid-<operator>` (seven subjects: boundary, equality, logical,
arithmetic, returns, delete, constant) cross each mutation-operator class with
five test styles (exact pin, parametrized table, property loop, helper-wrapped
assert, no assertion; returns adds a startswith case for a partial result),
one function per style. `authored-py-trap-kit` holds false-credit traps, tests
that look like they check the change but cannot notice it (`if False:`,
`skip`, non-strict `xfail`, `importorskip` of a missing module, a platform
guard, a misnamed test, an unused helper, assertions on inputs, an always-true
tuple assert, a swallowed `AssertionError`, a tautology, a truthy-only assert,
a patched owner, a swallowing wrapper, an empty parametrize table, a mock
checked without arguments, a sibling arm), each paired with a control that does
notice the change. Truth for these comes from running the subject's pytest
suite on the edit and on every listed mutant.

## When

`cargo xtask verdict-corpus check --language python` applies each edit to a
run-owned copy, runs `ripr check --json` (Python is enabled by project-marker
detection; no subject carries a `ripr.toml`), and projects the anchored
findings to one verdict. The required gate's `verdict-corpus check-all`
checks this corpus with every other.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent, and every row must equal its file under `expected/rows/`.
Where RIPR-SPEC-0238 names a verdict that differs from today's ("today X"),
the case keeps today's verdict in its labeling observation and its reasoning
names the spec's.

## Must Not

- Run mutation testing, pytest, Hypothesis, or network access.
- Treat the rates as a population estimate for Python code.

## Case names

The Python oracle and reach rules were drafted as RIPR-SPEC-0233 and merged
as RIPR-SPEC-0238 (#6590) with the same example numbers. The case ids keep
the `py-spec0233-exNN-` prefix and the subject keeps its
`authored-py-pytest-spec0233` name, because RIPR-SPEC-0238 cites
`py-spec0233-ex21-exc-value` by id and the subject sources are pinned by
digest. Each such case cites its example in `spec_examples`, and
`spec-coverage.toml` gates those citations through `spec-examples.lock`; example 34, added at
merge, has its cases under the same prefix.

## Refreshing

When a ripr change moves a verdict, `check` fails and names each moved row.
Re-run the case's mutants against the stored subject before re-blessing with
`cargo xtask verdict-corpus bless --language python`.
