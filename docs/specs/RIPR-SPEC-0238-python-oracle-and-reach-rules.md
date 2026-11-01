# RIPR-SPEC-0238: Python oracle and reach rules

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
- #6603 (mutation evidence for rules 12 to 14, from the #6597 corpus)
- #6599, #6600, #6601 (false actionable, false `exposed` and Hypothesis
  fixture verdicts in the #6597 corpus; rules 15 to 19)
- #10936 (numpy.testing and assertpy equality helpers; rule 28, example 10)
- #6733 (a removed term that is zero at every asserted input; rule 20)
- #9185 (a message-only change under a literal pattern that fits both
  messages; rule 2)
- #7586 (a mock passed for a called parameter, and the wrapped-argument
  gate; rule 17, example 9)

Linked PRs:

- #6750 (rules 15 to 19)

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
- Rules 12 to 14 add credit that no earlier spec promised. Each rests on
  runtime evidence instead: in the Python verdict corpus (#6597, #6603)
  the test shape of examples 8, 20 and 21 fails on a non-equivalent
  mutant of its owner, so today's `weakly_exposed` is a false actionable
  verdict there. Those findings move to `exposed`.
- Rule 28 adds credit that no earlier spec promised, and only where the
  test file proves the helper is numpy's or assertpy's. A numpy.testing
  `assert_array_equal` / `assert_equal` or an assertpy
  `assert_that(..).is_equal_to(..)` call that fails exactly when its two
  operands differ moves from `static_unknown` to the class the equivalent
  `assert a == b` reads (example 10, #10936). Every unproven name keeps
  `static_unknown`.
- Rules 15 to 19 (amended 2026-10-05) come from the same corpus (#6599,
  #6600, #6601). Rules 15, 16, 17 and 19 add credit where the corpus
  case's test fails on a non-equivalent mutant of its owner, so the
  uncredited `weakly_exposed` is a false actionable verdict: an `and`
  chain of exact comparisons, a boundary written as a constant plus or
  minus an integer, an assertion on the stream, file, mock call or field
  the changed line writes, and a Hypothesis `@example` row. Rules 18 and
  19 remove credit: an expected value computed through the changed owner,
  and a property test whose generated inputs carry the boundary.
- Rule 20 (2026-10-12, #6733) removes credit: a removed or added call term
  that every strong related call evaluates to `0` moves from `exposed` to
  `weakly_exposed`, as an unactivated boundary does.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, relation,
  `static_limit_kind` or `oracle_alignment` value. Rule 17 adds one
  `alignment_reason` token, `strong_oracle_observes_changed_effect_channel`
  (`direct`); RIPR-SPEC-0028 lists it.

## Problem

The Python adapter decides each related test's oracle kind and strength,
how a test reaches an owner, which static limit fires, and the final class.
RIPR-SPEC-0028 states the vocabulary, but most of the precedence lives only
in code, and three of its oracle lines contradict the code. Several rules
are order-dependent or substring-based, and both over-credit and
under-credit.

Measured on main bcb0be576 by calling the production `extract_tests`,
`related_test_candidates` and `classify_change_with_old` from a probe test
in a scratch copy; the probe scripts were not committed. The owner is
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

1. a comparison with any `==` in its chain: `exact_value` / strong
   (rule 1 narrows this to chains whose operators are all `==`);
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
| `assertListEqual` | `unknown`, shape `unknown_custom_helper`; the bounded standard unittest context in RIPR-SPEC-0028 promotes to `exact_value` / strong |
| `assertIn`, `assertRegex`, `assertNotEqual` | `relational_check` / weak |
| `assertTrue`, `assertFalse` | `smoke_only` / smoke |
| `assertRaisesRegex` | `exact_error_variant` / strong |
| `assertRaises` | `broad_error` / weak |
| `raises` as `pytest.raises` or bare `raises`, as a `with` item, with `match=` | `exact_error_variant` / strong |
| the same without `match=` | `broad_error` / weak |
| `assert_called`, `assert_called_once`, `assert_called_with`, `assert_called_once_with`, `assert_any_call`, `assert_has_calls`, `assert_not_called` | `mock_expectation` / medium |
| other segment starting `assert_`, or `assert_that` | `unknown`, shape `unknown_custom_helper` (rule 28 re-reads proven numpy.testing and assertpy equality) |
| anything else | no assertion recorded |

Strength ranks are strong 5, medium 4, weak 3, smoke 2, unknown 1. Only
strong credits `exposed`. A test with no recorded assertion reports
`unknown`, rank 1, and reads reach-only. The oracle shape (output, status
code, field, boundary, exact, exception, mock, helper) is evidence only and
never changes kind or strength.

This table is the existing behavior and is normative, with rules 1 to 3,
12, 14 and 15 below and the RIPR-SPEC-0028 `assertListEqual` context as
the changes (rule 15 changes row 5 for `and`).
Rule 13 changes relations and, for pytest's callable form, reads `match=`
as `broad_error`. Rule 17 reads a mock assertion
as strong for one credit branch only; its table row is unchanged.

### Oracle admission rules

1. **Duplicative or mixed equality is not exact.** An `==` comparison,
   or an `assertEqual`, `assertDictEqual` or (rule 12)
   `self.assertAlmostEqual` call, whose two compared
   operands are the same token sequence, ignoring whitespace outside
   string literals, assigns `relational_check` / weak. It cannot
   discriminate a change to the code both sides run. This mirrors
   RIPR-SPEC-0231 step 3. In an all-`==` chain only a chain whose every
   link is such a pair is weak: `fee(10) == fee(10) == 30` still pins the
   result through its last link. A comparison chain that mixes `==` with any
   other operator (`int('1') == 1 < parse('x')`) also assigns
   `relational_check` / weak: the operand the owner result sits in may be
   compared only by the other operator.
   An expectation computed from the code under test is the same emptiness
   behind a binding or a rewrite (#11531). Once workspace owners and tests are
   known, an exact comparison of two operands is also weak when the workspace
   owners at the root of one side's value (after reading the test's local assignments, the
   `setUp`/`setUpClass`/`setup_method` attributes and class attributes of its
   class and same-file bases, module constants and fixture bodies) are reached
   by the other side's calls, closed over the owners' own bodies (a forwarder
   `def g(x): return f(x)`): `self.expected = f(2)` then
   `assertEqual(f(2), self.expected)`, `EXP = f(2)`, `e = f(2)`,
   `assertEqual(f(2), g(2))`, `assertEqual(f(2), f(2) + 0)` and
   `assertEqual(f(2), f(1) * 2)`; so is a side that is an owner call minus,
   divided by, or xor-ed with the same call (`f(2) - f(2)`). The read is over
   the expression text, not a form allow-list, so an unrecognized expression
   that mentions the owner demotes; a literal, an independent name, or a call
   of a function unrelated to the other side keeps its credit. Only a call at
   the root of a side's value counts: a call inside the arguments of an owner
   call is an input of that call, neither a root nor expanded through bindings,
   so an input both sides read (`n = make_n()` then `f(n) == n * 2`,
   `parse(t) == reference_parse(t)`) keeps its credit. A dotted call resolves
   to an owner only through an imported name, `self`/`cls`, or a receiver the
   test built from an owner call; `d.get('k')`, `''.join(..)` and `f(x).get(..)`
   do not match a same-named owner. Known limits, all keeping credit rather
   than demoting: bindings and forwarder bodies are followed to a fixed depth
   of four; a test file that does not parse contributes no bindings; owner
   matching is by name, so same-named methods of different classes reached
   through an imported or `self` receiver are not told apart. The pass is
   `derived_expectation.rs`, pinned by `derived_expectation_tests.rs` and the
   `py-derived-*` corpus cases.
2. **A trivial message pattern is a broad error.** A `pytest.raises`
   `match=` value, or an `assertRaisesRegex` expected-regex argument,
   that is a string literal (any `r`, `b` or `u` prefix) whose content is
   empty or is one of `.*`, `.+`, `^`, `$`, `^.*$` or `(?s).*` assigns
   `broad_error` / weak. It pins only the exception class, as the call
   without the pattern does. A literal pattern containing an unescaped
   `|` outside a character class (`[...]`) also assigns `broad_error` /
   weak, because an alternation can admit
   both the old and the new message (`'empty|blank'`). The
   expected-regex argument is read positionally or as the
   `expected_regex=` keyword. A pattern that is a bare name the test file
   assigns exactly once, to a plain string literal, is read as that
   literal (`pattern = '.*'` then `match=pattern` is `broad_error`); a
   name assigned more than once in the file is not resolved. Any other non-literal pattern keeps
   `exact_error_variant` / strong (Decision 5).
   Change-aware reading (#9185): when the changed line is a message-only edit
   of one `raise Exc("...")` (same exception expression, one literal string
   argument on each side, no f-string), the old and the new message are both
   known. If EVERY strong `exact_error_variant` assertion of the strong related
   tests is a `pytest.raises(E, match=...)` or `assertRaisesRegex(E, ...)` with
   a literal pattern made of literal characters (`.` is read as the regex
   wildcard) that matches both messages, the patterns pin only the exception
   class and the finding is `weakly_exposed` with missing discriminator
   `message "<new message>"`. One pattern that rejects the old message, regex
   syntax in a pattern, a non-literal pattern, a comparison of `str(exc.value)`,
   an f-string message and an exception class change fail open and keep the
   existing verdict.
   The two-step form reads the same way: when the changed line is a plain
   `name = "..."` assignment (one name, one string literal on each side, the
   same name) and the owner function mentions `name` as a whole identifier
   exactly twice, the assignment and one later single-line
   `raise Exc(name)` with no cause and no keyword, the message is resolved
   through that raise and the rule above applies to the old and the new
   literal. Any other mention of the name (a rebind, a second use, a
   transforming call, a parameter, a nested scope, a raise before the
   assignment), a dynamic scope read (`locals`, `vars`, `globals`, `exec`,
   `eval`, `setattr`) or a non-function owner leaves the message unresolved
   and keeps the existing verdict (#9185, the attrs `default_if_none`
   instance).
3. **A fluent helper chain is a custom helper.** A call statement whose
   callee chain contains a call whose last segment starts with `assert_`
   or equals `assert_that` (`assert_that(x).is_equal_to(y)`) assigns
   `unknown`, shape `unknown_custom_helper`, when the table gives no
   other kind for the outermost call. Today only the outermost segment is
   read, so the chain records no assertion and the owner reads as a gap.
   Rule 28 re-reads the numpy.testing and assertpy forms of this shape as
   equality when the test file proves whose they are.

### Per-test oracle selection

4. **Every strong assertion is read.** A related test's reported strength
   is the highest strength among its assertions (unchanged). Sink
   alignment reads every strong assertion of every strong oracle-eligible
   related test, not one text per test, among the assertions the
   RIPR-SPEC-0224 family filter admits for the changed line: a
   `pytest.raises` call never credits a changed `return`, whatever its
   body calls. The credit branches keep their
   order (see Sink alignment). A branch credits only when one assertion
   satisfies all of it: the branch's own token or call test, the owner
   identity check (module or class identity, read from the imports of the
   test that holds that assertion), the dict and list changed-element gate (rule 10), the
   f-string length gate (rule 11) and, for an attribute write, the
   co-observation check. No gate may be satisfied by a different
   assertion or test. The finding's oracle kind and `observed_sink` come
   from the first crediting assertion, branches in order, then
   related-test order, then source order; each related-test row and its
   `test_oracle` evidence keep the RIPR-SPEC-0224 selected assertion.
   When none credits, they come from the strongest related oracle (the last of equal
   strength in related-test order, as before this rule). The class never depends on assertion order or test
   names. This adds credit where an owner assertion was hidden by a later
   one, and removes it where today's gates were met by two different
   assertions. An assertion's text for every gate excludes its failure
   message. For `assert` it is the expression before any `, message`.
   For `assertEqual`, `assertDictEqual`, `assertAlmostEqual` and the
   other comparison calls it is the compared operands, positional or
   named (`first=`, `second=`), not a `msg` argument or keyword. An
   exception assertion compares no operand, so its text is the whole
   call (and, for a `with` item, its body) less any `msg`: the callable
   of rule 13 and its arguments count, and a rule 14 promotion uses the
   joined text that rule defines. An owner call that appears only in a
   failure message observes nothing.

### Error-path gate

5. **Python error-path family.** A changed line is in the error-path
   family when it is `raise` or starts with `raise `, `try:`,
   `except ` (with a space), `except* ` or `finally:`, or is a `with`
   line containing `raises(` (`probe_shape.rs`). This is the existing
   classifier and is normative, with one addition: a bare `except:` line
   is also in the family. Today it is not, so broadening
   `except ValueError:` to `except:` can be credited by a normal-path
   value assertion that never runs the handler.
6. **The error-path gate reads one assertion.** For an error-path change,
   `exposed` needs one assertion of kind `exact_error_variant` in an
   oracle-eligible related test that itself satisfies a credit branch
   under rule 4. A strong value assertion never satisfies the gate, and
   `broad_error` is weak, so it never credits. When no such assertion
   exists and the finding has a strong oracle, it reads `weakly_exposed`
   with the strong-but-orthogonal stage states. The gate no longer reads
   the single `strongest_kind`.

### Reach order

A test file is a file whose name starts with `test_`, ends with
`_test.py`, or is exactly `tests.py` or `test.py` (the Django `startapp`
convention), or any path component is `tests` or `test`. A test file
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
  paths, ignoring any query string. A whole-segment path parameter
  (braces or angle brackets, with or without a converter) matches one
  concrete segment the router accepts for its converter: untyped and
  `str` take any non-empty segment, `int`, `float` and `uuid` validate
  the value, and `path` or any unknown converter stays unmatched. The
  literal must be the whole first argument (`"/items" + x`, `"/items" % x`,
  `"/items".format(..)` and adjacent literals name another path), in the test
  call and in the route decorator alike (a decorator whose path is not a
  whole literal is a dynamic route registration, a static limit). The
  receiver is `client`, or a name bound once by assignment or `with ... as`
  from `client`, another receiver or a `TestClient(...)` construction (any
  alias chain length; an assignment, annotation, augmented assignment, tuple
  or `for`/`with`/walrus target, `def` or `import` of the same name anywhere in
  the body withdraws it; a call wrapped over lines or written `app = x` reads
  like its one-line form), or an
  `AsyncClient(...)` built over an application (`app=` or an
  `ASGITransport`). A client built over an application name imported from a
  module that is not where the owner's application comes from is a rival and
  does not relate: the owner's file imports the receiver from a module (the
  test must import it from that module), or defines it itself (the test must
  import it from that module or a package above it) and so is one built over
  another symbol of the owner's module. A client's application comes from the
  test body or, for a fixture parameter, from the nearest `@pytest.fixture` of
  that name (the test's module, then `conftest.py` up the directory chain)
  whose every `return` or `yield` builds a client over one imported name. Only
  a client built over the owner's own application is `api_client_route_call`.
  A client whose application is not resolved (a fixture it cannot read or that
  builds on another fixture, an unresolved, dotted or called application name
  such as a factory, a rebound name, a router an application includes under
  another name, a decorator on a receiver the adapter does not recognise) that
  still calls the route's path and verb is `api_client_route_unidentified_app`
  (2, reach only, never an oracle) and reads as the static limit
  `missing_import_graph`, not as a gap or a credit (#10110);
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
does not make a callable argument a call. Rules 7 and 13 change what R1
admits.

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
  pytest test that calls the owner and uses a non-auxiliary fixture in its
  body. The body starts after the colon that ends the test's signature, so
  a multi-line signature's parameter lines are not uses and a one-line body
  on the `def` line is (#10685). A direct `@pytest.mark.parametrize` argname is a value pytest
  supplies, not a fixture; a parametrize mark does not resolve the test's
  other fixtures, and an `indirect=` argname is a fixture (#9619). An
  argname whose `indirect=` value the read cannot settle (`indirect=IND`, a
  computed list, a non-boolean constant) makes no fixture edge
  (RIPR-SPEC-0028) but still counts here; only literal `False` or a
  literal list or tuple of strings that omits it (an empty one included)
  leaves the argname a direct value (#10685). A mark pytest refuses at
  collection (`indirect=None`, a literal `indirect` list naming a name the
  mark does not parametrize, an argname parametrized twice within one mark,
  across stacked marks, or by the test's class or a local ancestor and the
  test) makes the whole test uncollectible: every argname of every mark on
  it is unsettled, a healthy sibling's routed fixture included, and the test
  carries the `pytest.mark.parametrize(collection error)` activation control,
  so it lends no credit through assertions, boundary inputs or fixtures
  (#10867). `indirect` is read from the
  keyword or the third positional argument; a `*args` or `**kwargs` splat in
  the mark leaves every argname unsettled. The mark is pytest's own when its
  root resolves through the imports in effect at the definition
  (`import pytest as pt`, `from pytest import mark as m`) or, with no import
  binding the root, is spelled `pytest.mark.parametrize` or
  `mark.parametrize`. An import root counts only when the module binds it
  exactly once (that import) and the enclosing class body does not bind
  it, and the module has no dynamic writer (`setattr`, `globals()`,
  `patch`). A writer name that only appears in an `import` statement
  (`from unittest.mock import patch`) is not a write; an aliased import, a
  call or a decorator using it still is (#11102). A writer inside a `test*` body or an
  undecorated module-level helper does not count, because such a body runs only
  when something calls it, after the decorators were applied; a module that
  names that function outside a `test*` or helper body (a call at import, an
  alias, an argument, a decorator; a name only another unreferenced body
  mentions does not count) keeps its writers (#11018, #11144). A canonical spelling whose root is not pinned that way
  (another library's `mark`, a rebound or class-local root) leaves every
  argname unsettled. A bare decorator name (`@cases`), the mark's callee
  (`parametrize = pytest.mark.parametrize`), argnames and `indirect` may
  each be one module-scope `NAME = <value>` binding that nothing in the
  module rebinds, bound before the test and not shadowed by the enclosing
  class body; argnames and `indirect` read through a name only a constant
  or a tuple, since a module list may be changed in place before
  collection. Any other name leaves the mark unread, so its argnames count
  as fixtures (#10685). A fixture is resolved, and does not
  count, when it hands a module-level test a fresh literal-built instance
  of a method owner's class (#9619, `py-fixtures-deposit`). The fixture
  is a single argument-free fixture decorator (`@pytest.fixture`,
  `@fixture`, an imported alias or `@pytest_asyncio.fixture`: function
  scope, no `params=`, no `autouse=`) on a synchronous def with no
  parameters whose body is an optional docstring and
  `return C(<literals>)`. `C` is the owner's class imported under its own
  name from the owner's module, and the fixture's file binds neither the
  fixture name nor `C` a second time. The test takes the fixture as a
  parameter without a default, does not rebind it, and carries no
  decorator other than a pytest mark. In its scope (the test's module and
  every `conftest.py` up its chain): every chain conftest that exists was
  read and parsed; no other fixture, renamed `name=` fixture (decorators
  included) or top-level compound statement spells the fixture name; no
  file but the fixture's own binds the name by import or assignment;
  every `from` import comes from the owner's module or pytest; and no file
  star-imports, declares `pytest_plugins` or `pytestmark`, defines a
  `pytest_*` hook, has a fixture that walks the request (`getfixturevalue`,
  `fixturenames`, `funcargs`, `node`) or rebinds a name with `global`.
  The fixture parameter then counts as a receiver bound to the owner class.
  A fixture is also resolved when its literal `params=` rows reach the
  test through `request.param` (#9619, `py-fixtures-fee-boundary`): its
  only decorator is the fixture decorator with keyword `params=` (a list
  or tuple whose rows are scalar literals or tuples of scalar literals)
  and at most a literal `ids=` list of strings, one per row; the def takes exactly
  `request` and its body is an optional docstring and
  `return request.param`; the fixture's file binds its name once and the
  scope rules above hold (without the owner-module import rule: tuple and
  scalar rows are immutable, so an imported fixture cannot change them).
  The test is a module-level pytest test with no parametrize mark, no
  Hypothesis inputs and no decorator other than a pytest mark; it takes
  the fixture as a parameter without a default, and its first statement
  after a docstring is `name = fixture` or `a, b = fixture` with distinct
  names that are not parameters and every row of the target's arity. No
  later statement names the fixture, rebinds a target name or defines a
  nested scope. The test then reads as `@pytest.mark.parametrize` over the
  target names with the rows as cases, and the unpack is not a rebinding.
  A fixture is also resolved when it is an inert test tool (#11016, a
  `click.testing.CliRunner` fixture such as pallets/click's `runner`): its
  only decorator is `@pytest.fixture` or `@pytest.fixture(scope="function")`,
  the def is synchronous and takes nothing or a lone `request` it does not
  read, and its body is an optional docstring and `return CliRunner(<literals>)`
  with `CliRunner` imported by an absolute, unaliased `from click.testing`
  (or `typer.testing`) import and bound once. The test is a module-level
  pytest test that takes the fixture without a default, does not rebind it,
  and carries no decorator other than a pytest mark. The scope rules above
  hold, and in addition no scope file or workspace module it imports (plain
  or `from`, followed through the modules those import) uses `CliRunner`
  other than to import or construct it (`CliRunner.invoke = fake`,
  `setattr(CliRunner, ...)`, an alias, or a patch string naming it), and no
  such module spells the fixture in a fixture or
  compound statement or is opaque. The runner only invokes commands, so it
  hides no discriminator or expected value.
  Any other shape keeps the limit; a third-party plugin fixture outside
  the workspace is not visible to this read;
- L9: `opaque_custom_assertion_helper`: an eligible candidate has a
  custom helper assertion and no eligible candidate has a strong assertion;
- L10: `missing_import_graph` for a changed line that uses a name the
  owner's own module imports (mock constructors and the click, typer and
  `sys` output calls are exempt, including `print(..., file=sys.stderr)`
  and `file=sys.stdout`, `py-fixtures-warning-stderr`);
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
   - `lambda` counts as a keyword token followed by a space, `:` or `(`.
     On a plain line (no `{`, backslash or triple quote) a `lambda` inside
     a string or comment does not count; on any other line it counts,
     because an f-string expression or a multi-line string edge can run
     it. `lambda: 0`, missed before, counts.
   - an import alias counts at an identifier boundary: the character
     before is not an identifier character or `.`, and `(` or `.`
     follows. `pos.x` does not match alias `os`, and `score(` does not
     match alias `re`.
   - `mocked_module` body calls count for `patch(`, `patch.object(` and
     `patch.multiple(` at an identifier boundary, except an HTTP request:
     a receiver that is `client`, `test_client`, `api_client`,
     `async_client`, `session`, `requests` or `httpx` (alone or after
     `self.`), in a test whose body and imports never mention a mock. Any
     other receiver, and any test that mentions a mock, keeps the limit
     (fail closed). The `monkeypatch` forms and decorator rule are
     unchanged.
   - `mocked_module` does not apply to a test whose every substitution is
     `with patch("<dotted.target>", return_value=<nonzero int or str literal>):`
     (`patch` bound once, from `unittest.mock`/`mock`, or `mock.patch`),
     when the test is a plain function (no decorators, defaults or
     `*args`) whose statements are only `assert <expr>`, `name = <expr>`
     and such `with` blocks of those, with expressions made of names,
     constants, operators and calls of the owner or a safe builtin (a
     `;`, a chained, attribute or subscript assignment, a helper call, a
     nested def or a default argument is not plain, so none can rebind
     names behind the patch), and the changed line, old and new, is one `return` of an f-string or `+`/`-` expression
     over names, int and string literals, the substituted zero-argument
     call (`time.time()` or `time()`) and `int(<that call>)`. The call
     appears exactly once on each side, with the same sign and the same
     `int(...)` wrapping, so the expression is `s*v + K` and an edit that
     changes `K` changes the value the assertion pins, as it would without
     the patch. With more than one call, or a changed sign, a collision at
     the fixed value (`v + v` against `v - v + 2` at `v = 1`) hides the
     edit. Products, divisions, `%`, `max`/`min`, indexing, comparisons,
     branches, other calls, an assignment on the changed line, and a zero,
     float, bool or `None` stand-in keep the limit. The patched name must
     appear in the owner only on the changed line, and the owner and its
     class must not be a segment of the target. A patch decorator,
     `patch.object`, an `as` binding, `side_effect`, `autospec`, a
     non-literal `return_value`, another mock name in the body, or an
     edit to the patched call's arguments keeps the limit. The substituted
     calls are not read as unresolved imports (`time.time()`).
   - `mocked_module` also applies when code the test does not spell runs
     around it and substitutes: an autouse fixture in the test's module,
     its class or a `conftest.py` up the directory chain; a fixture the
     test names as a parameter or a `usefixtures` mark names (test, class or
     `pytestmark`), and every fixture those request; a `setUp` or
     `setup_method` hook or a patch decorator of the test's class, a
     module setup hook, a `pytest_*` hook, or top-level code of the test's
     module or a chain conftest. Substitution means the body forms above
     plus a builtin `setattr(` call, `patch.dict(`, `sys.modules`,
     `reload(`, `__dict__` or an assignment to an attribute of an imported
     name. The read is a closed allow-list: it cannot prove a substitution's
     target unrelated to the owner, so any such form keeps the limit, while
     a fixture that uses none of them (data, `setenv`, `chdir`, `tmp_path`)
     and a patching fixture the test cannot reach keep their credit. A
     helper a fixture calls that itself substitutes is not followed.

### Sink alignment

The credit branches are, in order: `direct` by owner identity token;
`direct` by a method call on a bound receiver; `alias`; `direct` through a
module-qualified call or a result local that is an asserted operand
(RIPR-SPEC-0028, #4567); `changed_sink_token` from the changed-line token
delta; `direct` through the channel a changed write reaches (rule 17);
else `orthogonal`. A free-function owner's identity, delta-token and
effect-channel credit need module identity. An empty owner or changed-sink token
set does not itself establish observation. All owners, including `<module>` and
one-letter free functions, require an assertion that satisfies a credit branch
under rule 4. A genuine short-function output assertion can still credit the
existing module-identified call branch. With no crediting branch, a strong oracle
is `orthogonal`; without an admitted strong oracle alignment stays `unknown`.
This reverses the legacy no-token observing exception (#7526), which could
credit an unrelated strong assertion. Ordinary module-import links retain their
heuristic relation and do not gain oracle eligibility from this change.

A name, alias or changed-sink credit also requires that the asserted value can
depend on the owner's result (#6585). When every call of the owner in the
assertion sits inside a call that is not a known value-preserving wrapper
(`bool(x)`, `isinstance(x, t)`, a user helper such as `always_true(x)`, or an
expression callee such as `(f)(x)` or a subscripted callee), the finding is not
`exposed`. Value-preserving builtins (`str`, `len`, `sorted`, `round`, ...) and
grouping parentheses stay transparent. A receiver-qualified call (`other.parse(x)`)
is not counted as an owner call unless it is itself wrapped, so a foreign
same-named call neither supplies nor cancels the refusal. Pinned by the
`opaque_wrapper_*` and `value_preserving_wrapper_*` classify tests and the
`wrapped_call` unit table.

9. **A method owner's class token is not identity alone.** For a method
   or class-method owner, the class name in the assertion does not
   credit the identity branch. Such an owner credits `direct` only
   through the bound-receiver branch (`Cls.m(`, `Cls(...).m(`, or
   `v = Cls(...)` then `v.m(`), through `changed_sink_token` as today, or
   through rule 17 when the test calls the method on a local bound to its
   class. A method owner has no `alias` tokens: a class import alias is
   the class token again, so that branch no longer credits it (credit
   removed only).
10. **Changed elements match whole literals.** For a dict or list literal
    change, a changed value is observed only when it appears in the
    assertion as a whole literal token: a number not adjacent to a digit,
    letter, `_` or `.`; a string as a complete quoted literal with the
    same content. A changed key is observed by a literal subscript or
    `.get(` of that key, and a changed list index by a literal subscript
    of that index (unchanged). A changed value literal counts only when
    the same assertion does not subscript, by literal or `.get(`, a key
    or index of that collection other than a changed one; an equal value
    held by an unchanged sibling is not the changed element. A
    whole-collection comparison counts only when a dict or list display is
    the other compared operand of the assertion, not when `== {` or `== [`
    appears anywhere in its text.
    A changed element on its own line of a multi-line literal bound by
    `return` or an assignment is judged as the joined enclosing literal, so
    the same rules apply as for a one-line literal.
11. **The f-string length gate reads the crediting assertion.** For a
    changed f-string whose literal text changed and whose interpolations
    did not, an assertion that is a pure `len(...)` aggregate cannot
    credit any branch. Another assertion credits only when it satisfies
    the branch itself, under rule 4. Today the gate passes when any
    strong assertion is not a pure `len` aggregate, even one that does not
    observe the owner.

### Admissions from mutation evidence

These three rules were added on 2026-10-04 after #6603 showed that each
shape fails on a non-equivalent mutant of its owner (corpus cases
`py-spec0233-ex08-almost-equal`, `py-spec0233-ex20-raises-regex` and
`py-spec0233-ex21-exc-value` in #6597). The runtime outcome calibrates
the rule; ripr still reports only what the static shape shows.

12. **`assertAlmostEqual` pins a value.** A call statement
    `self.assertAlmostEqual(...)` assigns `exact_value` / strong, as
    `pytest.approx` does (Decision 6). At its default seven places it
    fails for any change of at least `1e-7` in the observed value. That
    holds only for the default tolerance: with an explicit `places=` or
    `delta=` (by keyword, or positionally as the third argument for
    `places` or the fifth for `delta`; the fourth is `msg`), or with a
    `*` or `**` spread that may carry either, the call is
    `relational_check` / weak, because static evidence cannot tell whether the band excludes
    the pre-change value. The corpus evidence covers the default form
    only. A change smaller than the tolerance is not discriminated, and
    ripr does not compute the size of a change; that is the same limit
    Decision 6 accepts for `pytest.approx`, whose default relative
    tolerance of `1e-6` is coarser. A literal `None` in either position
    counts as absent, since `unittest` then uses the default. The
    receiver must be `self`: `checks.assertAlmostEqual(...)` may be a
    project helper with its own tolerance, so it stays unrecorded, and
    `assertNotAlmostEqual` stays unrecorded.
13. **The callable of an exception assertion is called.** In a call
    statement whose last segment is `assertRaises` or `assertRaisesRegex`,
    or `raises` read as `pytest.raises`, the positional argument right
    after the exception class (after the expected regex for
    `assertRaisesRegex`) is the function the assertion calls when it is a
    bare or dotted name. The test relates to that owner as R1
    `syntactic_call`, under the same identity checks as a written call
    (rule 7 applies), and the assertion keeps its table row, with one
    exception: in the callable form, pytest passes keyword arguments,
    `match=` included, to the callable rather than checking the message,
    so `pytest.raises(E, f, ..., match=...)` is `broad_error` / weak, whatever
    the callable is. A lambda or any other expression in that position is
    not read as a relation. The call the callable form makes, with the
    positional and keyword arguments both frameworks pass on, binds its
    literal arguments for the boundary rule and rule 20 like a written
    call: `assertRaisesRegex(KeyError, 'big', check, 20)` places `n` at 20.
14. **A bound exception compared by value pins the error.** A `with` item
    `pytest.raises(...) as N` (or bare `raises`), or
    `self.assertRaises(...) as N` or `self.assertRaisesRegex(...) as N`
    (no other receiver), is recorded `exact_error_variant` / strong when a statement after the
    `with` statement, in the same test body, asserts `==` (an `assert`
    with one `==`, or `assertEqual`) between two operands, one of which
    is a value-preserving read of the exception: `N.value` (pytest) or
    `N.exception` (unittest), alone, as `str(...)` or `repr(...)` of it,
    or as its `.args` or a literal index of `.args`, and whose other operand
    does not name `N` (`str(N.value) == repr(N.value)` holds for any
    message). A coarser read such
    as `len(str(N.value))` or `type(N.value)` does not promote. No
    statement in between may assign `N`, or an attribute or subscript
    reached through it (`N.value.args = ('blank',)`), alone or in a tuple,
    list or starred target or by a walrus, or call a method on `N` or on
    such an attribute, or pass either to a call other than `str`, `repr`, `len`, `type` or
    `isinstance` (`setattr(N.value, ...)`), or assign a value that reads
    `N` (`msg = str(N.value)` would alias it); a compound statement in
    between also ends the search. The joined comparison text omits its failure message. A comparison inside the `with` body
    does not count: it runs only when nothing is raised. Rule 1 applies
    to that comparison first: a tautology does not promote. The value
    assertion keeps its own table row. The promoted item, its `with`
    body and that comparison form one assertion for rules 4 and 6: its
    text is the three together, so a direct owner call in the body meets
    the owner-name credit branch even when the change is only the
    exception message and the comparison holds the old message.

### Admissions from the Python verdict corpus

These five rules were added on 2026-10-05 from the Python verdict corpus
(#6597): #6599 (false actionable), #6600 (false `exposed`) and #6601
(Hypothesis inputs read as fixtures). As with rules 12 to 14, a corpus
mutant calibrates the rule; ripr reports only the static shape.

15. **An `and` chain asserts each conjunct.** `assert A and B` fails when
    either conjunct is false, so it is read as the strongest of its
    conjuncts under the table, rule 1 applying to each. The assertion
    text the rule 4 gates read holds only the conjuncts of that strongest
    strength, so an exact conjunct on something else does not credit a
    weak conjunct on the owner (`fee(10) > 0 and other == 5` does not
    observe `fee`). `or`,
    `not` and any other boolean form stay `smoke_only` / smoke, with one
    exception (#9542, RIPR-SPEC-0028): a bare `assert f(lit, ..)` or
    `assert not f(lit, ..)` whose single resolved workspace function owner
    provably returns `bool` is an exact pin, read as `f(lit, ..) == True` /
    `== False`.
16. **A constant offset is a static boundary operand.** In a changed
    comparison, an operand `C - k` or `C + k`, where `k` is an integer
    literal and `C` a literal or a module constant resolving to one, is
    the boundary value `C - k` or `C + k` for boundary activation, as a
    plain constant is today. The same shift applies to a name bound by a
    literal `parametrize` or `@example` row (`threshold - 1`). A `return` of a comparison still reads as a
    return value unless both operands are plain names or literals, so
    `return total + 1 > limit` keeps its return-value probe.
17. **An assertion on the written channel observes a changed effect.**
    When the changed line is a statement whose effect is a write rather
    than a value, a strong assertion in a related test that calls the
    owner credits `direct` with reason
    `strong_oracle_observes_changed_effect_channel` when it compares, with
    `==`, the channel that line writes. The test must call the owner by a
    module-identified name (a free function), a method owner on a
    local bound to its class, or a `@classmethod` or `@staticmethod` owner
    on that local or on its imported class, by its name or an import alias
    the test does not rebind (rule 9, #7634). A method's or classmethod's
    first parameter is its receiver whatever it is named; a staticmethod
    has none. The channels:
    - `print(...)` or `sys.stdout.write(...)`: `capsys`, `capfd` or their
      binary forms' `readouterr().out`, inline or through a local the
      test binds once to `readouterr()` after an owner call;
      `print(..., file=sys.stderr)` or `sys.stderr.write(...)`: the same
      with `.err`. The capture must be a test parameter. A capture taken
      before the owner runs holds none of its output.
    - `p.write_text(...)` or `p.write_bytes(...)` on an owner parameter
      `p`: `L.read_text()` or `L.read_bytes()` of the local `L` the test
      passes as `p`, by position or keyword. A local passed for another
      parameter is another file.
    - `p.m(...)` on another owner parameter: `L.m.assert_called_once_with(`
      for the local `L` passed as `p`, with at least one argument and
      every argument pinned. That mock assertion is read as strong for
      this credit only. An argument-free `assert_called_once()`, an `ANY`
      matcher or a starred argument accepts whatever the changed call
      passes, and `assert_called_with` (last call only) or
      `assert_any_call` can be met by another call the owner makes on
      the same method, so none of them credits. A changed `p(...)` that
      calls the parameter itself is read the same way, through
      `L.assert_called_once_with(`. When the owner's only `return` is its
      last statement, `return p`, the owner is synchronous, not a
      generator, and has no decorator but `@classmethod` or
      `@staticmethod` (#7634), which only bind the call, `p` is never rebound, and the body has no
      `lambda`, `for` or `:=` that could rebind it, a local the test binds
      once to the owner's result is the mock it passed as well, for both
      `p(...)` and `p.m(...)` (#7586). Any other decorator, or one whose
      name cannot be read, leaves the result uncredited.
      Either form credits only when the pinned expectation reads the new
      arguments as a different call. A changed argument count or keyword
      set does. So does a rewritten argument when neither side keeps the
      other inside it (`'hi'` to `'welcome'`, two literals of different
      value; `'hi'` to `"hi"` is the same value), or when one side is the
      other, a single operand, under literal arithmetic that is not an
      identity (`n + 1`, `n * 100`, but not `n + 0`, `n * 1` or `n + ''`). Any other rewrite that keeps the old
      argument inside it (`abs(n)`, `max(n, 0)`, `n or 0`, `(n)`, `n.real`,
      or `int(n)` to `n`) can return it unchanged at the input the test
      passes (`abs(1) == 1`), so the pinned expectation may still hold and
      the change does not credit. A starred argument on either side
      cannot be aligned and does not credit. An old line that is not one call
      statement (it carries a comment or assigns the result) cannot be
      compared and does not credit; a call to another callee, or a line
      with no old counterpart, does.
    - `self.a = ...` or an augmented assignment in a method owner:
      `L.a`, where `L` is a local bound to the owner's class whose
      method the test calls (rule 9's bound receiver).
    On every channel the assertion must start on or after the line of
    the test's first owner call: a read above it sees the state before
    the changed effect could run, on both sides of the change. An
    assertion with no known source line does not credit here.
    Rule 6 still applies on the error path.
    A comparison supplied by bounded assertion-helper resolution does not
    establish that an `async def` or generator owner's body ran. Such an
    owner receives no helper-derived credit, including through a method-name
    branch when the changed effect has no modeled channel. Generator refusal
    conservatively reads a whole `yield` identifier in the owner's source,
    including f-string expressions, strings and nested scopes; it does not
    claim precise generator execution. Raw assertions and mock expectations
    keep their existing rules. Synchronous function-result helpers retain
    the ordinary substituted-assertion reading.
    For helpers whose definition reads a parameter field, credit additionally
    requires a direct field compared with a literal expected operand, retained
    helper/class/module/receiver bindings, and an eager straight-line test body.
    Defining-module attribute writes join caller rebinding facts, including the
    original imported constructor name when the caller uses an import alias.
    Copies or aliases used as expected values, replaced helpers or methods,
    overwritten fields, deferred calls, definition-time calls and multiline
    call-shaped literal text do not establish field-helper credit. Supported
    field helpers use only changed-effect observation; a method-name token
    cannot restore credit. The observed receiver's own call must precede its
    helper call, including when several receivers or same-line statements exist.
    A refused helper's origin survives the reported-oracle fallback; an unrelated
    raw assertion retains its own origin. This bounded rule does not change the
    existing generic parameter/result-helper interpretation or claim general
    Python alias analysis or arbitrary helper execution.
18. **A self-computed expected value does not credit.** Following
    RIPR-SPEC-0035, an assertion does not credit when every compared
    operand that names the owner is computed with a top-level
    arithmetic operator and another operand is a call to something else:
    `assert checkout(1000) == 1000 + tax(1000)` for a changed `tax`. A
    change to `tax` moves both sides together. `assert tax(1000) + 1 ==
    81` still credits, because its other side is a literal.
19. **Hypothesis inputs are generated, not fixtures.** The parameters a
    `@given(...)` decorator supplies (positional strategies fill the
    rightmost positional parameters, keyword strategies the named ones;
    an unpairable `@given` supplies them all) are not pytest fixtures, so
    they never produce `unresolved_pytest_fixture`. `@example(...)` rows
    bind parameters like literal `parametrize` cases (positional values
    fill the rightmost parameters, as many as there are values; keyword
    values the named ones); a chained
    `@example(...).xfail()` or `.via(...)` row is skipped. A `@given`
    test gives `static_unknown`, `property_based_test`, only when it has
    assertions and either none is strong, or it has no `@example` row and
    its body reads a generated parameter. A `parametrize` mark binds
    other parameters, so it does not stand in for an `@example` row. A `@given` test with no
    assertion reads reach-only like any other test, because it has no
    oracle for the generated inputs to hide.
20. **A changed term that every asserted input makes zero is not
    activated.** When the whole difference between the old and the new
    line is one additive call term, `+ f(args)` or `- f(args)`, removed or
    added, the term changes the owner's result only for inputs where it
    is nonzero. The term must sit after an operand and before a closing
    bracket, a comma, another additive operator, a comment or the end of
    the line, so that dropping it leaves the rest of the expression as it
    was (`x - f(y) * 2` is not such a term). `f` must be a module function
    of the owner's file whose body, after an optional docstring, is
    exactly `if P OP V: return A` then `return B`, where `P` is a
    parameter, `OP` one comparison, and `V`, `A` and `B` are literals or
    module constants RIPR-SPEC-0028's boundary rule resolves. `f` must be
    the file's only definition of that name, undecorated and not `async`;
    the owner must not bind the name or take it as a parameter; and no
    related test may patch, `setattr` or assign an attribute of that name.
    On the changed line the term may not sit in a string, a comment or a
    `def` header, and the rule does not run on a line of a multi-line
    `def` header (docstring lines produce no finding at all); at the end
    of a line the line's brackets must balance with no `\` continuation. The term's argument for `P` must resolve in each strong
    owner call the boundary rule reads: a literal, such a constant, or an
    owner parameter that call binds to a literal and that the owner body
    never rebinds (a body with a `for`, a comprehension, a lambda, a
    walrus, a nested scope or `async` code has no such parameter). Every
    strong related test must contribute such a call: a strong test whose
    owner call binds nothing (`*args`, a comment inside the arguments) or
    that has no recognized owner call leaves the rule unresolved. When
    every call resolves and every one evaluates the term to integer `0`,
    the finding is `weakly_exposed` and names `f(args) != 0` as the
    missing discriminator, with the observed argument values. Any
    unresolved call, any nonzero value (a float `0.0` changes an integer's
    type, so it is nonzero here), or no analyzable call leaves the verdict
    to the other rules. The rule reads one call deep and does not run for
    a method owner. Known limits, each of which can withhold credit from a
    term a test does activate: a callee rebound by a module-level
    assignment or import rather than a `def`, a callee patched by a
    fixture, `setUp` or class decorator rather than in the related test
    itself, a string spanning several lines, brackets opened on an
    earlier line, and an owner call made through a test helper.

21. **An explicit `approx` band is evaluated where it is decidable, else
    abstains on a numeric edit.** Rule 12 reads `pytest.approx(expected)`
    at its default tolerance as pinning the value. When the assertion the
    row selects passes an explicit band, `approx(expected, rel=...)`,
    `abs=...`, a positional second or third argument, or a `*` or `**`
    spread that may carry one (a literal `None` counts as absent), the
    band may admit the pre-change value. One shape is evaluated: the owner
    is an undecorated one-statement function whose changed line is
    `return <expr>` over numeric literals, its own parameters and
    `+ - * /`, and the assertion is `<owner>(<numeric literals>) ==
    approx(<number>, rel=<number>, abs=<number>)` in either operand order
    (only `rel`, `abs` and `nan_ok` keywords). The old and new results are
    compared with pytest's tolerance (`abs` alone, else the larger of
    `rel * |expected|` and `abs`, default `1e-12`). An old result inside
    the band and a new result outside it is `exposed`
    (`py-idioms-approx-constant-outside-tolerance`); both inside, for
    every selected strong assertion and with no unbanded strong
    assertion, is `weakly_exposed` naming the band
    (`py-idioms-approx-constant-within-tolerance`, whose old reading as
    `exposed` was a false exposed). Everything else stays
    `static_unknown` with the static limit `unsupported_syntax` naming the
    band: an old result already outside the band, a result within `1e-9`
    (relative) of the band edge, a non-literal argument, a positional
    band, `**`, a longer body, a method or decorated owner, and a run
    without the old line (repo mode). The `py-bands-*` cases pin the
    abstentions: a positional band (an edit the band absorbs and one it
    rejects, the latter a documented missed credit), a `**` spread,
    named constants for the expected value and band, a two-statement
    owner, a method owner and a decorated owner.
22. **A `caplog.text` membership check on a changed log call is
    evaluated where it is decidable, else abstains.** When the changed
    line is a logging call (`<receiver>.debug`, `.info`, `.warning`,
    `.warn`, `.error`, `.critical` or `.exception` with a string literal)
    and the selected assertion is `<text> in caplog.text` sharing a word
    of three or more characters with that literal, the finding is
    `static_unknown` with `unsupported_syntax`, unless the formatted
    message can be evaluated. It can when all of these hold: the old and
    new lines are logging calls on the same receiver with a literal
    message and no keyword arguments, whose arguments are string or
    integer literals or owner parameters the test's own calls bind to
    such literals, formatted with `%s`, `%d`, `%i` and `%%` (no
    arguments: the message as written); the owner is an undecorated
    function whose lines between its `def` and the changed line are only
    plain `if <comparison>:` blocks, with nothing after the line but
    `return` or `pass`; every owner call of the test has a readable guard
    outcome; the test pins the capture level with exactly one
    `caplog.at_level(<level>)` block around its owner calls or one
    `caplog.set_level(<level>)` before them and makes no other call, no
    `caplog.clear`, no patching; the module configures no logger
    (`setLevel`, `propagate`, filters, handlers, `logging` configuration);
    and the asserted text holds a space after a character other than an
    upper-case letter, a digit or a space, so it is read from the message
    and not from the record header. A record is captured when its level is
    at least the pinned level. The asserted text in the old message and
    not in the new one (`py-idioms-caplog-message`, and a level lowered
    below the pin, `py-idioms-caplog-level`) is `exposed`; in both is a
    weak gap; a test that did not log the text before the edit, or any
    shape outside these bounds, abstains. A custom `log_format` is not
    read. The asserted literals are only those the assertion requires: a
    single membership or an `and` chain, never a literal under `or`. The
    receiver is `logging` or a module-level `name = logging.getLogger(...)`
    (a detached `logging.Logger(...)` or any other object abstains), the
    method is not `exception` (its traceback can hold the asserted text),
    and the test never rebinds `caplog`. `caplog` must be pytest's own
    fixture: the test requests it as a parameter and no `@pytest.fixture`
    in its module or a `conftest.py` up its directory chain defines a
    fixture of that name (including `@pytest.fixture(name="caplog")` and a decorator imported as an alias with
    `from pytest import fixture as fx`, unless a later import, assignment, `def` or `class`
    rebinds that name before the decorated def); otherwise the reading abstains. An assertion about
    unrelated text keeps its verdict.
    The same evaluation reads the captured records' formatted messages,
    which carry no record header: `"<text>" in caplog.messages` and
    `"<text>" in [r.message for r in caplog.records]` (or
    `r.getMessage()`) compare one whole message, and
    `any(<text> in r.message for r in caplog.records)` finds the text
    inside one message (`any(r.message == <text> ...)`, in either operand
    order, compares a whole one). Indexed or whole-list forms (`caplog.records[0].message == ...`,
    `caplog.messages == [...]`) and a filtered `any(...)` abstain when
    they share a word with the changed message.
    An absence assertion reads the same way with the outcome inverted:
    `"<text>" not in caplog.text`, `... not in caplog.messages`,
    `... not in [r.message for r in caplog.records]` and
    `not any(<text> in r.message for r in caplog.records)` pass while no
    captured text matches. A text absent from the old message and present
    in the new one is `exposed`; absent from both (including a new call
    below the pinned level, which records nothing) is a weak gap; a text
    already present in the old message means the test fails before the
    edit and is not read. An absence literal under `or` is not required,
    as for a presence literal. The membership is read from the parsed assertion, not its text layout:
    extra spacing, parentheses, line breaks, implicit literal
    concatenation, an assertion message and a membership joined by `and`
    all read the same way, while a membership against any other receiver
    is not read.
    A local alias of a captured-log read (`msgs = caplog.text`,
    `.messages` or `.records`) reads as the read itself only when the name
    is bound once, to the read, at the assertion's own indentation and after
    every owner call, before the assertion, and appears nowhere else in the
    test but as a read inside that native assertion span (a wrapped assertion
    counts as one statement). A semicolon-separated mutation or use is a
    separate statement even on the same physical line. Native store contexts,
    including walrus expressions inside f-strings and comprehension targets,
    cannot borrow the alias's captured-log evidence (#10595). An
    alias that is appended to, rebound, deleted, used outside the selected assertion,
    taken before the call or inside a nested block keeps the abstention, as
    does a name spelled inside a quoted literal (single, double or triple
    quoted), which is data. A local called `text`, `messages` or `records`
    is not confused with the attribute of `caplog.text` and the like. The
    `caplog.text` and `.messages` reads are rebuilt on each access, so an alias
    taken before the call would hold earlier records. `.records` exposes the
    handler's record list but follows the same bounded eligibility policy (#10151).
23. **An assertion that cannot run or cannot fail is not read.** The
    assertion collector skips the dead arm of an `if` or `while` whose
    condition is a literal (`if False:`, `if 0:`, `if None:`, `while False:`;
    the arm that can run is still read) and the body of a `try` whose
    handler catches `AssertionError` (a bare `except`, `Exception`,
    `BaseException`, `AssertionError`, `builtins.AssertionError`, or a tuple
    naming one) and does not raise, assert, call a `fail` helper or call a
    unittest `assert*` method. The `else` and `finally` blocks
    are still read. A swallowed or dead assertion cannot notice a change, so
    it must not credit one (`py-trap-dead-branch`,
    `py-trap-swallowed-assert`, #9691; the `py-unrun-*` cases add `if 0:`,
    `if None:` and `while False:` dead arms, tuple, bare and
    `builtins.AssertionError` handlers, and the live `else` arm, `else` and
    `finally` blocks and `fail`-calling handler that stay credited). An
    assertion after a definite exit in the same statement list (`return`,
    `raise`, `pytest.skip`/`fail`/`exit`/`xfail`, `sys.exit`, `os._exit`,
    `self.skipTest`) or in a `for` over an empty literal is dead too, and a
    test definition rebound later in the same scope (a second `def` of the
    name, `name = None`) is never collected, so it carries an activation
    control (`py-trap-exit-before-assert`, `py-trap-shadowed-test`; the
    `py-trap-conditional-exit` control stays credited).
24. **A test the collector skips is activation-controlled.** A parametrize
    decorator over an empty literal table (`[]` or `()`) and an
    unconditional module-level `pytest.importorskip(...)`,
    `pytest.skip(..., allow_module_level=True)` or `pytestmark` that applies
    a recognized skip, skipif or xfail mark control every affected test the
    same way a `@pytest.mark.skip` decorator does (a condition is not
    evaluated, so a `skipif` on a name or comparison that is false on the
    host still abstains): the finding is `static_unknown` with
    `decorator_indirection`. Two things are decidable from the source and
    leave the test active: a condition that is itself the boolean literal
    that never triggers the mark and is the only condition, given with the
    string `reason` the runner requires for a boolean
    (`skipif(False, reason="...")`, `xfail(False, reason="...")`,
    `unittest.skipIf(False, "...")`, `unittest.skipUnless(True, "...")`).
    A missing or non-string reason, a second positional condition, a
    `condition=` that overrides the positional one and a `*` spread keep
    the control, because the runner then errors, skips or xfails. The
    second decidable case is `pytest.importorskip` of one bare module name that
    every interpreter ships (`json`, `math`, `re`, ...). The adapter does
    not evaluate whether any other module is installed, so `importorskip`
    of it, or with `minversion` or `reason`, still abstains
    (`py-trap-module-skipped`, `py-trap-empty-table`,
    `py-trap-skipif-false`, `py-trap-module-loaded`, #9691; the `py-unrun-*`
    cases add a module `pytestmark` skip, a module-level `pytest.skip`, an
    `importorskip` of a missing module and an empty tuple table, which
    abstain, and `xfail(False, reason=...)` and
    `unittest.skipUnless(True, ...)`, which stay active and credited).
    A test pytest never collects is controlled the same way
    (`pytest.collection(...)`, #11527): a class or module that binds `__test__`
    to anything not provably the literal `True` (an absent binding stays
    collected and credited; an existing binding that cannot be resolved, such
    as a computed, destructured, `match`-arm or conditional one, withholds
    credit because collection is uncertain), a `conftest.py` `collect_ignore` or
    `collect_ignore_glob` entry that covers the file, and the pytest options
    `python_files`, `python_classes` (pytest-style classes),
    `python_functions` (pytest-style tests), `testpaths`, `norecursedirs`
    and `--ignore` / `--ignore-glob` in `addopts`, read from `pytest.ini`,
    `.pytest.ini`, `tox.ini`, `setup.cfg`, `pyproject.toml`
    `pytest.toml` and `.pytest.toml` on the test file's directory chain. The read is
    parse-everything-or-refuse: a collection form the adapter cannot
    evaluate (a computed `collect_ignore`, a collection hook, a character
    class, an unparsable or unreadable file, `-o`, `-c` or `--rootdir` in
    `addopts`) withholds credit for its scope. Not modelled, so credit is
    unchanged: the default `norecursedirs`, `-k`, `-m` and `--deselect`,
    `pytest_collection_modifyitems`, and command-line arguments. Known
    limits: `testpaths` is assumed to govern collection (an invocation that
    passes explicit paths bypasses it); a function-level `__test__` is not
    read; and a base class imported from another module is not resolved, so
    its `__test__` binding is unseen. A space-separated value of a known
    value-taking option (`--junitxml`, `--html`, `--log-file`,
    `--basetemp`, ...) is not treated as a positional path.
25. **A property check pins no value.** A single `==` in an `assert`
    statement is a weak relational check, as a tautology is (rule 1), when
    one side is a call and the other side is either a call that appears in
    that call's arguments (`f(f(x)) == f(x)`, either order, at any depth,
    keyword and container arguments included) or a `for` loop variable that
    appears in them (`for n in range(1, 5): assert f(1, n) == n`; `async for`
    and tuple or list targets count, `while` loops and names bound outside a
    loop do not). The calls need not be the changed owner. Idempotence holds
    for any constant return and an identity over a loop variable holds for
    `n / 1`, so neither notices an edit. An independent expected value (a
    literal, `n * 3`, a name bound outside a loop) still pins, and so does a
    chain (`f(f(x)) == f(x) == 1`). Only the `assert` statement is read:
    `assertEqual` and helper forms keep their strength
    (`py-grid-returns-property`, `py-grid-arithmetic-property`, #9594; the
    `py-props-*` cases add the reversed order, a tuple loop target and a
    loop identity scored as gaps, and a chain, a literal expected value and
    a name bound outside the loop credited).

26. **A doctest example is a test only when pytest collects it and running it
    notices the edit.** With `--doctest-modules` in the `addopts` of the
    pytest configuration (`pyproject.toml`, `pytest.ini`, `tox.ini` or
    `setup.cfg`, first configuration wins), a `>>>` example in a changed
    module's function docstring is read as the assertion it states. When no
    pytest configuration exists at all (no `pyproject.toml` pytest table of
    either form), a `tox.ini` `[testenv]` `commands` line that is exactly one
    bare `pytest`, `py.test` or `python -m pytest` run carrying
    `--doctest-modules` is the same signal; the line must hold no shell
    operator, factor condition or substitution other than an empty
    `{posargs}`, and the same flag and target refusals apply (#4561). Only the
    literal `[testenv]` section is read, one command per line (a trailing
    backslash leaves a stray token and refuses the read); a factor or named
    section such as `[testenv:docs]` overrides `commands` for that
    environment alone, so it is not evidence that the default run collects
    doctests, and is a deliberate non-claim.
    Collection is refused when the configuration holds any flag that
    reshapes it (`--ignore*`, `--deselect`, `-k`, `-m`, `-o`, `-c`, `-p`,
    `--doctest-glob`, `--import-mode`, cache selectors such as `--lf`, and
    their attached short forms), a positional collection target, any option
    this reader does not model (`norecursedirs` among them), a
    `.pytest.ini`, `pytest.toml` or `.pytest.toml` file, a native
    `[tool.pytest]` table, a `testpaths` that leaves the module out, or a
    `conftest.py` on the module's path that mentions `doctest`,
    `collect_ignore`, a `pytest_` hook, `autouse` or `pytest_plugins`, or
    one directly under a root subdirectory that defines a `pytest_` hook or
    `pytest_plugins` (its fixtures and ignore lists stay in its own tree). A bare `pytest` invocation is
    assumed; the command line CI uses is unknown. A docstring is read only
    when it belongs to an undecorated top-level `def` defined once, every
    example is one `>>>` call of a bare name with literal arguments, the
    expected line is one `repr` of a string, integer or boolean (a trailing
    `#` comment on the `>>>` line is allowed and cannot hide the comparison), and no
    `# doctest:` directive, backslash (in the decoded docstring or its
    source), `...` or `<BLANKLINE>` appears; a module-level `__test__`, a
    star import or a binding of `len` disables the read. The example must exist
    unchanged in the old source, so a diff does not write its own oracle.
    An exact-value oracle alone does not say the edit is noticed
    (`truncate("abc", 5)` is `'abc'` before and after an off-by-one cut),
    so a closed evaluator runs the example on the old and the new function
    and keeps it only when the old result equals the expected value and the
    new one differs; a function it cannot run (anything beyond a single
    `return` over integer, boolean and printable-ASCII string operations)
    drops the example; strings are capped at 4 KiB and expressions at 48
    levels so a reviewed diff cannot exhaust the analyzer. A credited
    example advertises no verification command, because pytest has no node
    for the synthetic test (`py-idioms-doctest-separator` credited,
    `py-idioms-doctest-short-input` stays a gap, #9619).
27. **An assertion helper called through a re-raising wrapper is the
    helper's assertion.** A top-level test statement
    `wrapper(lambda: helper(args))` reads as `helper(args)` when `wrapper`
    is a def of the test's own module that is the module's only binding of
    its name, is not imported or rebound in the test, and has one plain
    parameter `fn` and a body of exactly one `try`. The `try` body is one
    statement, `fn()` or `return fn()`; there is no `else` or `finally`; and
    every handler is a bare `raise` alone, because a call before it could
    skip or exit (`pytest.skip()`, `sys.exit(0)`) and a path that returns or
    falls off the end swallows. A handler's exception type must be a name,
    a dotted attribute path or a tuple of those, because the type expression
    runs when an exception arrives (`except pytest.skip('x'):` skips). The
    wrapper's name may appear in its module only as its one `def` and as
    plain calls `name(`; an assignment, `global`, import at any depth, walrus,
    `case` capture, decorator, attribute or string mention could rebind it
    where the binding scan cannot see, and `import *`, `globals(`, `vars(`,
    `exec(`, `eval(` or `__dict__` anywhere refuses the module. Another
    file that patches the module (a conftest fixture, a plugin) is not seen,
    as for every helper this module resolves. The lambda takes no
    parameters, is the only argument, and its body is one call. A wrapper
    that swallows any exception (including after a partial re-raise, a
    conditional re-raise or a `finally` that returns), re-raises a different
    exception, never calls its parameter, takes more parameters, or is
    decorated, async, imported or bound twice is not read, and the call
    keeps its reading (`py-trap-wrapped-through` credited;
    `py-trap-wrapped-swallow`, `py-trap-wrapped-partial`,
    `py-trap-wrapped-conditional` and `py-trap-wrapped-finally` stay
    non-credited, #9619).

28. **numpy.testing and assertpy equality helpers are the equality they
    run.** `numpy.testing.assert_array_equal(a, b)`,
    `numpy.testing.assert_equal(a, b)` and
    `assertpy.assert_that(a).is_equal_to(b)` fail exactly when the two
    operands differ, like `assert a == b`. Extraction still assigns them
    `unknown`, `unknown_custom_helper` (rule 3), because the name alone does
    not say whose helper it is. A pass over the test file then reads such a
    call as `assert a == b`, `exact_value` / strong, only when all of these
    hold. The call is `a`, `b` as exactly two positional operands (keywords
    only `err_msg`, `verbose` and `strict` for numpy; none for assertpy,
    and the chain is `assert_that(a).is_equal_to(b)` and nothing more). The
    head name (`np`, `numpy`, `npt`, an imported `assert_equal` or
    `assert_that`) resolves, through a top-level `import` or
    `from numpy.testing import` / `from assertpy import` of the same file,
    to that library's function, and is bound exactly once in the file;
    relative and star imports and a second import of the name resolve
    nothing. No name after the head (`np.testing = fake`) and no
    `is_equal_to` is bound anywhere in the file. The head is otherwise used
    only as `head.` (a module alias) or `head(` (an imported callable), so
    it is never stored or passed along, and `np.testing` is only dotted
    into. No line that patches, mocks or `setattr`s mentions the library or
    the helper, and `globals`, `locals`, `vars`, `exec`, `eval` or `__import__` as a
    word (so `exec (...)` with a space is refused like `exec(...)`), or
    `__dict__`, `__code__`, `sys.modules`, `importlib` or `import *` with any
    spacing, anywhere refuses the file. The call is not under a `with`, `try` or `match`, where it may
    be expected to fail (`with pytest.raises(AssertionError)`). A numpy
    helper with a scalar literal operand (`assert_array_equal(f(), 3)`; numbers, including signed,
    parenthesized and arbitrary-width `0x`/`0b`/`0o` integers,
    `True`/`False`/`None`, and string literals of any prefix, triple-quoted or
    implicitly concatenated) is
    not read either: numpy broadcasts the scalar, so the call passes for
    `[3]` as well as `3` and a shape change goes unseen (assertpy compares
    with `==` and is unaffected). The credit is a diff-mode pass: repo-mode
    exposure analysis keeps the rule 3 reading (#10945). The reported
    oracle text stays the call as written; the equality gates (rule 4, the
    tautology and wrapped-argument gates) read the equivalent comparison.
    `assert_allclose`, `assert_almost_equal`, `assert_array_almost_equal`,
    `assert_array_less`, an `np` or `assert_that` the file did not import
    from the library, a project-local fluent helper (example 11), a name
    renamed away from `assert_array_equal` or `assert_equal`, and a helper
    re-exported through a project module keep the rule 3 reading. Another
    file that patches the library (a conftest fixture, a plugin) is not
    seen, as for every helper this module resolves
    (`py-numpy-assertpy-equal-to` and `py-numpy-assert-equal-import`
    credited; `py-numpy-allclose-tolerance`, `py-numpy-under-raises`,
    `py-numpy-shadowed-np`, `py-numpy-patched-helper` and
    `py-numpy-tautology` stay non-credited, #10936).

29. **Bounded integer dunder truthiness.** For a sole-line replacement in
    `__bool__`, a bare `assert Class(integer)` (optionally `not`) is an exact
    discriminator only when its condition holds before and fails after the
    edit. The module must contain one plain, undecorated class without bases;
    methods have no decorators, defaults or annotations and are limited to
    `__init__` and `__bool__`.
    Its constructor is one `self.field = parameter` and
    `__bool__` is one return comparing that field to a signed 64-bit integer
    literal with `==`, `!=`, `<`, `<=`, `>` or `>=`. Both old and new predicates
    must resolve on the same field. The test module contains one import and
    one argument-free, undecorated function without annotations or defaults;
    literal docstrings are allowed. This capability is limited to explicit
    `--diff` input's live source view; default/`--base`, `--worktree` and immutable
    candidate subjects retain abstention. A committed-content overlay also
    refuses credit: a physical audit cannot exclude committed writers or import
    artifacts deleted or truncated on disk. A truthiness-only physical audit must find
    exactly those two nonempty, ASCII Python sources with bytes matching the
    parsed view. Empty package initializers are allowed. It inspects every
    directory, including generated, excluded and hidden paths, within 512
    entries and 16 levels; a third Python source, local bytecode/native/import
    artifact, symlink, non-ASCII path, incomplete read or traversal/type error
    refuses this credit. The audit is a bounded current filesystem observation,
    not a change to the general walker or a hostile-race guarantee.
    Pytest configuration files in the workspace or its bounded ancestor chain
    also refuse credit, because collection settings and options are not
    interpreted by this audit. Environment options, external plugins and
    explicit pytest command-line selection are not audited. Assertions with
    message expressions refuse credit because those expressions run on failure.
    The physical observation cannot be reused through `explain/context --from`:
    artifacts carrying this credit refuse with `check_artifact_unusable` and
    direct fresh analysis; other artifact findings retain their existing contract.
    The selected function's sole statement is that assertion,
    the class import identifies the exact owner module, and no fixture, implicit
    setup, activation control, edited test, rebinding or known foreign write
    can replace the class, constructor, method or field. Other types and wider
    object behavior retain the existing abstention. In particular, `Money(1)`
    and `not Money(0)` do not distinguish `!= 0` from `> 0`; `Money(-1)` does
    (#9795). This does not broaden equality guards, properties or `__str__`.

    Owner-resolved credit (#9814): `f(g(x)) == g(x)` discriminates a change
    to the bare function `f` when `g(x)` is independent, although rule 25
    demotes the assertion at extraction. Extraction records the outer callee
    when `f` takes the shared call as its sole argument and does not appear
    inside it; a finding whose owner is exactly that module-level function
    reads the assertion as an exact value check. Every other owner, including
    `g`, keeps the demotion, so a constant edit of `g` still reads
    `weakly_exposed`. The `py-nested-independent-outer-owner` corpus case
    retains the credit; `py-nested-repeated-inner-owner` and
    `py-nested-idempotent-owner` retain the negative controls.

30. **A coarse projection of the owner's result pins no value.** An
    assertion credits a changed owner only when it reads the result itself.
    When every call of the owner in the assertion sits under a projection
    that an edit commonly leaves unchanged, the assertion keeps its
    `exact_value` kind but earns no `exposed` credit (`weakly_exposed`, the
    same fail-closed reading as an opaque wrapper): a comparison, `is` or
    `not` result used as a value in an operand (`assertEqual(f(2) > 0, True)`,
    `assertEqual(f(2) is None, False)`), a numeric remainder (`assert f(3) % 3 == 0`; string formatting is not one),
    `.__class__` or `.__name__` of the result, `len(str(f(x)))` or
    `len(repr(f(x)))`, and `len(...)` directly over the owner's result (through
    `list`, `tuple`, `sorted`, `reversed`) when the edit replaces only the element
    expression of a `yield` or comprehension, so the element count cannot
    move. A direct operand beside such a read (`assert f(3) == 6 and f(3) % 3
    == 0`), a top-level `>` boundary, other arithmetic and a `len` over an
    edit that can change the count keep their verdict. A first `==` operand
    that holds the owner's value at an input the edit leaves fixed is a
    separate problem and is not read here. The walk follows comprehensions,
    generator expressions, set and dict literals, conditional expressions and
    lambda bodies, so `assert all(f(2) > 0 for _ in xs)` is coarse while
    `assert [f(x) for x in xs] == [6]` is a direct read. A shape the walk does
    not follow that mentions the owner counts as a direct read: the demotion
    needs proof that every owner call is coarse, so an unknown shape never
    invents one.

31. **A nested def is reached through its enclosing call only when the enclosing
    body calls it.** A def nested in a function is its own owner
    (`outer.inner`). A test that calls only `outer` is related to a changed
    `outer.inner` (by call, by name token or by import of `outer`) only when
    the statements that run with `outer` call `inner` by name, spaced calls
    included. The nested defs and classes inside `outer`, at any depth through
    `if`, `with`, `for`, `try` and `match` blocks, are not part of those
    statements, and neither is the body of a `lambda` anywhere (found in the
    syntax tree, so the word inside a string does not count), so a call from
    the nested body itself, from an unused sibling def or from a lambda is not
    an edge. The decorators, defaults, annotations, return annotations and
    class bases of a nested declaration are evaluated with the enclosing body
    and still count. A body that rebinds the name (assignment, walrus, `for`, `as`,
    `import`), a name defined more than once in the enclosing body, and a def
    that is only returned or passed are fail-closed: they give no reach. A direct test call of the nested name is
    unchanged. Reach is a call path; credit needs a result path. When every
    call the enclosing body makes is an expression statement of its own
    (`inner()` on a line by itself), the nested function's result reaches
    nothing the enclosing function returns, so a strong assertion on the
    enclosing function's result gives a changed `return` in the nested body no
    direct return-value credit: the finding is `weakly_exposed` with the
    missing discriminator "an assertion on a value computed from the result of
    `outer.inner`". A call whose value may be returned, bound, passed or
    asserted keeps the credit, and so does a related test that calls the
    nested name itself.

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
| 6 | strong, credited, boundary or changed term (rule 20) not activated | `weakly_exposed` | yes / yes / weak |
| 7 | strong otherwise (orthogonal or error-path gate) | `weakly_exposed` | yes / weak / weak |
| 8 | weaker than strong | `weakly_exposed` | yes / weak / weak |

Confidence is 0.6 for `exposed`, 0.2 for `static_unknown` and 0.4
otherwise. Python never emits `reachable_unrevealed` or
`infection_unknown`. The ladder is the existing behavior and is normative.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable documented
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
   Rejected: credit a class-only `pytest.raises(KeyError)` when the
   changed line swaps the exception class, as for `ValueError` to
   `KeyError`. Whether that discriminates depends on the class
   hierarchy: a change from `KeyError` to `LookupError` with
   `raises(LookupError)` passes on both versions, and static evidence
   does not resolve the hierarchy. It can follow rule 12's route when a
   corpus case shows the cost.
4. **`raises(...) as exc` with a value assertion on `exc.value`.**
   Adopted (amended 2026-10-04, #6603): rule 14 promotes the bound
   `with` item to `exact_error_variant` / strong when the test compares
   `exc.value` or `cm.exception` with `==`, like the RIPR-SPEC-0106 Rust
   upgrade. The first draft kept it uncredited because it added credit
   without proof; the corpus case `py-spec0233-ex21-exc-value` fails on
   the `KeyError("blank")` mutant, so the uncredited reading is a false
   actionable verdict. The binding proof is the rule's name and
   no-reassignment check. Rejected: any use of `exc.value`, because
   `assert exc.value is not None` pins nothing.
5. **Non-literal `match=`.** Adopted: a name bound once to a string
   literal is read as that literal (rule 2, amended after review); any
   other non-literal pattern stays strong, because it usually holds a
   real message (`re.escape(expected)`) and weakening it would turn those
   findings into false actionable verdicts. Rejected: weaken every
   non-literal pattern.
6. **`pytest.approx` and `len(x) == n`.** Adopted: unchanged,
   `exact_value` / strong; both pin a value, and the f-string gate covers
   the length case it cannot see. Rejected: weaken them. Known limit: an
   explicit `abs=` or `rel=` wide enough to accept the pre-change value
   still credits; rule 12 treats an explicit `assertAlmostEqual`
   tolerance as weak, and aligning `approx` with it is tracked in #6585
   rather than changed here without a corpus case.
7. **Unrecognized forms.** Adopted: `assertIs`, `assertIsNone`,
   `assertListEqual`, `pytest.warns`, an aliased `pt.raises` and an
   awaited mock assertion stay unrecorded (reach-only), because recording
   them adds credit no spec promises and no corpus case yet shows the
   cost. `assertAlmostEqual` was in this list until #6603: the corpus
   case `py-spec0233-ex08-almost-equal` fails on its `+ 2` mutant, so
   rule 12 records its default-tolerance form like `pytest.approx`. Rejected: add the rest here
   without evidence. Each can follow rule 12's route when a corpus case
   shows a false actionable verdict.
8. **`mocker.patch`.** Adopted: counts as `mocked_module`, because
   pytest-mock substitutes at runtime. Rejected: limit the rule to
   `unittest.mock` receivers.
9. **Callable arguments of exception assertions.** Adopted (amended
   2026-10-04, #6603): rule 13 relates the named callable as an R1
   `syntactic_call`. The first draft kept `assertRaisesRegex(E, r, f, '')`
   a reference read by R7 because no spec made a callable argument a
   call. But `unittest` and pytest document that form as calling `f`,
   and the corpus case `py-spec0233-ex20-raises-regex` fails on the
   `KeyError("blank")` mutant, so the R7 reading is a false actionable
   verdict. Rejected: relate any name passed to any call, because only
   these assertions are documented to call their argument.
10. **Non-transparent decorators.** Adopted: unchanged, `@property` and
    `@functools.lru_cache` stay `decorator_indirection`. Rejected for now:
    make `@property` transparent, because it adds credit.
11. **`and` chains.** Adopted (2026-10-05, #6599): rule 15, the strongest
    conjunct. The first draft kept example 5's `smoke_only` reading, but
    the corpus case `py-spec0233-ex05-and-chain` fails on its mutant, so
    that reading is a false actionable verdict. Rejected: credit an `or`
    chain's strongest operand, because either side alone can pass.
12. **Effect channels.** Adopted: rule 17 with its own
    `alignment_reason`, so a consumer can tell an effect observation from
    a return-value one. Rejected: reuse `strong_oracle_observes_changed_sink_token`,
    because no changed-line token appears in the assertion. Rejected: any
    capture, file or mock read in the test, because only the channel the
    changed line writes, read through the value the test passed in,
    observes that line.
13. **Hypothesis.** Adopted: rule 19. A generated input still hides the
    boundary unless an `@example` row pins it, so a strong assertion on a
    generated parameter stays `property_based_test`; a test that asserts
    nothing is a gap, not a limit, so it keeps its actionable verdict.
    Rejected: treat every `@given` test as a limit, which hides the
    "asserts nothing" gap of `py-hypothesis-label-no-crash`.
14. **A removed term that is zero at the asserted input.** Adopted
    (2026-10-12, #6733): rule 20, evaluating one guarded-return callee one
    call deep with the boundary rule's own literal bindings. The corpus
    case `py-pytest-quote-shipping-term` removes `+ shipping_fee(...)`
    and its only test orders above the free-shipping threshold, so the
    test passes on the mutant while every gate credited `exposed`. Rejected: record
    the case as a known limit, because the false `exposed` hides a real
    gap and the shape (a fee, discount or surcharge that is zero above a
    threshold) is common in pricing code. Rejected: general
    interprocedural value evaluation, because one guarded return is the
    largest shape whose value static evidence reads without running code.
15. **A mock passed for a called parameter.** Adopted (2026-10-13,
    #7586): rule 17 reads a changed `p(...)` like `p.m(...)`, and reaches
    the mock through the owner's result when the owner returns `p`. The
    corpus case `py-spec0233-ex09-mock-arg` fails at runtime on the
    handed 2 while ripr read it `weakly_exposed`. A rewrite that keeps the
    old argument inside it, other than literal arithmetic, does not credit
    on either form: `py-spec0233-ex09-mock-sign` wraps the argument in
    `abs`, and the test's positive input hands the mock the same value. Rejected: promote `mock_expectation` to strong
    for exact arguments, because it would credit the `abs` case too.
    Rejected: require every argument rewrite to change the value at every
    input, because a scaled argument (`amount * 100`, case
    `py-unittest-transfer-notify`) differs at every input except 0 and
    its test fails at runtime. Literal arithmetic is therefore credited,
    accepting the miss at the one input where it is the identity.
    Rejected: gate only a single wrapping call, because `max(n, 0)`,
    `n or 0` and nested wrappers return `n` unchanged just as often.
16. **A decorator argument or decorator-factory loop edit is limited by
    design.** Adopted (2026-10-26, #9619): an edit to a decorator's
    argument (`@retry(3)` to `@retry(4)`) or to the loop inside the factory
    it calls (`range(times)`) is judged by module state that persists across
    calls (`flaky()` succeeds on the third attempt), and no bounded static
    evaluator reads that soundly. The rows `py-shapes-decorator-retry-count`
    and `py-shapes-decorator-loop-bound` keep ideal `credited` and accepted
    `limited`, and stay in `missed-credit.toml` with this Decision as the
    reason. The equivalent twins `py-shapes-decorator-retry-count-slack`
    and `py-shapes-decorator-loop-bound-slack` make the same edits where
    the pinned call cannot tell (`@retry(4)`, `range(times + 1)`), so
    crediting by edit shape would read `exposed` on a test that passes on
    the mutant; they must stay `gap` or `limited`. Rejected: credit any
    test that calls a function decorated with the edited factory, because
    the twins show that call does not discriminate. Revisit with a
    fixture-backed state evaluator under its own spec, as Decision 9 of
    RIPR-SPEC-0246 does for mocha hooks.
17. **A test defined under a platform guard, or asserted inside a
    callback wrapper, is limited by design.** Adopted (2026-10-26, #9619).
    `py-trap-platform-matched` defines its test under
    `if sys.platform != "win32"`, which holds where the corpus was labeled
    and fails on Windows, where ripr also runs; a verdict that depends on
    the host would differ between machines for the same diff.
    `py-trap-wrapped-through` asserts inside a lambda handed to a local
    wrapper that re-raises, while its twin `py-trap-wrapped-swallow` hands
    the same lambda to a wrapper that swallows the failure. Telling them
    apart needs a rule that reads the wrapper's handlers, and a misread
    credits a pin the runner never sees. Both rows kept ideal `credited`
    and accepted `limited`, and stayed in `missed-credit.toml` with this
    Decision as the reason. Rule 27 (2026-10-26, #9619) later took the
    bounded wrapper rule this Decision named as its revisit, so only the
    platform-guard half stays limited by design. The twins `py-trap-platform-guarded` and
    `py-trap-wrapped-swallow` already hold the negatives: each is a
    `not_discriminated` row that must stay `gap` or `limited`. Rejected:
    credit by guard polarity, because that reads the host platform.
    Rejected: credit any lambda passed to a local helper, because the
    swallowing twin is the same shape. Revisit with a bounded wrapper rule
    (a test-file local whose every handler ends in a bare `raise`) under
    its own spec, and a platform-independent reading of the guard.

## Required Evidence

- Each Problem row reads the class the rules give, through
  `classify_change_with_old` and in `ripr check --json` for a fixture.
- Swapping two assertions in a test, or two test names, never changes a
  class on the acceptance set.
- Every "unchanged" example keeps its kind, strength, relation and class.
- Golden drift lists every Python finding whose class, relation or
  `oracle_alignment` moved, split into credit gained (rules 4, 6, 8, 12,
  13, 14, 15, 16, 17, 19) and
  credit removed (rules 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 13, 18, 19, 20).
- The static-limit detectors have a negative test per token rule (a
  string literal, a longer identifier, a `.` receiver).

## Non-Goals

- No new oracle kind, strength, relation, limit kind or alignment value,
  apart from rule 17's `alignment_reason` token.
- No helper body resolution, fixture resolution or import graph. Rule
  20 reads one owner-module callee of a single guarded-return shape; it
  is not a general value evaluator.
- No change to the boundary activation rule, apart from rule 16's offset
  operands, and no change to the changed-default rule, the dunder rules
  or RIPR-SPEC-0201.
- No relation for a callable passed to any call other than the
  exception assertions of rule 13.
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
5. `assert parse('1') == 2 and other == 3`: `exact_value` / strong,
   `exposed`, `direct` (today `smoke_only` / smoke, `weakly_exposed`;
   rule 15). `assert parse('1') == 2 or other == 3`: `smoke_only` /
   smoke, `weakly_exposed` (unchanged). `assert int('1') == 1 < parse('1')`:
   `relational_check` / weak, `weakly_exposed` (today `exact_value` /
   strong, `exposed`, inferred; rule 1).
6. `self.assertEqual(1 + 1, 2, parse('x'))`: `weakly_exposed` (today
   `exposed`, inferred; rule 4: the owner call is the failure message).
   `assert 1 + 1 == 2, parse('x')`: the same.
   `assert parse('1') == parse('1')`: `relational_check` / weak,
   `weakly_exposed` (today `exact_value` / strong, `exposed`). The same for
   `self.assertEqual(parse('1'), parse('1'))`.
   `self.assertEqual(first=parse('1'), second=2)`: `exact_value` /
   strong, `exposed` (unchanged; named operands are compared). `assert norm('a b') ==
   norm('ab')` is not a tautology: whitespace inside a string literal is
   part of the value, so it keeps `exact_value` / strong.
7. `assert parse('1') == pytest.approx(2.0)`: `exact_value` / strong,
   `exposed` (unchanged).
8. `self.assertAlmostEqual(parse('1'), 2.0)` only: `exact_value` /
   strong, `exposed` (today no assertion, reported `unknown`,
   `weakly_exposed`; rule 12, #6603). With `delta=100` or `places=0`:
   `relational_check` / weak, `weakly_exposed` (class unchanged). With
   `places=None`: as the default form. `checks.assertAlmostEqual(parse('1'),
   2.0)` only, through a project helper: no assertion, `weakly_exposed`
   (unchanged).
9. `m = parse('1')` then `m.assert_called_once_with(1)`:
   `mock_expectation` / medium, `weakly_exposed` (unchanged). For an
   owner `def on_parse(text, handler): handler(int(text) + 1)` then
   `return handler`, changed from `handler(int(text))`, the test
   `m = on_parse('1', Mock())` / `m.assert_called_once_with(1)`:
   `exposed` (today `weakly_exposed`; rule 17, #7586), since `m` is the
   mock the test passed. With the change `handler(abs(int(text)))`
   instead: `weakly_exposed` (unchanged), since `abs` hands the mock 1 at
   that input.
10. `np.testing.assert_array_equal(parse('1'), [2])` only, with
    `import numpy as np` in the test file: `exact_value` / strong,
    `exposed` (today `unknown` helper, `static_unknown`,
    `opaque_custom_assertion_helper`; rule 28, #10936). Without that import,
    or with `np` bound by anything else: `unknown` helper, `static_unknown`,
    `opaque_custom_assertion_helper` (unchanged). `assertpy`'s
    `assert_that(parse('1')).is_equal_to(2)` with `from assertpy import
    assert_that` reads the same way.
11. `assert_that(parse('1')).is_equal_to(2)` only, where `assert_that` is
    a project helper (`from tests.fluent import assert_that`): `unknown`
    helper, `static_unknown`, `opaque_custom_assertion_helper` (today no
    assertion, `weakly_exposed`; rule 3).
12. `assert parse('1') == 2` then `assert other == 3`: `exposed`,
    `direct`, reported oracle `assert parse('1') == 2` (today
    `weakly_exposed`, `orthogonal`).
13. `assert other == 3` then `assert parse('1') == 2`: `exposed`,
    `direct` (unchanged).
14. Error owner, `with pytest.raises(KeyError, match='empty'): perr('')`:
    `exact_error_variant` / strong, `exposed`, `direct` through the owner
    identity token `perr` (the first successful credit branch, before
    `changed_sink_token`; the exposure classification remains `exposed`).
    This example exercises owner-identity credit, not changed-sink-token
    credit; the exception class changes as specified above.
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
    `broad_error` / weak, `weakly_exposed` (class unchanged). With
    `match='empty|blank'` and the raise line changed only in its
    message, from `'empty'` to `'blank'`: `broad_error` / weak,
    `weakly_exposed` (today `exact_error_variant` / strong, `exposed`,
    inferred; rule 2). With `pattern = '.*'` in the test body and
    `match=pattern`, and the same message-only change: `broad_error` /
    weak, `weakly_exposed` (today `exact_error_variant` / strong; class
    unchanged, since neither `perr` nor `blank` is in the item; rule 2).
20. Error owner, `self.assertRaisesRegex(KeyError, 'empty', perr, '')` in
    a `unittest.TestCase`: relation `syntactic_call`,
    `exact_error_variant` / strong, `exposed` (today `same_stem`, oracle
    not used, `weakly_exposed` with no repair card; rule 13, #6603).
    `self.assertRaises(KeyError, perr, '')`: `syntactic_call`,
    `broad_error` / weak, `weakly_exposed` (today `same_stem`, no repair
    card). The same call with
    `lambda: perr('')` in place of `perr, ''`: unchanged from today.
21. Error owner, `with pytest.raises(KeyError) as exc: perr('')` then
    `assert str(exc.value) == "'empty'"`: `exact_error_variant` / strong,
    `exposed` (today `weakly_exposed`; rule 14, #6603). The same with
    `with self.assertRaises(KeyError) as cm:` and
    `self.assertEqual(str(cm.exception), "'empty'")`: `exposed` (today
    `weakly_exposed`). With the raise line changed only in its message,
    `raise KeyError('blank')` from `raise KeyError('empty')`, and the
    same pytest test: `exposed` through the owner call in the `with`
    body (today `weakly_exposed`; the corpus case
    `py-spec0233-ex21-exc-value` has this shape).
22. Owner `def build(): return {'port': 80, 'timeout': 8080}`, changed
    from `'port': 8080`; test `assert build()['timeout'] == 8080`:
    `weakly_exposed`, `orthogonal` (today `exposed`). Test
    `assert build() == {'port': 80, 'timeout': 8080}`: `exposed`
    (unchanged). Owner `def build(): return {'port': 80, 'timeout': 80}`,
    changed from `'port': 8080`; test `assert build()['timeout'] == 80`:
    `weakly_exposed`, `orthogonal` (today `exposed`, because the `80`
    literal matches the changed value whatever key holds it).
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
34. Error owner, `with pytest.raises(KeyError) as exc: perr('')` then
    `assert exc.value is not None`: `weakly_exposed` (class unchanged;
    rule 14 needs `==`). The reported kind is `broad_error` / weak under
    rule 4 (today `relational_check`, the last weak assertion). With
    `exc = other` between the block and
    `assert str(exc.value) == "'empty'"`: `weakly_exposed` (unchanged;
    `exc` was assigned again). The same with `exc.value.args = ('empty',)`
    in place of `exc = other`: `weakly_exposed` (unchanged). With
    `assert str(exc.value) == "'empty'"` inside the `with` body after
    `perr('')`: `weakly_exposed` (unchanged; the comparison never runs
    when the call raises). With `assert len(str(exc.value)) == 7` after
    the block: `weakly_exposed` (unchanged; a length read does not pin
    the message, and `'empty'` and `'blank'` have equal length).
    With an owner `def perr(text, **kwargs):` (same body),
    `pytest.raises(KeyError, perr, '', match='empty')`, the callable
    form: `syntactic_call`, `broad_error` / weak, `weakly_exposed`
    (rule 13; `match=` goes to `perr`; today `same_stem`, class
    unchanged). With `def handle(x):` whose `except ValueError:` line is
    changed to `except:`, and only `assert handle(1) == 1`: error-path
    family, `weakly_exposed` (today `exposed`, inferred; rule 5).
35. Module `LIMIT = 5000` and `def fee(s):` / `if s > LIMIT - 1:` /
    `return 0` / `return 499`, the `if` line changed from `s >= LIMIT`;
    test `assert fee(4999) == 499`: `exposed` (today `weakly_exposed`;
    rule 16). Test `assert fee(6000) == 0` only: `weakly_exposed`
    (unchanged; the boundary is not activated).
36. Owner `def show(n): print(n * 2)`, changed from `print(n * 3)`;
    test `def test_s(capsys):` / `show(2)` /
    `assert capsys.readouterr().out == '4\n'`: `exposed`, `direct`,
    `strong_oracle_observes_changed_effect_channel` (today
    `weakly_exposed`, `orthogonal`; rule 17). The same test with
    `captured = capsys.readouterr()` taken before `show(2)` and
    `assert captured.out == ''`: `weakly_exposed`. Test
    `assert show(2) is None` only: `weakly_exposed` (unchanged). For a
    method owner `Printer.show` with the same body, a test calling
    `Fake().show(4)` on another class: `weakly_exposed`.
37. Owner `def notify(c, u): c.send(u, 'welcome')`, changed from
    `'hi'`; test `c = Mock()` / `notify(c, 'u')` /
    `c.send.assert_called_once_with('u', 'welcome')`: `exposed` (today
    `weakly_exposed`; rule 17). With `c.send.assert_called_once()`,
    `c.send.assert_called_once_with('u', ANY)`,
    `c.send.assert_called_with('u', 'welcome')` or
    `c.send.assert_any_call('u', 'welcome')`: `weakly_exposed`
    (unchanged). With `def notify(c, a, u):` whose unchanged line calls
    `a.send(u)`, the assertion `a.send.assert_called_once_with('u')`:
    `weakly_exposed` (`a` is not the changed collaborator). Owner
    `def save(p, t): p.write_text(t.lower())` and a test that passes `f`
    and asserts `f.read_text() == 'a'`: `exposed` (rule 17). Owner method
    `def bump(self): self.count = self.count + 2` and a test
    `c = Counter()` / `c.bump()` / `assert c.count == 2`: `exposed`
    (rule 17); with `assert c.count > 0`: `weakly_exposed` (unchanged).
38. Owner `def tax(x): return x * 8 // 100`, changed from `x * 7 // 100`,
    and `def checkout(x): return x + tax(x)` in the same module; test
    `assert checkout(1000) == 1000 + tax(1000)`: `weakly_exposed` (today
    `exposed`; rule 18). Test `assert tax(1000) + 1 == 81`: `exposed`
    (unchanged).
39. Owner `def passed(score): return score >= 50`, changed from
    `score > 50`. Test `@given(st.integers(0, 100))` / `@example(49)` /
    `@example(50)` / `def test_p(score):` /
    `assert passed(score) == (score >= 50)`: `exposed` (today
    `static_unknown`, `property_based_test`; rule 19). Without the
    `@example` rows: `static_unknown`, `property_based_test`
    (unchanged). A `@given` test `def test_p(tmp_path, score):` that only
    calls `passed(score)`: `score` is not a fixture, and the finding is
    `weakly_exposed` (today `static_unknown`,
    `unresolved_pytest_fixture`; rule 19).
40. Module `FREE = 5000`, `def ship(s):` / `if s >= FREE:` / `return 0`
    / `return 499`, and `def quote(s): return {'total': s}`, changed from
    `return {'total': s + ship(s)}`; test
    `assert quote(6000)['total'] == 6000` only: `weakly_exposed`, missing
    discriminator `ship(s) != 0` (today `exposed`; rule 20, #6733). Adding
    `assert quote(1000)['total'] == 1499`: `exposed` (unchanged; that call
    activates the term). With `amount = 6000` and `quote(amount)`:
    `exposed` (unchanged; the input does not resolve). With `ship` changed
    to `return s // 100`: `exposed` (unchanged; not a guarded return).

41. Class `Account` with `def deposit(self, amount): self.balance += amount`,
    changed to `self.balance = self.balance + amount`; conftest
    `@pytest.fixture` / `def account(): return Account(100)` with
    `from shop import Account`; test `def test_d(account):` /
    `account.deposit(50)` / `assert account.balance == 150`: the same
    verdict as `account = Account(100)` inlined in the test body (today
    `static_unknown`, `unresolved_pytest_fixture`; L8,
    `py-fixtures-deposit`). A `yield`, an extra statement, a fixture
    parameter, decorator arguments, a second decorator, an `async def`, a
    non-literal or starred argument, another class, a class from another
    module, a renamed import, a second binding of the class or the
    fixture name, a star import, `pytest_plugins`, another fixture or hook
    (here or up the chain) that names `account`, a nearer conftest that
    imports or assigns `account` or overrides it with `name="account"`,
    `pytestmark`, a `pytest_*` hook, a fixture that walks
    `request.fixturenames`, an imported fixture name, a defaulted
    parameter, a test decorator, a test that rebinds it, a test module
    that binds it and a class-level test all keep `static_unknown`,
    `unresolved_pytest_fixture` (unchanged).

42. Function `def handling_fee(amount): return 0 if amount >= 100 else 5`,
    changed to `amount > 99`; conftest
    `@pytest.fixture(params=[(99, 5), (100, 0)], ids=["below", "at"])` /
    `def fee_case(request): return request.param`; test
    `def test_fee(fee_case):` / `amount, fee = fee_case` /
    `assert handling_fee(amount) == fee`: the same verdict as
    `@pytest.mark.parametrize("amount, fee", [(99, 5), (100, 0)])` over
    `assert handling_fee(amount) == fee` (`exposed`; today
    `static_unknown`, `unresolved_pytest_fixture`; L8,
    `py-fixtures-fee-boundary`). Rows `(50, 5), (200, 0)` read
    `weakly_exposed`, as the decorated form does. Named, list,
    `pytest.param`, starred or non-literal rows, empty params, callable
    or short `ids=`, `scope=`, `autouse=`, a positional decorator argument, a
    transformed `request.param`, an extra fixture statement or parameter,
    a second decorator or definition; an unpack of the wrong arity, a
    starred, nested, attribute, chained or subscripted unpack, an unpack
    that is not first, a later rebinding or read of the fixture, a nested
    scope, a duplicate name, a defaulted or shadowing parameter, a foreign
    test decorator; a dependent fixture, a request walk,
    `pytest_generate_tests`, a star import, a module import of the name and
    a nearer conftest binding all keep `static_unknown`,
    `unresolved_pytest_fixture`. A test that also carries its own
    parametrize mark keeps only the mark's cases, and the fixture stays
    unresolved: `static_unknown`, `unresolved_pytest_fixture` (today
    `exposed`, a false credit for off-boundary rows).

43. Owner `def f(x): return x * 3`, changed from `x * 2`: tests
    `self.assertEqual(f(2) > 0, True)`, `assert f(3) % 3 == 0` and
    `assert len(str(f(3))) == 1` are each `weakly_exposed` (today `exposed`;
    `py-coarse-sign`, `py-coarse-residue`, `py-coarse-text-length`), and
    `assert f(3) == 6` stays `exposed` (`py-coarse-exact-control`).

## Test Mapping

- Existing: `crates/ripr/src/analysis/language/python/doctest_gate_tests.rs`
  `an_example_the_edit_breaks_is_a_test`,
  `an_example_both_functions_satisfy_is_not_a_test`,
  `doctests_not_collected_yield_nothing`,
  `an_example_added_by_the_diff_is_not_its_own_oracle`,
  `a_doctest_directive_or_continuation_drops_the_docstring`,
  `a_function_the_evaluator_cannot_run_is_not_read`,
  `a_decorated_function_is_not_read`, `a_rebound_name_is_not_read`,
  `a_test_dictionary_replaces_the_docstring_examples`,
  `a_conftest_that_ignores_collection_stops_the_read`,
  `conftest_hooks_and_namespaces_outside_the_file_path_stop_the_read`,
  `newer_pytest_config_files_leave_collection_unknown`,
  `attached_short_flags_and_cache_selectors_stop_the_read`,
  `a_positional_collection_target_in_addopts_stops_the_read`,
  `a_module_that_binds_len_is_not_read`,
  `an_escape_in_the_docstring_source_drops_it`,
  `a_trailing_comment_on_the_example_line_does_not_hide_the_comparison`.
- Existing: `crates/ripr/src/analysis/language/python/helper_assertions_tests.rs`
  `given_a_reraising_wrapper_when_it_runs_a_helper_lambda_then_the_helper_call_is_the_assertion`,
  `given_a_wrapper_that_can_swallow_when_it_runs_a_helper_lambda_then_nothing_resolves`,
  `given_a_reraising_wrapper_when_the_call_is_not_a_bare_helper_lambda_then_nothing_resolves`,
  `given_a_wrapper_name_the_test_rebinds_or_imports_when_called_then_nothing_resolves`,
  `given_a_reraising_wrapper_around_an_unresolvable_helper_when_called_then_nothing_resolves`.
- Existing: corpus rows `py-trap-wrapped-swallow`, `py-trap-wrapped-partial`,
  `py-trap-wrapped-conditional` and `py-trap-wrapped-finally` (`cargo xtask
  verdict-corpus check --language python`): rule 27's runtime-verified
  swallowing negatives, which must never credit.
- Existing: corpus rows `py-trap-platform-guarded` and
  `py-trap-wrapped-swallow` (`cargo xtask verdict-corpus check --language
  python`): Decision 17's negatives, which must never credit.
- Existing: corpus rows `py-shapes-decorator-retry-count-slack` and
  `py-shapes-decorator-loop-bound-slack` (`cargo xtask verdict-corpus
  check --language python`): Decision 16's negatives, which must never
  read `exposed`.
- Existing: `crates/ripr/src/analysis/language/python/doctest_eval.rs`
  `floor_division_and_modulo_follow_the_divisor_sign`,
  `slices_normalize_indexes_like_python`,
  `integer_overflow_and_bool_int_confusion_decide_nothing`,
  `an_exponential_replace_chain_is_refused_not_built`,
  `a_deep_sum_is_refused_not_recursed`.
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
- Rules 15 to 19:
  `crates/ripr/src/analysis/language/python/verdict_corpus_tests.rs`
  pins a crediting and a non-crediting form of each (examples 5, 35 to
  39, and rules 1, 2, 4 and 9 alongside them);
  `python/hypothesis.rs` unit tests pin `@given` pairing and `@example`
  rows.
- Corpus (#6597): `py-spec0233-ex05-and-chain` (rule 15; the
  `py-conjuncts-*` cases add an exact owner conjunct credited, an exact
  conjunct on a constant, all-weak conjuncts, an `or` chain and a `not`
  comparison scored as gaps),
  `py-pytest-shipping-threshold` and `py-unittest-fee-waiver-mixin`
  (rule 16; the `py-offsets-*` cases add a `C - 1` and a `C + 1` operand
  hit at the boundary value credited, a parametrize row at `threshold - 1`
  credited, and inputs that miss the boundary value scored as gaps),
  `py-fixtures-report-file`, `py-unittest-transfer-notify` and
  `py-unittest-deposit-add` (rule 17; the `py-channels-*` cases add exact `capfd` and
  local-after-owner `readouterr()` output, a file read back through the
  written parameter, and an arithmetic-wrapped mock argument credited, and a
  `capsys` read before the owner, stderr read as stdout, the other
  parameter's file, `assert_called_with`, `assert_any_call`, an argument-free
  or `ANY` mock assertion and an `abs(n)` rewrite scored as gaps), `py-pytest-tax-self-computed`
  (rule 18), and `py-hypothesis-passing-threshold` and
  `py-hypothesis-label-no-crash` (rule 19).
- Planned: one `classify_change_with_old` test per acceptance example,
  and an order-swap test that runs examples 12, 13, 17 and 18 in both
  orders.
- Planned: a split-gate test for examples 31 and 32.
- Planned: a negative detector test per token case of rule 8.
- Rules 3, 12 to 14 and 20: `verdict_corpus_tests.rs` pins examples 8,
  11, 20, 21, 34 and 40 with crediting and non-crediting forms: the
  explicit-tolerance, non-`self` receiver and `assertNotAlmostEqual`
  negatives; the `assertRaises` callable, a trivial regex and another
  function as the callable; the message-only change of example 21, whose
  comparison holds the old message; the `cm.exception`, `repr(...)` and
  `assertRaisesRegex ... as cm` forms; and the reassigned-`exc`, tuple
  target, walrus, `setattr`, attribute assignment, wrong-field,
  inside-the-body, length-read, `is not None`, both-operands-read and
  tautology negatives. The callable form's boundary binding has an
  activated and an unactivated pin. Rule 20 pins cover a rebound
  parameter, an unbindable strong call, a patched and a redefined
  callee, a subtracted term whose zero comes from the fallback branch,
  and the discriminator and observed values of example 40.
  `term_activation.rs` unit tests pin the term reader, including the
  precedence, open-bracket, continuation, string, comment, default
  value, unary-sign and changed-argument negatives, and integer zero.
- Corpus: `py-spec0233-ex08-almost-equal`,
  `py-spec0233-ex20-raises-regex` and `py-spec0233-ex21-exc-value`
  (#6597) score credited under rules 12 to 14; the `py-almost-*` cases
  cover rule 12's other forms (default and `places=None` credited,
  explicit `places=` and `delta=`, the positional `delta`, a spread and a
  non-`self` receiver scored as a gap), the `py-raises-*` cases cover
  rules 13 and 14 (a callable-form `raises` or `assertRaises`, with and
  without `match=`, and a coarse length read of a bound exception score
  as gaps; a bound `.exception` compared with `==` and an `.args` index
  score credited),
  `py-spec0233-ex11-fluent-helper` scores limited under rule 3, and
  `py-pytest-quote-shipping-term` scores its ideal `weakly_exposed` under
  rule 20. Example 34's `exc.value is not None` and length forms have
  cases (`py-spec0233-ex34-exc-not-none`, `py-spec0233-ex34-exc-length`,
  #7546); `assertRaises` callable and `assertRaises ... as cm` now have
  cases (`py-raises-callable-unittest`,
  `py-raises-bound-unittest-exception`), and any other example 34 form
  needs its own case before that part reads as established.
  Example 9 has an argument case (`py-spec0233-ex09-mock-arg`) whose
  mutant the `assert_called_once_with` expectation kills, which scores
  credited under rule 17's called-parameter form, and a sign case
  (`py-spec0233-ex09-mock-sign`) the test cannot see, which stays at its
  ideal gap under the wrapped-argument gate.
- Rule 17's called-parameter form (#7586): `verdict_corpus_tests.rs` pins
  the returned and the passed-local mock, the `abs` wrap, an owner that
  returns another value, rebinds the parameter or has a second `return`,
  and the argument-free and last-call-only expectations. A second test
  pins the gate on `p.m(...)`: nested and multi-argument wrappers, `or`,
  parentheses, an attribute read, an equal numeric literal and an old line
  with a comment or an assignment, a starred argument, identity
  arithmetic and a requoted string or respelled number do not credit;
  literal arithmetic on either side, an added argument, a changed keyword
  set, different literals and another name do. Each row pins the
  arguments the new call passes (#7719). A third test pins a
  `@classmethod` and a `@staticmethod` owner called on its imported class,
  through the returned and the passed-local mock, an import alias, a
  staticmethod handler in second position and a classmethod receiver
  named `klass`. It keeps uncredited the result of a classmethod with a
  second decorator, an instance method called on the class, a decorator
  that only ends in `classmethod`, a same-named method of another class
  and a class name the test rebinds (#7634).

- L8's literal owner-instance fixture (#9619):
  `crates/ripr/src/analysis/language/python/literal_fixture_tests.rs`
  pins the inlined control, the conftest and same-module fixture, and
  every negative shape and interference route of example 41.
- L8's literal-params fixture and the parametrize-mark scope (#9619):
  `crates/ripr/src/analysis/language/python/param_fixture_tests.rs` pins
  the decorated control, the conftest, same-module and single-name forms,
  the off-boundary parity and every negative shape of example 42;
  `classify/tests.rs` `an_indirect_parametrize_argname_is_an_unresolved_fixture`.

## Numeric factor reorders

A factor swap involving named operands retains the ordinary table-row reading
(#9815). Python `int`/`float` annotations and numeric defaults do not enforce
runtime operand types: custom `__mul__`/`__rmul__` methods can make the swap a
behavior change. Numeric test rows alone cannot establish global commutativity.
Order-sensitive repair guidance requests operands that distinguish multiplication
order; varying numeric rows alone cannot do so. Literal-only products have no
named table operand and keep their ordinary exact-oracle reading. This rule does
not infer local or module runtime types.

Guidance follows the changed product inside a container, and quoted `#` text
does not hide the expression. Exact binary swaps also read primitive finite
float rows. Integer literals are bounded to i128; mixed integer/float rows
require the integer to fit i32 before conversion, and the product must remain
finite. Calls, booleans, strings and nonfinite values are not numeric evidence.
This reading never flattens multiplication trees or assumes floating-point
associativity; unsupported rows retain the ordinary reading.

A direct whole-result table assertion can distinguish an independently changed
integer tuple or list sibling beside a factor swap. Credit this only for a
single-line replacement and a direct, undecorated simple owner with stable
parameters, a plain literal parametrization and a direct equality whose whole
old result matches an actual row while the new result differs. Unsupported
numeric comparisons and projected assertions do not establish this proof. A
test that rebinds a directly parametrized factor and has one admitted strong
assertion keeps the factor finding weak when the row no longer binds
that call value. The rebound-specific fallback applies only when the test has
one admitted strong assertion; prior handling of tests with multiple admitted
strong assertions is retained.

## Implementation Mapping

- `crates/ripr/src/analysis/language/python/doctest_tests.rs` and
  `doctest_eval.rs`: rule 26, collection detection, example reading and the
  evaluator gate.
- `crates/ripr/src/analysis/language/python/helper_assertions.rs`: rule 27,
  the re-raising wrapper read beside the single-assert helper read.
- `crates/ripr/src/analysis/language/python/oracles.rs`: assertion table,
  rules 1 to 3, 12, 14 and 15, and each assertion's gate text (rules 4
  and 13).
- `crates/ripr/src/analysis/language/python/related_tests.rs`:
  `strongest_assertion` and `RelatedTest` oracle text (rule 4), relation
  order, rules 7 and 13.
- `crates/ripr/src/analysis/language/python/sink_alignment.rs`: per
  assertion credit (rule 4) under the RIPR-SPEC-0224 family filter, the
  error-path gate (rule 6), rules 9 to 11 and the rule 17 credit branch.
- `crates/ripr/src/analysis/language/python/classify.rs`: the verdict
  ladder.
- `crates/ripr/src/analysis/language/python/probe_shape.rs`: error-path
  family (rule 5), unchanged.
- `crates/ripr/src/analysis/language/python/static_limits.rs`: limit
  order and token detection (rule 8), the `property_based_test` condition
  (rule 19).
- `crates/ripr/src/analysis/language/python/boundary.rs`: offset operands
  (rule 16), and the strong owner calls rule 20 reads.
- `crates/ripr/src/analysis/language/python/term_activation.rs`: the
  changed call term and the guarded-return callee (rule 20).
- `crates/ripr/src/analysis/language/python/effect_alignment.rs`: effect
  channels (rule 17), including the called-parameter form and the
  argument-rewrite gate, and self-computed expected values (rule 18).
  `term_activation.rs` records the parameter an owner returns.
- `crates/ripr/src/analysis/language/python/hypothesis.rs` and
  `owners_tests.rs`: generated inputs and `@example` rows (rule 19).
- `crates/ripr/src/analysis/language/python/oracles.rs` (dead arms and
  swallowed `try` bodies, rule 23) and `test_activation.rs` (empty tables and
  module-level skips, rule 24), pinned by `unreachable_assertion_tests.rs` and
  the `py-trap-*` corpus cases.
- `oracles.rs` (`is_property_compare`: idempotence and loop-variable identity
  checks, rule 25), pinned by `property_assertion_tests.rs` and the
  `py-grid-*-property` corpus cases.
- `crates/ripr/src/analysis/language/python/param_fixtures.rs` (literal
  `params=` rows and the unpack rewrite) with `fixtures.rs` (isolation) and
  `static_limits.rs` (direct parametrize argnames): L8, example 42.

## Metrics

- `python_oracle_reach_acceptance_mismatches`: acceptance examples whose
  kind, strength, relation or class differs from this spec, counted over
  both assertion orders and both test-name orders; must be zero.
