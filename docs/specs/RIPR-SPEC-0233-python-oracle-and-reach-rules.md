# RIPR-SPEC-0233: Python oracle and reach rules

Status: proposed

Owner: product / analysis

Created: 2026-10-04

Linked proposal:

- [RIPR-PROP-0001: Multi-Language Adapter Preview](../proposals/RIPR-PROP-0001-multi-language-adapter-preview.md)

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #1290 (changed-element, f-string and error-path gates, cited in code)
- #4567 (owner call through a module and result locals)
- #4765 (same-class transitive reach, RIPR-SPEC-0201)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Python stays preview under RIPR-SPEC-0028 and
  [support tiers](../status/SUPPORT_TIERS.md).
- Most rule changes remove credit: a tautology, a trivial `match=` regex,
  a substring dict or list match, and a method owner's class token stop
  crediting `exposed`; a fluent `assert_that(...)` chain moves a finding
  from `weakly_exposed` to `static_unknown`; a rival import stops routing a
  repair card to a test of another function.
- Rules 4 and 11 read every gate on the one assertion that credits. Today
  each gate is a separate "any strong test" check, so owner identity can
  come from one assertion and the whole-collection or f-string gate from
  another. Such a split finding moves from `exposed` to `weakly_exposed`
  (examples 31 and 32).
- Rule 8 (token-shaped detection) also adds limits that today's
  substrings miss: `lambda: 0` (no space after `lambda`) and a spaced call
  such as `getattr (o, n)` or `type (x)` now give `static_unknown`. Those
  findings move from their ladder class to `static_unknown`.
- Three rule changes can move a finding to a stronger class. Each restores
  a reading RIPR-SPEC-0028 already promises. Rule 4 (an owner-observing
  strong assertion credits whatever its position) follows the 0028
  revealability rule ("credited as discriminating ... only when its
  assertion observes the changed sink"). Rule 6 (the error-path gate reads
  each assertion, not the last-ranked kind) follows the 0028 Required
  Evidence line that `pytest.raises(..., match=...)` and
  `self.assertRaisesRegex(...)` are exact exception observers. Rule 8
  follows the 0028 statement that a `static_limit_kind` is emitted "when
  syntax-first analysis cannot classify"; a substring false positive such
  as `content_type(` is not such a case.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, relation,
  `static_limit_kind`, `oracle_alignment` or `alignment_reason` value.

## Problem

The Python adapter decides each related test's oracle kind and strength,
how a test reaches an owner, which static limit fires, and the final class.
RIPR-SPEC-0028 states the vocabulary, but most of the precedence lives only
in code, and three of its oracle lines contradict the code. Several rules
are order-dependent or substring-based, and both over-credit and
under-credit.

Measured on main bcb0be576 by calling the production `extract_tests`,
`related_test_candidates` and `classify_change_with_old` from a probe test
in a scratch copy (research notes and `probe.log`). The owner is
`src/subject.py`, `def parse(text): return int(text) + 1` changed from
`return int(text)`, unless the row says otherwise. Tests import `parse` from
`src.subject`.

| Test snippet | Today: kind / strength, class | What it pins |
| --- | --- | --- |
| `assert parse('1') == parse('1')` | `exact_value` / strong, `exposed` | nothing; both sides run the same code |
| `assert parse('1') == 2` then `assert other == 3` | `exact_value` / strong, `weakly_exposed` (orthogonal) | the owner result exactly |
| the same two asserts in reverse order | `exact_value` / strong, `exposed` | the same |
| `raise ValueError` to `KeyError`; `pytest.raises(KeyError, match='.*')` | `exact_error_variant` / strong, `exposed` | the class, as a bare `raises` does |
| same change; test_a `raises(KeyError, match='empty')`, test_b `assert parse('1') == 1` | `weakly_exposed` | the raised class and message |
| same tests with the bodies swapped between the names | `exposed` | the same |
| `fmt` f-string `"ID:{n}"` to `"NO:{n}"`; test_a `assert len(fmt(7)) == 4`, test_b `fmt(7)` then `assert other == "x"` | `exposed` (inferred from `sink_alignment.rs`) | only the length, which did not change |
| `assert_that(parse('1')).is_equal_to(2)` | no assertion, `weakly_exposed` | the result, through an opaque helper |
| `'port': 8080` to `80`; `assert build()['timeout'] == 8080` | `exposed` | only the sibling key |
| `class Cart: def total`; `assert other.total() == Cart.LIMIT` | `exposed` | a method on an unrelated receiver |
| `from src.other import parse`; `assert parse('1') == 2` | `syntactic_call`, `weakly_exposed` | another module's function |
| changed `return content_type(x)` | `static_unknown` (`metaprogramming`) | n/a: `type(` substring |
| `import os`; changed `return pos.x + 1` | `static_unknown` (`missing_import_graph`) | n/a: `os.` inside `pos.` |
| changed `return 'lambda x'` | `static_unknown` (`unsupported_syntax`) | n/a: text in a string |
| `client.patch('/x')` in the related test | `static_unknown` (`mocked_module`) | n/a: an HTTP call, not `mock.patch` |

The same run confirmed the rows the rules below keep (examples marked
"unchanged"); the second and third probe batches were re-run for this spec
on the same scratch copy. Rows and examples marked "inferred" come from
reading the code, not from a probe.

Three lines of RIPR-SPEC-0028 disagree with the code. 0028 calls
`assertNotEqual` an exact-value oracle, `!=` a "smoke-style ... broad"
oracle, and `isinstance` a "broad-type" oracle. The code gives all three
`relational_check` / weak, and no broad-type kind exists.

RIPR-SPEC-0107 defines the error-path gate for Rust and excludes Python.
The Python gate (`classify.rs`, `error_path_oracle_ok`) reads a single
`strongest_kind`: the last related test among those of the highest
strength, after sorting by relation rank, strength, file and test name.
Within one test, `strongest_assertion` likewise keeps the last assertion of
the highest strength, and sink alignment reads only that assertion's text.

## Behavior

### One authority

`python/oracles.rs` stays the single classifier of Python assertion kind
and strength. `python/related_tests.rs` stays the single relation
authority, `python/static_limits.rs` the static-limit authority,
`python/sink_alignment.rs` the alignment authority, and
`python/classify.rs` the verdict ladder. No renderer or caller re-derives
any of them. Python reads fixed strengths; the `[oracle]` policy settings
(`mock_expectation_strength`, `broad_error_strength`) do not apply to
Python today.

### Assertion table

Only the test function's own body is read. The walker enters `if`, `for`,
`while`, `with`, `try`, `try*` and `match` bodies, handlers, `else` and
`finally`, and `with` item expressions. It does not enter a nested `def`,
`class` or `lambda`, and it does not follow a helper function. An
`await`ed call is not read.

`assert <expr>`, first match wins:

1. a comparison with any `==` in its chain: `exact_value` / strong;
2. any other comparison (`!=`, `<`, `<=`, `>`, `>=`, `is`, `is not`,
   `in`, `not in`): `relational_check` / weak;
3. a call to `isinstance`: `relational_check` / weak;
4. any other call: the call table below, else `smoke_only` / smoke;
5. anything else (a name, `not x`, `a and b`, a subscript): `smoke_only` /
   smoke.

A call statement or `with` item, by the last dotted segment of its callee:

| Last segment | Kind / strength |
| --- | --- |
| `assertEqual`, `assertDictEqual` | `exact_value` / strong |
| `assertIn`, `assertRegex`, `assertNotEqual` | `relational_check` / weak |
| `assertTrue`, `assertFalse` | `smoke_only` / smoke |
| `assertRaisesRegex` | `exact_error_variant` / strong |
| `assertRaises` | `broad_error` / weak |
| `raises` as `pytest.raises` or bare `raises`, with `match=` | `exact_error_variant` / strong |
| the same without `match=` | `broad_error` / weak |
| `assert_called`, `assert_called_once`, `assert_called_with`, `assert_called_once_with`, `assert_any_call`, `assert_has_calls`, `assert_not_called` | `mock_expectation` / medium |
| other segment starting `assert_`, or `assert_that` | `unknown`, shape `unknown_custom_helper` |
| anything else | no assertion recorded |

Strength ranks are strong 5, medium 4, weak 3, smoke 2, unknown 1. Only
strong credits `exposed`. A test with no recorded assertion reports
`unknown`, rank 1, and reads reach-only. The oracle shape (output, status
code, field, boundary, exact, exception, mock, helper) is evidence only and
never changes kind or strength.

This table is the existing behavior and is normative, with rules 1 to 3
below as the only changes.

### Oracle admission rules

1. **Duplicative equality is not exact.** An `==` comparison, or an
   `assertEqual` or `assertDictEqual` call, whose two compared operands
   are the same expression after whitespace normalization assigns
   `relational_check` / weak. It cannot discriminate a change to the code
   both sides run. This mirrors RIPR-SPEC-0231 step 3.
2. **A trivial message pattern is a broad error.** A `pytest.raises`
   `match=` value, or an `assertRaisesRegex` expected-regex argument,
   that is a string literal (any `r`, `b` or `u` prefix) whose content is
   empty or is one of `.*`, `.+`, `^`, `$`, `^.*$` or `(?s).*` assigns
   `broad_error` / weak. It pins only the exception class, as the call
   without the pattern does. A pattern that is not a literal keeps
   `exact_error_variant` / strong.
3. **A fluent helper chain is a custom helper.** A call statement whose
   callee chain contains a call whose last segment starts with `assert_`
   or equals `assert_that` (`assert_that(x).is_equal_to(y)`) assigns
   `unknown`, shape `unknown_custom_helper`, when the table gives no
   other kind for the outermost call. Today only the outermost segment is
   read, so the chain records no assertion and the owner reads as a gap.

### Per-test oracle selection

4. **Every strong assertion is read.** A related test's reported strength
   is the highest strength among its assertions (unchanged). Sink
   alignment reads every strong assertion of every strong oracle-eligible
   related test, not one text per test. The credit branches keep their
   order (see Sink alignment). A branch credits only when one assertion
   satisfies all of it: the branch's own token or call test, the owner
   identity check (module or class identity, read from the imports of the
   test that holds that assertion), the dict and list changed-element gate (rule 10), the
   f-string length gate (rule 11) and, for an attribute write, the
   co-observation check. No gate may be satisfied by a different
   assertion or test. The reported `oracle` text, kind and
   `observed_sink` come from the first crediting assertion in related-test
   order, then source order. When none credits, they come from the first assertion of the
   highest strength. The class never depends on assertion order or test
   names. This adds credit where an owner assertion was hidden by a later
   one, and removes it where today's gates were met by two different
   assertions.

### Error-path gate

5. **Python error-path family.** A changed line is in the error-path
   family when it is `raise` or starts with `raise `, `try:`,
   `except ` (with a space), `except* ` or `finally:`, or is a `with`
   line containing `raises(` (`probe_shape.rs`). A bare `except:` is not
   in the family. This is the existing classifier and is normative.
6. **The error-path gate reads one assertion.** For an error-path change,
   `exposed` needs one assertion of kind `exact_error_variant` in an
   oracle-eligible related test that itself satisfies a credit branch
   under rule 4. A strong value assertion never satisfies the gate, and
   `broad_error` is weak, so it never credits. When no such assertion
   exists and the finding has a strong oracle, it reads `weakly_exposed`
   with the strong-but-orthogonal stage states. The gate no longer reads
   the single `strongest_kind`.

### Reach order

A test file is a file whose name starts with `test_` or ends with
`_test.py`, or any path component is `tests` or `test`. A test file
contributes only tests and any other file only owners, so `conftest.py`
and helper modules under `tests/` are never owners. Fixture bodies are
never resolved. The changed line belongs to the narrowest enclosing owner.

Relations, first match wins (rank, oracle-eligible):

- R0: a dunder method owner takes the RIPR-SPEC-0028 dunder path;
- R1: `syntactic_call` (5, eligible): `name(` at an identifier boundary
  outside comments and strings and not a `def` or `class` header,
  `Qualified.name(`, or, for a method or class-method owner, `.name(` on
  any receiver;
- R2: `import_alias_call` (4, eligible): `from M import name as alias`
  then `alias(`, a module import that reaches the owner module then
  `alias.name(`, or a submodule receiver by full dotted path;
- R3: `api_client_route_call` (4, eligible): `client.<verb>(` whose first
  argument is a string literal equal to one of the owner's static route
  paths;
- R4: `construct_call` (4, eligible): a `__call__` owner invoked as
  `Class(...)(...)`;
- R5: `local_binding` (4, eligible): a `__call__` owner invoked through
  one `x = Class(...)` bound once;
- R6: otherwise the test must reference the owner (RIPR-SPEC-0028), or
  the test is unrelated;
- R7: `same_stem` (3, heuristic), `test_name_similarity` (2, heuristic),
  `fixture_name` (1, heuristic).

This order is the existing behavior and is normative. A callable passed
by name, as in `self.assertRaisesRegex(E, r, f, '')` or
`pytest.raises(E, f, '')`, is a reference without a recognized call
shape, so it reaches R7 only. That is what RIPR-SPEC-0028 prescribes:
a test relates "only when the test references the owner", proximity
"only rank[s] a test that already references the owner without a
recognized call shape", and heuristic links "must not promote unrelated
assertions to strong revealability". Its "token-aware" direct call rule
does not make a callable argument a call. Rule 7 changes what R1 admits.

7. **A rival import does not relate.** A bare `name(` does not relate a
   free-function owner when the test file binds `name` only by
   `from M import name` (or `as name`) and `M` resolves to another
   workspace module that defines `name`, under the RIPR-SPEC-0028 module
   identity rules. That import is a local binding for the reference check
   of R6. A bare call with no import of the name keeps R1.

### Static limits

The first match wins; any limit gives `static_unknown` with reach and
observe read from the relations and infection and propagation `unknown`.
This order is the existing behavior and is normative:

- L1: `dynamic_dispatch` from the changed line;
- L2: `missing_import_graph` for `importlib.import_module(` or
  `__import__(`;
- L3: `metaprogramming` from the changed line;
- L4: `decorator_indirection` for a dynamic route decorator;
- L5: `decorator_indirection` for any owner decorator that is not
  transparent. Transparent are `staticmethod`, `classmethod`, the
  internal `async_def` marker, a static route decorator, a `click` or
  `typer` command, group, option, argument or callback decorator, and a
  `<receiver>.command` or `<receiver>.callback` decorator when the owner's
  module imports `typer` and binds that receiver;
- L6: `mocked_module` from any related candidate, heuristic ones included;
- L7: `property_based_test`: an eligible candidate with a `given` or
  `example` decorator and no strong assertion;
- L8: `unresolved_pytest_fixture`: every eligible candidate is a
  non-parametrized pytest test that calls the owner and uses a
  non-auxiliary fixture in its body;
- L9: `opaque_custom_assertion_helper`: an eligible candidate has a
  custom helper assertion and no eligible candidate has a strong assertion;
- L10: `missing_import_graph` for a changed line that uses a name the
  owner's own module imports (mock constructors and the click, typer and
  `sys` output calls are exempt);
- L11: `unsupported_syntax` for `lambda`;
- L12: the implicit dunder dispatch limit (RIPR-SPEC-0028), then
  `python_transitive_reach_unresolved` on `no_static_path`
  (RIPR-SPEC-0201).

8. **Detection is token-shaped.** Every line check above reads tokens
   outside string literals and comments, never raw substrings:
   - `getattr`, `setattr` and `type` count only as a call of that bare
     name: the character before is not an identifier character or `.`,
     and `(` follows after optional spaces (the existing
     `contains_python_call_shape`). `content_type(x)`, `my_getattr(x)`
     and `x.type(1)` do not count; `getattr (o, n)` now counts.
   - a subscript call counts when `]` is followed by `(` outside strings.
   - `__getattr__` counts as a whole identifier.
   - `lambda` counts as a keyword token followed by a space or `:`, so
     `'lambda x'` does not count and `lambda: 0`, missed today, does.
   - an import alias counts at an identifier boundary: the character
     before is not an identifier character or `.`, and `(` or `.`
     follows. `pos.x` does not match alias `os`, and `score(` does not
     match alias `re`.
   - `mocked_module` body calls count for `patch(`, `patch.object(` and
     `patch.multiple(` only when the callee is bare or its receiver is
     `mock`, `unittest.mock`, `mocker`, or a name the test file imports
     from `unittest.mock` or `mock`. `client.patch(` does not count. The
     `monkeypatch` forms and decorator rule are unchanged.

### Sink alignment

The credit branches are, in order: `direct` by owner identity token;
`direct` by a method call on a bound receiver; `alias`; `direct` through a
module-qualified call or a result local that is an asserted operand
(RIPR-SPEC-0028, #4567); `changed_sink_token` from the changed-line token
delta; else `orthogonal`. A free-function owner's identity and delta-token
credit need module identity. A `<module>` owner with no usable token
counts as observing. This is the existing behavior and is normative, with
rule 4 and the rules below.

9. **A method owner's class token is not identity alone.** For a method
   or class-method owner, the class name in the assertion does not
   credit the identity branch. Such an owner credits `direct` only
   through the bound-receiver branch (`Cls.m(`, `Cls(...).m(`, or
   `v = Cls(...)` then `v.m(`), or through `alias` or
   `changed_sink_token` as today.
10. **Changed elements match whole literals.** For a dict or list literal
    change, a changed value is observed only when it appears in the
    assertion as a whole literal token: a number not adjacent to a digit,
    letter, `_` or `.`; a string as a complete quoted literal with the
    same content. A changed key is observed by a literal subscript or
    `.get(` of that key, and a changed list index by a literal subscript
    of that index (unchanged). A whole-collection comparison counts only
    when a dict or list display is the other compared operand of the
    assertion, not when `== {` or `== [` appears anywhere in its text.
11. **The f-string length gate reads the crediting assertion.** For a
    changed f-string whose literal text changed and whose interpolations
    did not, an assertion that is a pure `len(...)` aggregate cannot
    credit any branch. Another assertion credits only when it satisfies
    the branch itself, under rule 4. Today the gate passes when any
    strong assertion is not a pure `len` aggregate, even one that does not
    observe the owner.

### Verdict ladder

No finding is produced for docstring, comment or blank changes, header-only
lines of a multi-line `def`, annotation-only changes, a new `def` header
whose body is also added, and module-scope annotation-only variables. Then
the first matching row wins (reach / observe / discriminate):

| # | Condition | Class | R / O / D |
| --- | --- | --- | --- |
| 1 | a static limit | `static_unknown` | no / no / no with no related test; else yes (eligible) or weak (heuristic only) / same / unknown |
| 2 | no related test | `no_static_path` | no / no / no |
| 3 | only heuristic relations | `weakly_exposed`, no repair card | weak / weak / weak |
| 4 | strong, credited, gates pass | `exposed` | yes / yes / yes |
| 5 | strong, credited, changed default always overridden | `weakly_exposed` | yes / weak / weak |
| 6 | strong, credited, boundary not activated | `weakly_exposed` | yes / yes / weak |
| 7 | strong otherwise (orthogonal or error-path gate) | `weakly_exposed` | yes / weak / weak |
| 8 | weaker than strong | `weakly_exposed` | yes / weak / weak |

Confidence is 0.6 for `exposed`, 0.2 for `static_unknown` and 0.4
otherwise. Python never emits `reachable_unrevealed` or
`infection_unknown`. The ladder is the existing behavior and is normative.

### Decisions

Steven delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **`assertNotEqual`, `!=` and `isinstance`.** Adopted: the code's
   `relational_check` / weak for all three, and RIPR-SPEC-0028 is amended.
   An inequality pins one excluded value, as RIPR-SPEC-0231 rule 1 holds
   for Rust. Rejected: 0028's exact-value reading, because it overstates.
2. **Per-test selection.** Adopted: rule 4, any strong assertion that
   observes the owner credits, and every gate holds on that same
   assertion. Rejected: keep one assertion per test with a fixed
   tie-break such as first in source order, because any single pick lets
   an unrelated assertion hide an owner assertion. Rejected: keep the
   gates as separate "any test" checks, because two assertions that each
   pass half the checks do not make one discriminator.
3. **Error-path gate.** Adopted: rule 6, an `exact_error_variant`
   assertion that itself credits. Rejected: port RIPR-SPEC-0107's Rust
   variant-token requirement, because the Python credit branches already
   need the owner name or a changed-line token in that assertion.
4. **`raises(...) as exc` with a value assertion on `exc.value`.**
   Adopted: unchanged, not credited. Rejected for now: promote such an
   assertion to `exact_error_variant` like the RIPR-SPEC-0106 Rust upgrade,
   because it adds credit and needs its own binding proof.
5. **Non-literal `match=`.** Adopted: stays strong, because a variable
   usually holds a real message. Rejected: weaken every non-literal
   pattern.
6. **`pytest.approx` and `len(x) == n`.** Adopted: unchanged,
   `exact_value` / strong; both pin a value, and the f-string gate covers
   the length case it cannot see. Rejected: weaken them.
7. **Unrecognized forms.** Adopted: `assertAlmostEqual`, `assertIs`,
   `assertIsNone`, `assertListEqual`, `pytest.warns`, an aliased
   `pt.raises` and an awaited mock assertion stay unrecorded (reach-only).
   Recording them adds credit no spec promises. Rejected: add them here.
8. **`mocker.patch`.** Adopted: counts as `mocked_module`, because
   pytest-mock substitutes at runtime. Rejected: limit the rule to
   `unittest.mock` receivers.
9. **Callable arguments of exception assertions.** Adopted: unchanged,
   `assertRaisesRegex(E, r, f, '')` stays a reference read by R7.
   RIPR-SPEC-0028 relates by reference and keeps proximity heuristic, and
   no spec makes a callable argument a call. Rejected: count it as an R1
   call, because that adds credit no spec promises.
10. **Non-transparent decorators.** Adopted: unchanged, `@property` and
    `@functools.lru_cache` stay `decorator_indirection`. Rejected for now:
    make `@property` transparent, because it adds credit.

## Required Evidence

- Each Problem row reads the class the rules give, through
  `classify_change_with_old` and in `ripr check --json` for a fixture.
- Swapping two assertions in a test, or two test names, never changes a
  class on the acceptance set.
- Every "unchanged" example keeps its kind, strength, relation and class.
- Golden drift lists every Python finding whose class, relation or
  `oracle_alignment` moved, split into credit gained (rules 4, 6, 8) and
  credit removed (rules 1, 2, 3, 4, 7, 8, 9, 10, 11).
- The static-limit detectors have a negative test per token rule (a
  string literal, a longer identifier, a `.` receiver).

## Non-Goals

- No new oracle kind, strength, relation, limit kind or alignment value.
- No helper body resolution, fixture resolution or import graph.
- No change to the boundary activation rule, the changed-default rule, the
  dunder rules or RIPR-SPEC-0201.
- No new relation for a callable passed by name to `assertRaises`,
  `assertRaisesRegex` or `pytest.raises` (Decision 9).
- No change to Rust, TypeScript or Perl classification.
- No claim about runtime mutation outcomes.

## Acceptance Examples

Unless stated, the owner is `src/subject.py`
`def parse(text): return int(text) + 1` (changed from `return int(text)`),
and the test is `tests/test_subject.py`, which imports each owner from
`src.subject` (`from src.subject import parse`). The error owner `perr` is
`def perr(text):` / `if not text:` / `raise KeyError('empty')` /
`return int(text)`, with the raise line changed from
`raise ValueError('empty')`, imported the same way.

1. `assert parse('1') == 2`: `exact_value` / strong, `syntactic_call`,
   `exposed`, `direct` (unchanged).
2. `assert parse('1') != 0`: `relational_check` / weak, `weakly_exposed`
   (unchanged).
3. `self.assertNotEqual(parse('1'), 2)` in a `unittest.TestCase`:
   `relational_check` / weak, `weakly_exposed` (unchanged; 0028 amended).
4. `assert isinstance(parse('1'), int)`: `relational_check` / weak
   (unchanged).
5. `assert parse('1') == 2 and other == 3`: `smoke_only` / smoke,
   `weakly_exposed` (unchanged).
6. `assert parse('1') == parse('1')`: `relational_check` / weak,
   `weakly_exposed` (today `exact_value` / strong, `exposed`). The same for
   `self.assertEqual(parse('1'), parse('1'))`.
7. `assert parse('1') == pytest.approx(2.0)`: `exact_value` / strong,
   `exposed` (unchanged).
8. `self.assertAlmostEqual(parse('1'), 2.0)` only: no assertion, reported
   `unknown`, `weakly_exposed` (unchanged).
9. `m = parse('1')` then `m.assert_called_once_with(1)`:
   `mock_expectation` / medium, `weakly_exposed` (unchanged).
10. `np.testing.assert_array_equal(parse('1'), [2])` only: `unknown`
    helper, `static_unknown`, `opaque_custom_assertion_helper`
    (unchanged).
11. `assert_that(parse('1')).is_equal_to(2)` only: `unknown` helper,
    `static_unknown`, `opaque_custom_assertion_helper` (today no
    assertion, `weakly_exposed`).
12. `assert parse('1') == 2` then `assert other == 3`: `exposed`,
    `direct`, reported oracle `assert parse('1') == 2` (today
    `weakly_exposed`, `orthogonal`).
13. `assert other == 3` then `assert parse('1') == 2`: `exposed`,
    `direct` (unchanged).
14. Error owner, `with pytest.raises(KeyError, match='empty'): perr('')`:
    `exact_error_variant` / strong, `exposed`, `changed_sink_token`
    (unchanged).
15. Error owner, `with pytest.raises(KeyError): perr('')`: `broad_error` /
    weak, `weakly_exposed` (unchanged).
16. Error owner, `assert perr('1') == 1` only: `weakly_exposed`
    (unchanged).
17. Error owner, the `raises(KeyError, match='empty')` block then
    `assert perr('1') == 1` in one test: `exposed` (today
    `weakly_exposed`).
18. Error owner, `test_a` holds the `raises(..., match='empty')` block and
    `test_b` holds `assert perr('1') == 1`: `exposed` (today
    `weakly_exposed`). With the bodies swapped between the names:
    `exposed` (unchanged).
19. Error owner, `with pytest.raises(KeyError, match='.*'): perr('')`:
    `broad_error` / weak, `weakly_exposed` (today `exact_error_variant` /
    strong, `exposed`). With `pytest.raises(Exception, match='')`:
    `broad_error` / weak, `weakly_exposed` (class unchanged).
20. Error owner, `self.assertRaisesRegex(KeyError, 'empty', perr, '')` in
    a `unittest.TestCase`: relation `same_stem`, oracle not used,
    `weakly_exposed` with no repair card (unchanged).
21. Error owner, `with pytest.raises(KeyError) as exc: perr('')` then
    `assert str(exc.value) == "'empty'"`: `weakly_exposed` (unchanged).
22. Owner `def build(): return {'port': 80, 'timeout': 8080}`, changed
    from `'port': 8080`; test `assert build()['timeout'] == 8080`:
    `weakly_exposed`, `orthogonal` (today `exposed`). Test
    `assert build() == {'port': 80, 'timeout': 8080}`: `exposed`
    (unchanged).
23. Owner `def build(): return [80, 8080]`, changed from `[8080, 8080]`;
    test `assert build()[1] == 8080`: `weakly_exposed` (today `exposed`).
24. Owner `class Cart:` / `def total(self): return 5 + 1`, changed from
    `return 5`. Test `from src.subject import Cart` and
    `assert other.total() == Cart.LIMIT`: `weakly_exposed`, `orthogonal`
    (today `exposed`). Test `c = Cart()` then `assert c.total() == 6`:
    `exposed`, bound receiver (unchanged). Test with no import,
    `other = Other()` then `assert other.total() == 6`: `syntactic_call`,
    `weakly_exposed` (unchanged).
25. `src/other.py` also defines `parse`; test
    `from src.other import parse` and `assert parse('1') == 2`: not
    related, `no_static_path` (today `syntactic_call`, `weakly_exposed`,
    measured without `src/other.py`).
    A test with no import of `parse` and the same assertion:
    `syntactic_call`, `weakly_exposed` (unchanged).
26. Owner `def label(x): return content_type(x)`, changed from
    `content_kind(x)`; test `assert label(1) == 'a'`: no static limit,
    `exposed` (today `static_unknown`, `metaprogramming`). Changed to
    `return type(x)`: `metaprogramming` (unchanged). Changed to
    `return x.type(1)`: no static limit.
27. Owner module has `import os`; changed `return pos.x + 1` in
    `def shift(pos)`, test `assert shift(p) == 3`: no static limit (today
    `missing_import_graph`). Changed `return os.getcwd()`:
    `missing_import_graph` (unchanged).
28. Changed `return 'lambda x'` in `def label(x)`: no static limit (today
    `unsupported_syntax`). Changed
    `return sorted(x, key=lambda v: -v)`: `unsupported_syntax`
    (unchanged). Changed `return my_getattr(x)`: no static limit (today
    `dynamic_dispatch`); `return getattr(x, 'a')`: `dynamic_dispatch`
    (unchanged).
29. Related test with `client.patch('/x')` and `assert parse('1') == 2`:
    no static limit, `exposed` (today `static_unknown`, `mocked_module`).
    `from unittest.mock import patch` and `with patch('os.getcwd'):`, or
    `mocker.patch('os.getcwd')`: `static_unknown`, `mocked_module`
    (unchanged).
30. Owner `@property def v(self)` in `class C`, changed body line:
    `static_unknown`, `decorator_indirection` (unchanged).
31. Owner `def fmt(n): return f"NO:{n}"`, changed from `f"ID:{n}"`.
    `test_a` asserts `assert len(fmt(7)) == 4`; `test_b` calls `fmt(7)` and
    asserts `assert other == "x"`: `weakly_exposed`, `orthogonal` (today
    `exposed`, inferred). A test asserting `assert fmt(7) == "NO:7"`:
    `exposed` (unchanged).
32. Owner `def build(): return {'port': 80, 'timeout': 8080}`, changed
    from `'port': 8080`. `test_a` asserts
    `assert build()['timeout'] == 1`; `test_b` calls `build()` and asserts
    `assert expected == {'port': 1}`: `weakly_exposed`, `orthogonal`
    (today `exposed`, inferred: identity from `test_a`, whole-collection
    gate from `test_b`).
33. Changed `return sorted(x, key=lambda: 0)`: `unsupported_syntax`
    (today no limit). Changed `return getattr (x, 'a')`:
    `dynamic_dispatch` (today no limit).

## Test Mapping

- Existing: `crates/ripr/src/analysis/language/python/python_tests.rs`
  `oracle_for_call_recognizes_all_unittest_and_mock_variants`,
  `collect_with_item_assertions_treats_pytest_raises_match_as_exact_error`,
  `contains_dynamic_dispatch_detects_registry_indexed_call`,
  `contains_metaprogramming_detects_metaclass_declarations`,
  `test_has_mocked_module_recognizes_dotted_patch_decorator`.
- Existing: `crates/ripr/src/analysis/language/python/tests.rs`
  `error_path_change_with_value_oracle_not_exposed`,
  `error_path_change_with_exception_oracle_stays_exposed`,
  `dict_changed_element_sibling_key_oracle_not_exposed`,
  `dict_changed_element_whole_comparison_stays_exposed`,
  `list_changed_element_sibling_index_oracle_not_exposed`,
  `fstring_length_invariant_change_via_len_aggregate_not_exposed`,
  `method_name_class_constructed_but_method_on_other_receiver_does_not_credit_exposed`,
  `src_layout_same_named_function_from_other_module_does_not_credit_exposed`,
  `static_limit_detection_covers_python_preview_limit_kinds`,
  `classify_change_opaque_custom_assertion_helper_fails_closed`.
- Planned: one `classify_change_with_old` test per acceptance example,
  and an order-swap test that runs examples 12, 13, 17 and 18 in both
  orders.
- Planned: a split-gate test for examples 31 and 32.
- Planned: a negative detector test per token case of rule 8.

## Implementation Mapping

- `crates/ripr/src/analysis/language/python/oracles.rs`: assertion table,
  rules 1 to 3.
- `crates/ripr/src/analysis/language/python/related_tests.rs`:
  `strongest_assertion` and `RelatedTest` oracle text (rule 4), relation
  order and rule 7.
- `crates/ripr/src/analysis/language/python/sink_alignment.rs`: per
  assertion credit (rule 4), rules 9 to 11.
- `crates/ripr/src/analysis/language/python/classify.rs`: error-path gate
  (rule 6) and the verdict ladder.
- `crates/ripr/src/analysis/language/python/probe_shape.rs`: error-path
  family (rule 5), unchanged.
- `crates/ripr/src/analysis/language/python/static_limits.rs`: limit
  order and token detection (rule 8).

## Metrics

- `python_oracle_reach_acceptance_mismatches`: acceptance examples whose
  kind, strength, relation or class differs from this spec, counted over
  both assertion orders and both test-name orders; must be zero.
