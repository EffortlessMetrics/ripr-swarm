# RIPR-SPEC-0028: Python Preview Static Facts

Status: proposed

## Problem

The Python preview adapter must emit RIPR static facts from Python source
without depending on `mypy`, `pyright`, an import graph, or any runtime
tooling. Syntax-first facts are the contract; semantic enrichment is
explicitly deferred.

This spec defines the per-language behavior the adapter must produce. The
language-neutral boundary, the router, the output metadata, the opt-in
posture, and the cross-language non-goals live in
[RIPR-SPEC-0026: Language adapter contract](RIPR-SPEC-0026-language-adapter-contract.md).
The proposal context is
[RIPR-PROP-0001: Multi-Language Adapter Preview](../proposals/RIPR-PROP-0001-multi-language-adapter-preview.md).

## Behavior

The Python preview adapter is enabled by repo configuration:

```toml
[languages]
enabled = ["rust", "python"]
```

When enabled, it routes `*.py` files. It emits the same RIPR fact
families as the Rust adapter and tags each finding with
`language = "python"` and `language_status = "preview"`.

When `ripr.toml` is absent, Python project detection may also enable the
adapter for roots with markers such as `pyproject.toml`, `setup.py`,
`setup.cfg`, `requirements.txt`, `pytest.ini`, `tox.ini`, `noxfile.py`, or
Python files under `src/` or `tests/`. An explicit `ripr.toml` remains
authoritative and can keep Python disabled.

The adapter is syntax-first. It must not depend on `mypy`, `pyright`, a
runtime test runner, or an import graph. When syntax-first analysis
cannot classify, the adapter emits an explicit `static_limit_kind`
instead of silently coercing to `no_static_path`.

## Inputs

- Python source files routed to this adapter.
- Diff spans inside those files.
- Repo configuration including `[languages] enabled` and any future
  Python-specific options layered on top of this spec.
- Python project marker filenames used only to decide whether missing-config
  roots should select Python preview analysis.

The adapter may observe project marker filenames, but it does not parse
`pyproject.toml` build metadata, install requirements, read virtualenv contents,
read generated stubs, or consume runtime test output.

Diff mode reads the workspace under a file-count cap
(`RIPR_PYTHON_MAX_WORKSPACE_FILES`) and byte budgets. The count cap refuses only
unchanged context: changed `.py` files are always kept. When the workspace
exceeds the cap, the rest of it is filled from the files that may reach the diff
(#10785): every `conftest.py` and the package `__init__.py` files above a
changed file; files whose text names a name a changed file defines (a `def` or
`class` name at any depth, a top-level assignment target, a word of a decorator
route literal) or a pytest fixture a reaching test module or `conftest.py`
defines over such a name (a `fixture as` decorator alias and an `import ... as`
alias of the name count, and fixture hosts are rescanned as fixture names join);
for a module-level change, files that import the
changed module, and for a `__dunder__` method change, files that import from a
package above it; and the workspace modules that a reaching test or a changed
file imports. A refused
file outside that set names nothing a related-test relation could match, so it
does not make the run partial. A refused file inside it is a named
`diff_scope_oversized` limitation that counts the refused reaching files and
says changed files were kept. A changed route literal with no fixed word in it
(`@app.get("/")`, `@app.get("/{item_id}")`), an unreadable changed file, a
fixture closure that does not settle, or a workspace past the scan's own file
bound keeps the earlier behavior: the earliest paths fill the cap and
every refusal is disclosed. The byte budgets still apply to changed files and
disclose their own refusals.

## Owner Facts

Owners the adapter must recognise:

- top-level `def` functions and `async def` functions
- nested functions only when they participate in changed behavior
- `class` definitions and their methods (regular, `@staticmethod`,
  `@classmethod`)
- decorated functions and methods; the decoration is preserved as
  syntactic context, not resolved semantically
- module-scope expressions whose right-hand side participates in changed
  behavior (when a probe attaches to the expression)

Owner kinds emitted in output (per RIPR-SPEC-0026):

- `function`, `method`, `class_method`, `module_function`.

The stable Python `probe.owner` identifier is language-qualified and
path-qualified:

```text
python:<normalized/path.py>::<qualified_owner>
```

Examples include `python:app/pricing.py::calculate_discount`,
`python:app/cart.py::Cart.apply_discount`,
`python:app/models.py::Invoice`, and
`python:app/settings.py::<module>`. Class owner findings may carry a
class-shaped `probe.owner` while omitting `owner_kind` until the shared
RIPR-SPEC-0026 owner-kind vocabulary explicitly adds a class value.

## Test and Assertion Facts

Known pytest skip/skipif/xfail marks and unittest controls in runner-honored
placements retain a related pointer but cannot contribute an oracle or boundary
input. Function-level unittest skip wraps the function under either runner;
unittest class skip flags and expected-failure metadata qualify only TestCase
tests. Those flags do not suppress ordinary pytest assertions. The relation is
`test_activation_unestablished`. Conditions are not evaluated: a visible false
conditional skip on a name or comparison remains uncertain within this bounded
static contract. The exception is a condition that is the boolean literal that
never triggers the mark (`skipif(False)`), which leaves the test active
(RIPR-SPEC-0238 rule 24).
Imported decorator aliases use the last explicit binding in the declaration-time
module/class scope, before test-body imports or later imports can shadow them.
A canonical-looking name explicitly imported from another module is not a
framework control. Unresolved canonical spellings remain conservative; arbitrary
assignment rebinding is not evaluated.
Unaliased dotted imports bind their root (`import unittest.mock` binds
`unittest`); explicit aliases bind the alias. Method body lookup keeps
its separate module/global scope; class-local imports are not method globals.
When no independent eligible test supplies a relation, report
`decorator_indirection` with the test activation reason and emit no repair
packet. A separate active test can still supply its own evidence. Module-level
`pytestmark`, dynamic `skipTest`, custom decorator semantics and imported-base
activation remain outside this rule.

Oracle admission does not prove the absence of side effects. Expected-failure
bodies may execute, conditional skips may be inactive, and a pytest-skipped
function can be called directly by an active test or helper. The existing
whole-module constant-rebinding veto therefore remains conservative, including
writes in marked bodies. Independent active assertions retain their own oracle
identity; resolving their boundary still requires constants unaffected by
potential writes. Proving nonexecution of body effects requires a separate
execution-effect model.

Curated upstream `native_cases` in the existing Python real-repo corpus record
independent behavior/test-understanding controls without implying an analyzer
run. The ItsDangerous future-age case retains immutable upstream source/license,
the real defect/fix, original pytest fixture closure, a weak alternate observer,
skip and expected-failure controls. Native evidence does not establish current
static classifications, installed journeys, or support-tier promotion.

Test discovery:

- `pytest` test functions with the default `test` name prefix at module level
- pytest test methods with the same prefix under `class Test*`
- `unittest.TestCase` subclasses and their default `test`-prefixed methods,
  including classes that reach `TestCase` through a base defined in the same
  file, and the `test`-prefixed methods of a same-file mixin that a collected
  test class inherits (#4562), directly or through another same-file mixin.
  A mixin member is collected only when some collected subclass resolves it
  to that mixin along Python's C3 method resolution order: the subclass's
  own definition (a method or a `test_x = None` assignment) or an earlier
  class in that order wins, and an imported base, a base defined after the
  subclass or a redefined class name is unknown, so a member behind it is
  not collected. A collected mixin member is recorded under the subclass
  that runs it (`TestConsumer.test_shared`), the node id the runner accepts,
  once per such subclass. Only the last definition of a class name is
  collected.
  An uninherited mixin is not collected, nor a `Test*` class that pytest
  skips because it or a same-file ancestor defines `__init__` or `__new__`
  or is a dataclass
- parametrized tests via `@pytest.mark.parametrize` (recognised
  syntactically)
- pytest fixture and parameter names captured from test function signatures
- fixture files matched by configured patterns (default: `test_*.py`
  and `*_test.py`; the configured pattern is part of the repo config
  cross-spec contract)
- framework-shaped verify commands for related tests when the static selector
  is known: `python -m pytest path::node` for pytest and
  `python -m unittest module.Class.test_method` for unittest. A literal dot
  in a unittest path component, after removing the `.py` extension, leaves
  module identity unresolved: omit that test's verify command while retaining
  related-test evidence. Repair placement uses the first oracle-eligible test
  in candidate order with a known verify command, or stays absent when none
  exists; repair-card and context/diagnostic-witness evidence describe that
  same selected file and test through one shared row resolver.
  When related rows share a file and bare method name, placement carries the
  selected source line as evidence so the card can distinguish them. Older
  reports without that line require a unique matching row to establish card
  identity. An incomplete, malformed, or ambiguous declared identity cannot
  borrow a differently ranked test's oracle. With no declared repair target,
  the diagnostic witness retains its existing related-test fallback.
  Shell quoting cannot preserve such
  a dot against unittest's module splitting. Ordinary quoted paths and pytest
  file selectors retain their existing behavior. The pytest form
  runs through `python -m` so the working directory is on `sys.path` and a
  flat-layout package at the repository root imports without a `pythonpath`
  setting; bare `pytest path::node` fails collection there with
  `ModuleNotFoundError`. Both forms spell the interpreter `python`, which names
  the virtual environment's interpreter on every platform

The default name prefix is case-sensitive and does not require an underscore:
`test`, `testCamelCase`, and `test_with_underscore` all qualify. `_test_private`,
`contest`, and `TestWrongCase` do not. The prefix rule applies to both `def`
and `async def`; it does not change test-file or class selection. Custom
`python_functions` patterns, modified `TestLoader.testMethodPrefix` values,
and collection hooks are not resolved by this syntax-only collector.

Assertions / oracles the adapter must recognise:

The repair-routing lane preserves a conservative internal oracle shape for
pytest and unittest facts without expanding the shared public `OracleKind`
vocabulary. That shape distinguishes exact assertions, boundary comparisons,
exception assertions, dict/object field assertions, output assertions through
`caplog.text` / `capsys.readouterr().out` / stdout-stderr-output attributes,
status-code and exit-code assertions, broad smoke assertions, reach-only tests,
mock expectations, and custom `assert_*` helpers.

- bare `assert expr` â†’ smoke oracle
  - exception (#9542): a bare `assert f(lit, ..)` / `assert not f(lit, ..)` whose
    single resolved workspace function owner provably returns `bool` is an
    exact pin, read as `f(lit, ..) == True` / `== False`; any ambiguity, patching
    or non-literal argument stays smoke
- `assert a == b` â†’ exact-value oracle; `assert a != b` and other
  non-equality comparisons â†’ relational check, weak
- `assert isinstance(value, SomeType)` â†’ relational check, weak
- `pytest.raises(...)` context manager â†’ error-path oracle
- `self.assertEqual(a, b)` â†’ exact-value oracle; `assertNotEqual` â†’
  relational check, weak
- `self.assertRaises(...)` â†’ error-path oracle
- `self.assertListEqual(owner(), [literal elements])` supplies strong exact-value
  evidence for a whole returned list, including short numeric element changes.
  Valid whitespace and grouping parentheses in the direct callee preserve its
  parsed identity, including imported aliases and module-qualified free calls.
  This bounded shape requires a module-level direct canonical `unittest.TestCase` or imported
  `TestCase` base, no additional bases, class hooks/assignments/decorators, and an
  ordinary `self` method with that call as its only statement. Both positional
  operands are required; message arguments, slices, nonliteral expectations,
  custom receivers, inherited wrappers and visible framework rebinding remain
  unknown helpers. Visible owner rebinding cannot supply sink alignment. Credit
  also requires the asserted operand to make a zero-argument call to the
  same imported free-function owner through a bare or dotted static name.
  Computed callees such as `owner().method()` or `owner()()` remain unknown
  helpers. The assertion row binds its declaration line as well as file/name;
  matching expected literals in another row
  or a nested wrapper cannot supply sink alignment. Skip and parser limitations
  remain governed by their existing admission paths.
  Whole-list credit additionally requires old/new literal-list returns with a
  different length or a same-type scalar constant that compares differently.
  Equal spellings/values and mixed scalar types (including Python-equal `1`,
  `True` and `1.0`) remain uncredited; numeric coercions and nonliteral return
  elements are not evaluated by this bounded path. An unproved list-equality
  change retains the finding and limitation without inventing a missing exact
  assertion, typed repair placement or repair card.
- `self.assertTrue(...)` / `assertFalse(...)` â†’ smoke oracle
- `mock.assert_called*` family (`assert_called_once_with`,
  `assert_called_with`, `assert_called`, `assert_not_called`) â†’
  side-effect/call oracle
- `unittest.mock` patches recognised syntactically as call-context only

Related-test heuristics mirror the Rust and TypeScript adapters: changed
owner name match, import-reference match, file-path proximity, and
syntactic call proximity. Direct owner calls must be token-aware. Module
import aliases may match attribute calls such as `pricing.apply_discount(...)`;
arbitrary object method calls must not be treated as related to a top-level
function owner unless the changed owner is itself a method or class method.
Free-function module identity compares a `from M import name` source module
with the owner's dotted module path by exact equality. An owner below a
directory named `src` (the PyPA src layout, including monorepo
`packages/<pkg>/src/`) also matches the dotted path below that `src` import
root, so `from pricing.discounts import f` identifies `src/pricing/discounts.py`
while the repository-relative `src.pricing.discounts` form still matches. Bare
file stems and arbitrary path suffixes never count as module identity.
A test that imports a free-function owner's name from a workspace module that
defines its own function of that name, when no import reaches the owner's module,
is not related to the owner even if its call text matches the name (#8759). The
evidence must be affirmative: the import's source module is exactly the module
path of another workspace source file defining the same free function. A facade
or package re-export, an unresolved or third-party source, a source the owner's
path could also be spelled as under an import root the adapter cannot see, an
ambiguous src-layout name, a test with no import of the name, a star import, and
a test in the owner's own file keep the name-based relation.
Only top-level free functions are gated, and only top-level imports are read: a
same-named wrapper that delegates to the owner, or an import under `try`/`if`,
is not modeled, so such a test can read as unrelated.

A package whose `__init__.py` re-exports a top-level function or class under
its own name is also an import path of that owner: `from .time import
naturaldelta` in `src/humanize/__init__.py` lets `import humanize` +
`humanize.naturaldelta(...)` and `from humanize import naturaldelta` relate to
`src/humanize/time.py::naturaldelta`, and `from .more import *` lets
`import more_itertools as mi` + `mi.one(...)` relate to
`more_itertools/more.py::one`. The re-export is followed only from
`__init__.py` module imports whose source module is the owner's module (or an
earlier re-exporting package), for at most three packages. A renamed
re-export (`from .time import naturaldelta as delta`) is not followed. A star re-export
never carries a `_private` name, and when the source module binds `__all__`
at top level the name must be listed in it; any other binding of `__all__`
(an import, `del`, a loop target, a binding inside a conditional block, or a
value other than a literal list or tuple of strings or a `+=` of one) fails
closed, and a mention of `__all__` in a comment or docstring is not a binding.
When the initializer also binds the name to something else (a second import
under that name, a star import from another module that defines it, its own
definition, an assignment, `del`, a loop or `with` target, or any binding
inside a conditional block), the re-export is not followed, because the
reader does not order bindings. An unreadable or unparsable initializer
re-exports nothing.
Methods and module owners are never re-exported. The test must still call the owner's own name through
the package, so a test that only calls a sibling name from the same package
stays unrelated, and a test that binds a local named like the package alias
(a parameter, fixture or assignment) calls that local, not the package. Diff
mode and repo mode apply the same rule.
A package import also reaches a free-function owner through the remaining
submodule path (#4560): `import click` binds `click`, so
`click.utils._expand_args(...)` calls `src/click/utils.py::_expand_args`, and
`from click import _internal` reaches `_internal.utils.f(...)`. The receiver
must spell the owner's full dotted module path below the imported module; a
different submodule (`click.other.f(`) or a local shadowing the alias does not
match. An import of the owner's own module path reaches it the same way, which
covers an owner in a package `__init__.py` (`from dateutil import zoneinfo`
then `zoneinfo.get_zonefile_instance(...)`), whose file stem never matches.
Test-name and fixture-name proximity may provide a suggested repair location,
but these links must be marked uncertain, must keep weak reachability, and must
not promote unrelated assertions to strong revealability.

A top-level helper function that no test calls itself is related, with the
weak `helper_owner_call` relation, to a test that calls a same-module caller of
it (`Version.__init__` reaching `_validate_epoch`), following bare-name calls
for at most three hops in the helper's file. The relation is uncertain,
carries no oracle, and never reaches `exposed`; a duplicate helper name, a
parameter or local rebinding of the name, an attribute call, and comment or
string text make no edge, nor does a caller wrapped by any decorator
(including one whose name cannot be read, or a call form such as
`@property(x)`) other than a bare `classmethod`, `staticmethod` or
`property`, alone or combined, except that a top-level `click.command` or
`click.group` function (decorated only by those and `click.option` /
`click.argument`, with `import click` in its module) is a caller as well, since
its decorators build a command that runs the body; an `async def` caller is
allowed (#4568, #9226).
A top-level `click.command` or `click.group` function that no test calls is
related, with the weak `cli_runner_invoke` relation, to a test that passes it
as the first argument of `<runner>.invoke(` (`runner.invoke(hello, [...])`),
and, through the `helper_owner_call` hops above, so are the helpers it calls
(#9226). The test must name the command through the provenance-checked import
that every direct call uses; `<runner>` must be a local assigned exactly once
to `CliRunner(...)` imported from `click.testing` or `typer.testing`, or a
literal `CliRunner()` fixture that nothing else rewrites. A different module's
`CliRunner`, a non-first argument, an attribute of the command, a rebound or
parameter-shadowed name, a fixture with arguments or extra statements, an
undecorated function make no edge. The relation is uncertain, carries no
oracle and never reaches `exposed`: the command's output is not credited, and a
helper reached through the command is reach-only too (the invoke may never run
the body, e.g. `--help`). `import click as c` and `from click import command`
are not recognised.
A group subcommand gets the same weak relation by name (#9226): a top-level
synchronous function bound once and decorated by one `@<group>.command`
registration (plus `click.option` / `click.argument` only) is reached by a test
that passes the group, through the same provenance-checked import, as the first
argument of `<runner>.invoke(` with a literal argument list (positional or
`args=`) whose first element is the subcommand's name. The group must be a
top-level function bound once in the same file, decorated only by `click.group`
(without `cls=`), `click.option` and `click.argument`, with `import click` as
the file's only binding of `click`. The name is a string literal given to the
registration (positionally or as `name=`), or the function's own name lowered
with `_` as `-`; a function name ending `_command`, `_cmd`, `_group` or `_grp`
is refused, since click versions differ on stripping it. Every registration on
the group must be readable and distinct, the text `<group>.command` /
`<group>.group` must appear only as those decorators, and the file must not
spell `add_command` or `.commands`. A name after an option, a command-line
string, a name built at run time, an option on the registration other than
`name`, `help`, `short_help`, `epilog`, `hidden`, `deprecated` or
`no_args_is_help`, and a helper the subcommand calls make no edge. The
subcommand's `decorator_indirection` limit is lifted only while such a test
exists; with none, the owner keeps the limit.
An owner left `no_static_path` while a test that imports its module (or a
package above it) calls `.invoke(` on a recognised fresh `CliRunner` keeps its
class but replaces the generic "No Python test references" line with one
naming that test and saying ripr does not follow into command internals, and
withdraws the add-a-test next step (#11069). No recognised invoke, no change.

Known limits, all weak-only: a module-level rebinding such as
`helper = wrap(helper)` is not detected, and a call inside a nested `def` or
lambda counts as an edge of the enclosing owner.

The call-site lexer behind direct owner calls and helper edges hides comment,
string and docstring text. A backslash continues an ordinary string over the
next line break, which may be CRLF, so a quoted `"""` on the continuation line
opens nothing and a call there stays hidden. The `{...}` replacement fields of
a triple-quoted f-string (prefix `f`, `F`, `rf`, `fr`, any case) are code: a
call on a line of a multiline field is visible, while the string's own text, a
`{{` escape, a string nested in a field (single-line or a complete
different-delimiter triple-quoted literal, newlines and braces included), the
literal format-spec text after a field's top-level `:` (only a nested `{...}`
inside the spec is code again), and a field not closed before the string ends
stay hidden (#8787). A backslash before a brace in an f-string is a literal
character on its own and the brace lexes normally. A nested f-string literal (up
to four levels) in a field has its own replacement fields scanned by the same
rules, so only real field code is visible; its own text, `{{`, spec text, and a
plain or `b` nested literal are hidden locally while a later call in the same
field stays visible. The scanner gives up and hides the rest of the text only
for a `#` comment in field expression text (Python 3.12), the string's own
delimiter nested in a field (3.12), a single-line f-string with a field of its
own inside a field (3.12 same-quote nesting), a field or nested literal that is
not closed, and nesting deeper than four levels. A colon that may or may not be
the spec colon hides. The triple-quoted scanning here gives up and hides
rather than inventing a call for the cases above. A top-level single-line
f-string (any `f`/`rf`/`fr` prefix, either quote) whose quote pairing cannot be
followed also hides the rest of the text: its own quote nested inside a
replacement field (Python 3.12: `f"{ "helper(1)" }"`, `f"{y["helper(1)"]}"`) or
a newline inside a field (`f"{\n y\n}  helper(1)"`). Pairing that does follow
(`f"{x}"`, `f"{d['k']}"`, `f"{{"`, a nested spec field) typically keeps a real call after
it visible; a brace or quote inside a nested string literal in a field
(`f"{'{'}"`) or a quote in format-spec text can over-hide, which is a miss and
fails closed (#9230). Known limits: inside
triple-quoted f-strings a replacement field on the same line as the opening
quote is hidden by the line-prefix quote check, a real call after a field the
scanner gave up on is hidden too, and a real call after a single-line f-string
the pairing cannot follow is hidden (misses).

A pytest test that names a module-level `@pytest.fixture` function (in its own
module or a `conftest.py` up its directory chain, nearest wins) as a parameter
is related, with the weak `fixture_owner_call` relation, when that fixture's
body calls the owner by the rules a test body uses (#4563). One hop only; a
renamed fixture (`name=`) and a test that rebinds the parameter name make no
edge, nor does a parameter that a pytest `parametrize` names as an argname,
read syntactically (variable argvalues, `pytest.param(marks=...)` and
unreadable `indirect` values still count) unless `indirect=` is the literal
`True` or a literal list naming it, directly or through one never-rebound
module `NAME = <value>` binding (#10685). The mark, its argnames and its
`indirect` are read through imports and module names as RIPR-SPEC-0238 L8
describes. A class-level `parametrize` (on the test's
class or a local ancestor) counts the same way. When a file defines the same
fixture name twice the last definition wins, and the analyzer attaches at most
eight fixture definitions to one test (an implementation limit, not a pytest
rule). A later ordinary `def` of the same name replaces the fixture and
is not attached. Not covered: module-level or class-body
`pytestmark = pytest.mark.parametrize(...)`, aliased or bare parametrize forms (`import pytest as pt`, `from pytest.mark
import parametrize`), fixtures defined inside a class, `mock` or rebinding
inside the fixture body, parameters with defaults or supplied by `@patch`,
`autouse` fixtures and fixtures of fixtures. The relation is uncertain,
carries no oracle, and never reaches `exposed`.

A Python test is related to an owner only when the test references the owner.
Same-stem file, test-name, and fixture-name proximity only rank a test that
already references the owner without a recognized call shape; they never
relate a test that only exercises a sibling owner in the same module, so a new
owner that no test references reads `no_static_path`. A reference is outside
comments, strings, and docstrings:

- function or class owner: the bare owner name (`handler = loyalty_price`),
  unless the test binds a local of that name (a parameter or fixture, an
  assignment, a walrus `:=`, a `for` loop target, an `as` target, a nested
  `def` or `class`) or the name is a keyword-argument or
  assignment target; a renamed import local
  (`from pricing import loyalty_price as lp`, then `lp`) whose source module is
  the owner module; or a module-qualified member through an import of the
  owner module (`import pricing` then `pricing.loyalty_price`, or
  `import pricing as p` then `p.loyalty_price`);
- method or class-method owner: an attribute reference `.name`, consistent
  with the direct rule that relates any `.name(` call for these owners;
- dunder method owner (`__init__`, `__eq__`, `__setitem__`, ...): only a
  reference to its class. Every class defines the same dunder names, so a
  test-local `def __init__` or `super().__init__(...)` in a helper class is
  not a reference. Constructing the class (`Class(...)`, a renamed import, or
  `module.Class(...)`) relates `__init__`, `__new__`, and `__post_init__` as
  `constructor_call`, an oracle-eligible relation; for any other dunder it
  relates as `dunder_protocol`, which stays uncertain because syntax cannot
  bind `obj[key] = value` or `a == b` to the constructed instance. The one
  exception is a builtin protocol entry applied to a sole argument bound to the
  owner class (`reversed(Class(0))` for `__reversed__`, likewise `len`, `bool`,
  `iter`, `next`, `hash`, `str`, `repr`): it relates as `protocol_entry_call`,
  an oracle-eligible relation, and only when an assertion takes the entry as an
  asserted operand (an unasserted entry elsewhere in the body does not relate).
  The entry, the `list`/`tuple` wrapper and the receiver must keep their
  bindings: a builtin shadowed by an import, a module or test-local binding
  (including tuple and annotated targets), or a receiver rebound by a chained,
  bracketed (spaced or nested too: `( r), x = ...`, `((r), x) = ...`,
  `for ((r), x) in ...`; #9178; non-adjacent single-line targets
  `(x, (r)) = ...`, `[a, [r]] = ...`, outer/chained targets and inline
  loop/with header targets too; #9531), loop, `with`, walrus or augmented target
  (`(r) += ...` and `(r): list = ...` included), or not the whole bound value,
  is not credited, as is an augmented assignment after a header or `;`
  (`if c: r += 1`), an `import r`, `nonlocal r`, `global r`, `del r` or `type r = ...`, and any `case` pattern line that
  mentions the name other than as a dotted value or class pattern (`case (r):`,
  `case {"k": r}:`, `case Foo(x=r):`; a guard read such as `case x if r:` is
  demoted too, a deliberate miss). Within one
  bracketed target, a comment, a backslash continuation or a run of blanks
  before the name, its closing bracket or the `=` / `in` / `as` after it, and
  a run of blanks after `for`, `as`, `def` or `class`, do not hide the rebind.
  A `for` or `as` inside a string or after a `#` comment is text, so
  `assert "for" and ((r)) in [r]` stays a credited read. Loop-keyword masking
  retains earlier lines for multiline literals while searching only the current
  line; an unterminated literal supplies no parsed test or protocol-entry credit.
  Known limits, none of
  them closed by this: a non-adjacent assignment inside an inline compound
  body (`if c: (x, (r)) = ...`) is outside the complete-target fallback;
  a multi-line tuple target whose earlier elements sit on
  other lines (`(x,\n r) = ...`) is not seen, so the receiver stays credited
  (the credit-keeping direction); and a dict key at the start of a continuation
  line (`{\n (r): 1\n}`) reads as an annotated target and withholds credit
  (a miss). A `:` ends a target
  position only when it closes a compound-statement header whose brackets
  balance outside string literals (single, double and tripled quotes, escaped
  quotes) and comments (`if ")": ( r), x = ...` and
  `if x == '''it's''': ( r), x = ...` rebind, a dict value `{"k": (r), 1}` does
  not). A header whose brackets do not balance, or that is split across
  lines, is not read as a header, so a one-line-suite rebind after it is not
  seen and the receiver stays credited (the credit-keeping direction, a known
  limit). Extra, keyword, starred or trailing-comma
  arguments are not the entry and fall back to `dunder_protocol`. A dunder
  owner with no related test, in a workspace whose tests import its class,
  its module, or anything from a package above it, reads `static_unknown`
  with the `dynamic_dispatch` limit instead of `no_static_path`: the adapter
  cannot bind a unittest mixin's `self.Cache`, a test-local subclass, or a
  public API that builds a private subclass (a descriptor's `__get__` behind
  a decorator) to the owner class. A nested class (`Outer.Inner`) is reached
  through its outermost class, `Outer.Inner(...)` after importing `Outer`;
- a changed or new default on a parameter line inside a multi-line `def`
  header is read like a one-line header change: each name on the line must
  be a declared parameter of the owner with a default, and when every strong
  related call binds that parameter the default is never reached and the
  line is not credited `exposed`. A keyword argument never binds a
  positional-only parameter. Methods, constructors included, fail open:
  subclasses and `cls(...)` factories construct through receivers the
  adapter does not see;
- module-level owner: a local bound by an import of, or from, the owner
  module.

A module-level `NAME = <literal>` edit (#9423) relates as `module_value_call`,
an oracle-eligible relation, only to a test whose straight-line `assert`
provably holds under the old literal and fails under the new one. The module
binds `NAME` once (no `global`, walrus, attribute write, star import or dynamic
namespace writer), the edit is the file's only changed line, no test file
writes the attribute, and the test keeps its imported names. Reader functions
must retain their binding too: same-module walrus/attribute replacements and
workspace foreign writes withhold the reader's credit. Attribute targets come
from the parsed statement tree, including parenthesized, unpacked, annotated,
loop/context and spaced targets; comparison reads and annotations without values
are not writes. An unparseable foreign source retains word-based write uncertainty;
references to its module name, malformed test/initializer/conftest files, hidden
loader spellings or package export uncertainty withhold credit globally. An
unreferenced malformed production module alone does not erase exact credit.
The workspace word index is bounded; overflow withholds credit. This is a bounded
source read, not a complete import or reflective-execution graph. The assertion
reads `NAME` itself or calls a module function with literal arguments whose
body is one `return` expression over its parameters, literals and `NAME`; a
name, call, operator or value kind outside that bounded evaluator decides
nothing and the edit keeps the `missing_import_graph` abstention. A body-only edit of
a top-level `NAME = lambda a, b: <body>` binding (#9619) credits the same way:
the parameters are unchanged positional ones without defaults, `*args` or
keywords, `NAME` is bound once in the old and new module, no file writes or
patches it, and the bound expression over the parameters and literals is inside
the same evaluator; a test that calls `NAME` with literal arguments in a
straight-line `assert` that holds with the old body and fails with the new one
is the oracle, and only then is the lambda-syntax limit lifted. A changed
parameter list, a default, a body outside the evaluator, a second binding or a
test that shadows, patches or may skip the call keeps `unsupported_syntax`. A value no
assertion distinguishes is never graded as a gap. A default edit of a field in
a plain `@dataclass` class (#9619) relates the same way: the class is a
top-level `@dataclass` or `@dataclasses.dataclass` with no arguments, bases or
keywords, imported from `dataclasses` and bound once; its body holds only
annotated fields, a docstring, `pass` and methods, with no `__init__`,
`__post_init__`, `__new__` or attribute hook and no `ClassVar`, `InitVar`,
keyword-only or `init=False` field; and the edited line swaps one literal
default under an unchanged name and annotation. A straight-line `assert` that
builds the class from literals, leaves the field unset and reads it through an
undecorated one-`return` method or an attribute is the oracle when it holds with
the old default and fails with the new one. For this read the evaluator adds floats
(`+`, `-`, `*`, `/`, comparisons and unary signs, declining integers beyond 2^53)
and the unshadowed one-argument builtin `round`.
An explicit value for the field, a field default that is `field(...)` or
computed, a method outside the evaluator, a rebound class or field, or a test
that shadows, writes or may skip the construction keeps `decorator_indirection`.
The class body must also run nothing the model does not evaluate: a field call
other than `field(default=<literal>)` or `field(default_factory=list|dict|set)`
with the builtin unbound, an annotation other than an unbound builtin name, a
non-literal parameter default or non-builtin annotation on any method (async
included), or a decorator other than an unbound `property`, `classmethod` or
`staticmethod` keeps the abstention, and a test that requests a fixture is not a
reader (directly parametrized arguments are not fixture requests).
An operator swap among `+`, `-` and `*` in the single `return` of an
undecorated module function that calls an imported workspace function (#9619)
is read without following the import: the line moves only the operator, so
the call returns the same whole number `X` before and after. The call is a bare
name that resolves to exactly one undecorated module-level function, bound once
and rewritten nowhere, over parameters and literals, and the other operand is
an expression over parameters and integer literals. A straight-line
`assert owner(<literals>) == <integer>` of a test that names the owner through
an import it does not rebind fixes `X` through the old operator (taking the
assertion to hold before the diff, as for any passing test), and is the oracle
when the new operator over that `X` gives another value; only then is the
`missing_import_graph` limit lifted. An assertion both operators satisfy, a
non-literal, keyword or short call, a second call, a division, a changed
operand, side or argument, a decorated, unresolved, ambiguous or rebound
callee or owner, a patched owner, any workspace file reachable from the owner's
or the callee's module through imports that defines `__add__`, `__sub__`,
`__mul__` or a reflected form (its result need not be the pinned number), or a
test that rebinds, writes or may skip keeps `missing_import_graph`. The
guarantee is bounded: the pass does not prove the call returns an ordinary
integer. An exact integer assertion is taken to describe the returned value,
and the abstention covers only arithmetic defined in reachable workspace
files. A third-party or dynamically built type with its own `+`, `-` or `*` is
not visible to this scan, and a pinned number it produced may not behave as the
algebra assumes. An edit that changes only the value of integer literals on a line that calls an
imported symbol (`islice(seq, 0, None, n)` to `islice(seq, 1, None, n)`,
`chunks[-1]` to `chunks[-2]`) lifts `missing_import_graph` on token equality
alone: the old and new lines must lex to the same tokens except integer
values, so no callee, keyword, operator, operand, string or float moved (#9226).
The abstention is lifted only when a strong exact oracle observes the owner;
no test, a smoke assertion or an assertion on another value keeps it. The callee
is not evaluated, so a strong exact oracle that reaches the owner is
not credited as `exposed`: the finding is `weakly_exposed` with no nameable
discriminator, no canonical gap or flow sink, no `suggested_*` repair card and no
next step. Any other limit that applies (rebind, dunder dispatch,
heuristic-only relation, `caplog` evidence) still wins. Without the old
line the limit stays.
A function defined under a
decided module-level `if sys.version_info >= (3, 0):` condition (the only
decided shapes are Python 2/3 version tuples, with `sys` the single plain
import) is an ordinary function owner when its name has no other live
binding (including walrus replacements), and no module-source write or
uncertainty may replace `sys.version_info`, including an annotated attribute
target. A definition in the dead arm stays with the `<module>` abstention.
The workspace pass also withholds conditional-derived owners when another file
may replace the callable or `sys.version_info`. Physical continuation indentation
never makes a trailing `return` safe before the asserted value read; ordinary
multiline setup remains eligible. A generator test lends no executed-body credit.
Formatted strings remain uncertain in this bounded admission because their
expressions can carry generator syntax. Statement identity uses the reported
source assertion, independently of the boolean oracle's normalized gate text.
A workspace assignment to a function's `__code__`, `__defaults__`, `__kwdefaults__` or
`__closure__` can replace a reader without rebinding its name, so it withholds the module
constant's credit for scalar and table constants alike (#10626).

A boundary input that a test passes as a plain name resolves like a literal
when the name is bound once in the test body, by a top-level single-target
assignment of a literal before the call (#10707). The test is parsed; the name
is refused if it is a parameter, is stored, deleted, imported, declared
`global`/`nonlocal`, bound by a `for`, `with`, `except`, walrus, comprehension,
nested `def`/`class`/`lambda`, tuple unpack or augmented assignment, or if the
body contains a `match`. A refused name stays unresolved, so the verdict is
the weak one.

The same input also resolves when the name is a scalar-literal constant of the
test's own module (`THRESHOLD = 50` above the tests, #10707). The module must
bind it exactly once at top level, with no `global` rebind, no attribute
assignment of it, no dynamic namespace write and no opaque construct; the
test must not bind the name in any scope (parameter, store, loop or
comprehension target, lambda or nested parameter, import, `match`); the body
must not mention `monkeypatch`, `patch`, `setattr`, `globals`, `vars`, `exec`
or `eval`; the module must hold no code that runs outside the tests (autouse
fixture, `pytestmark`, setup or teardown hook, module-level call, or a
`conftest.py` on the directory chain that declares a fixture or hook); and a
test file that the diff also changes gets no credit. Anything else keeps the
input unresolved. A plain write to the constant from another file that runs
before the test (a sibling test module assigning `module.THRESHOLD` at
import) is not seen; no extra fact is read for it.

A changed line whose old and new text differ only in a trailing `#` comment or
in whitespace between tokens emits no probe (#10706). Both lines are lexed and
compared by the source spelling of each token, so the statement is the same
code; a `#` inside a string, a respelling (`0x1` to `1`, a requoted string), a
changed token or a changed indentation is still an edit, and a line that does
not lex alone (part of a multi-line construct) stays analyzed. A line inside a
string literal that crosses a line break is string content, so the guard does
not apply there, on either the new line or its paired old line (read from the
reconstructed old source); a file that cannot be lexed to rule that out gets no
guard. No gap id or
discriminator can then embed comment text.

A one-line module-level dict literal of unique `str` or `int` keys and scalar
values (#9619) is a constant of the same kind, read only by a module function
through `NAME.get(key[, default])` or `NAME[key]`; a test reading the table
itself, a `bool` lookup key, a missing key without a default, a computed or
spread entry and a multi-line literal decide nothing. The table is mutable in
place, so credit also requires that no source mention `NAME` other than as an
import or export, a `.get` read or a subscript read on a line that binds
nothing (the owning module's defining statement aside): a store through a
subscript, `update`/`setdefault`/`pop`, an alias, a renamed import, an
f-string mention (multi-line included), a statement sharing a `;` line with an import, a lexing error, or a file using a reflective spelling (`getattr`, `dir`,
`getmembers`, `locals`, `__globals__`, `attrgetter`, `eval`, `compile`, `inspect`, `gc`,
`__code__`, `__defaults__`, `__kwdefaults__`, `__closure__`) that mentions the table or its
module withholds credit. A reflective file withholds credit for the tables of every module in its import
component (files linked by an import word naming another file's stem or package `__init__.py`, in either
direction, plus every `conftest.py` component; import statements are read across parenthesized and backslash continuations by the lexer, so a bracket in a comment or string does not end one), because the module it reflects over may be another
file's re-export, or be handed to it as an argument, with no word of the file naming it. A reflective
file that also loads modules by a runtime name (`importlib`, `__import__`, `sys.modules`, `eval`, `exec`, `compile`, ...) withholds
credit for every table. Reflection over modules outside that component keeps the credit. A definition after a `;` is discovered like one at the start of a line, so an explicit
line continuation cannot hide a table from the foreign-store scan.
Preceding assertions must evaluate true under the same bounded old/new scope
before a later assertion can supply the value-flip credit.

A scalar or table constant's reader may also be reached through one forwarding
hop in a sibling module (#9619). A test that imports `quote` with `from
pkg.quote import quote [as name]`, where `quote`'s body is one `return` calling
`lookup` bound by `from .table import lookup [as local]`, relates as
`module_value_call` when the assertion holds through the old reader and fails
through the new one. The conditions are:

- the owner module is not `__init__.py` and sits in a regular package (its
  `__init__.py` is a workspace source), does not mention the forwarder
  module's name, and loads inertly: its top level holds only a docstring,
  `pass`, `from __future__` imports, assignments of literals (dict, list and
  tuple displays of constants included) to plain non-dunder names or
  `__all__`, with an optional annotation that is a string or, under
  `from __future__ import annotations`, a bare name, and undecorated `def`s
  with literal defaults and annotations of the same kinds.
  It imports nothing else, not even a standard-library module, because a
  test-root module can take a standard-library name and calls such as
  `pickle.loads` or `pydoc.locate` import by a string. Loading it therefore
  never imports the forwarder module;
- the forwarder module is a non-test sibling in the same directory (not
  `__init__.py` or `conftest.py`) whose source spells `from .<stem> import`; at
  most 16 such siblings are read per changed file, and more reads none;
- loading the forwarder module runs nothing else: its top level holds only a
  docstring, `pass`, `from __future__` and level-1 `from .<stem>` imports,
  assignments of literals to plain names other than dunders (`__all__`
  aside, so `__package__` cannot redirect the relative import), and
  undecorated `def`s whose defaults are literals; an annotation is absent or
  a string, or a bare name under `from __future__ import annotations`, so
  loading never evaluates one;
- the local name and the forwarder are each bound once in that module; the
  forwarder is a top-level `def` with positional parameters and no defaults,
  and it calls the local name with positional arguments only;
- the test reaches the forwarder only through a `from` import of the
  forwarder's own name whose module path ends with the forwarder's package
  directory and module (`rates.quote`), never through a module receiver,
  which a package `__init__` may rebind, or a path that drops the package,
  which breaks the relative import; its module path does not start with a
  standard-library name;
- the forwarder module gets its own write view of every other workspace file:
  a write of the forwarder or the local name, a code-slot or dunder write,
  reflection or a non-byte-exact name anywhere, or another file, extension
  module or compiled package sharing the forwarder module's name withholds
  credit, as do reflection, a dynamic writer or a dunder assignment in the
  forwarder module's own source, and a shared name of the owner module;
- inside the forwarder only its parameters are bound (the owner's constant is
  not), and the reader it calls is evaluated with no forwarder of its own, so
  a second hop decides nothing.

Along a module's dotted path from any import root, read from its whole path
and from the path below a `src` layout root (any depth) or a top-level `lib`
root, a package with an initializer of any kind or a source module named for
a leading component that is not the module's own ancestor
(`tests/shapes/__init__.py` or `tests/shapes.py` for `shapes.rates.quote`)
shadows the module's package under pytest's prepend mode. An import root
that is itself a package below the workspace root (it holds `__init__.py`,
as `tests/` does when `tests/__init__.py` exists) is skipped by this rule:
pytest inserts the first directory above a test that has no `__init__.py`,
so it never puts a package directory on `sys.path`. It withholds the
credit of every pass that refuses a shared module name. A shadow reached only
through another suffix of the path is not checked.

A change to the single `return` of an undecorated module function or method
that no test calls directly (#9226) relates as `module_value_call` to a test
whose straight-line `assert <entry> == <literal>` holds with the old body and
fails with the new one, evaluated through a chain of model callables. The
changed line is the file's only changed line. A model callable is a top-level
undecorated synchronous `def` with plain positional parameters (no defaults,
`*args`, keywords, or annotations other than a bare name) whose body is one
`return` after an optional docstring, bound once in the module, with no
`yield`, `await` or `:=`. It may also be such a method of a model class: a
top-level class with no bases, keywords or decorators, bound once, whose body
holds only a docstring, `pass` and `def`s, with no method whose name starts
with `__` (dunder or name-mangled), no method defined twice or bound by a
walrus or `global`, and no `def` carrying a decorator, a default, or an
annotation other than a bare name or string (each runs while the class body
does), built as `Class()` with no arguments. Inside a callable a name is a parameter, and a call is a bare call
of a model function or class of the module, or a method call on an instance
of a model class (the method's receiver, a parameter holding one, or
`Class()`), all with positional arguments. Values
and operators are the module-value evaluator's integers, booleans and strings,
plus `str * int` repetition (at most 4096 characters) and `.upper()`/`.lower()`
of an ASCII string. The test reaches the entry through a function or class it
imports with `from M import name` from the owner's module and does not rebind
(`import pkg.mod as alias` and `from pkg import mod` read `mod` as a package
attribute, which `pkg/__init__.py` may rebind, so they do not count), or through a local it binds once to `Class()`
of a model class before the assertion and uses only as a method receiver (a
local bound to a model function's result runs that function, so it is not one). The test is
undecorated, defined once in its file, requests no fixture, has no autouse
conftest fixture that may skip it, and runs nothing before the assertion but
other assertions, `pass`, a docstring and its local bindings. A workspace file
that writes or patches any name the chain runs, a code slot, or any dunder
attribute (`__new__`, `__getattribute__`, ...; in the owner module too)
decides nothing, and the finding keeps its verdict. So do `__class__`,
`__bases__` or `eval` in the owner module, a call nesting deeper than 16, more
than 4096 calls, and any name, call, operator or value outside that model.

Reflection refuses the credit wherever it appears, in the owner module or in
any other workspace file: a reflective spelling (`getattr`, `dir`,
`getmembers`, `locals`, `inspect`, `gc`, `exec`, `compile`, `attrgetter`,
...), a frame or a function's globals (`__globals__`, `f_globals`,
`f_locals`, `_getframe`, `currentframe`, `f_back`, `tb_frame`,
`get_referrers`, `get_referents`), `gc.get_objects`, `setattr`,
`__setattr__`, `delattr`, `__dict__` and `vars`, or the import system
(`sys` with `modules` or `path`, however imported, `addsitedir`,
`meta_path`, `path_hooks`,
`path_importer_cache`, `__path__`, `pkgutil`, `extend_path`, `importlib`, `__import__`,
`import_module`, `exec_module`, `reload`). Such code can write a module
namespace under a name built at runtime, or hand an importer a different
module, so no name it mentions bounds what it changes. Another workspace file
with the owner's module name (its file stem, or its package directory for
`__init__.py`) refuses it too: under pytest's default `prepend` import mode,
an extra import root, or a package `name/__init__.py` beside `name.py`, the
test's import of the owner's dotted path may load that file instead. So does
a compiled extension module of the owner's name beside it (`ledger.*.so`,
`ledger.so`, `ledger.pyd`, or `__init__.*.so` for a package's `__init__.py`),
which the import system loads before the source.
A non-ASCII character in a name or an f-string, in the owner module or any
other workspace file, refuses the credit: Python NFKC-normalises identifiers,
so a fullwidth `_ｐad` rebinds `_pad`, while the credit's name matches are
byte-exact. So does a PEP 263 encoding declaration other than UTF-8 in any of
those files: in UTF-7 an all-ASCII comment can decode to a second line of
code. A NUL byte or a carriage return not followed by a newline refuses it
too: CPython breaks lines on a lone `\r`, so it and the scan disagree on
which line holds a declaration. An unchecked-hash pyc (PEP 552) of any
workspace source refuses it as well, since CPython loads it without comparing
it with the source. Timestamp and checked-hash pycs are trusted the way
CPython trusts them; the residual risk is a stale timestamp pyc after a
same-size edit whose mtime lands in the compile second or is restored
(`rsync -t`, `tar`, CI caches), which CPython itself would load. A pytest or
tox configuration (including `pytest.toml` and `.pytest.toml`) beside any
workspace source, or above one, whose
`pythonpath`, `python_paths` or `PYTHONPATH` setting names anything but its
own directory refuses the credit: any other entry (an archive, a directory
the workspace walk excludes, a root holding only compiled extensions, a path
outside the workspace, an expansion such as `{toxinidir}`, or entries that
cannot be read off its lines) can hold code the scan does not read.
`pythonpath = ["."]` keeps it. Any `addopts` refuses too, since its shell
split `-o pythonpath=...` can be quoted past any word match. TOML
configurations are always parsed, so an escaped key counts, every
`pythonpath` key at any depth counts and a mention outside such a key
refuses; an INI value runs on across blank and comment lines, as iniconfig
reads it. A compiled extension or sourceless `.pyc` in any directory that
holds a source or lies above one (pytest's prepend mode can put any of them
on `sys.path`) named like the owner or one of its packages (a
compiled `__init__` counts as its package) refuses it, and so does one in a
package directory below such a root along a dotted path the owner can be
imported by (`tests/reach/ledger.pyc`, which pytest's prepend mode puts
ahead of `reach/ledger.py`), as does a package directory named for the
whole path with an initializer of any kind (`tests/reach/ledger/
__init__.pyc`) other than the owner's own package. A test import whose
top-level module is a standard-library name (`from math import floor`)
never credits a workspace `math.py`: the built-in and frozen importers run
before any path entry, and pytest has already imported many pure-Python
stdlib modules.
What the environment supplies is outside the static claim and stays a
residual risk: a `PYTHONPATH` or `PYTEST_ADDOPTS` set by the caller (a
Makefile, a noxfile, CI configuration, `.envrc`), a `.pth` file in site packages, or an installed
non-editable copy that wins over the checkout (#10969). A test whose module has header effects is
refused like one under a conftest with effects. Because these are
whole-word matches over every workspace file, the credit rarely applies in
repositories that use reflection anywhere; that is missed credit, not a
wrong verdict.
`__setattr__`, `delattr` and the frame words, like `setattr`, also mark a file
as a dynamic writer for every module-value pass.

A `def name(`, `async def name(`, or `class name(` header in a test defines a
local; it is not a call of `name`.

A title, a fixture name, a file stem, or an object-member use such as
`order.loyalty_price` on a receiver that is not the owner module is not a
reference to a free-function owner.

## Probe Facts

Probes the adapter must generate (syntax-first):

- predicate probes for changed `if`/`elif`/conditional-expression
  boundary conditions
- return-value probes for changed `return` and final-expression shapes
- error-path probes for changed `raise` statements and `try`/`except`
  shapes
- field probes for changed attribute assignments
- call probes for changed function and method calls including argument
  changes
- mock-interaction probes for call surfaces resolved through a syntactic
  `mock.Mock()` / `MagicMock()` initializer

The adapter must not emit a probe for a change that both source versions parse
as an edit inside a real module, class, function, or async-function docstring.
It must derive this from AST string-expression spans in both source versions,
not from triple-quote text alone. Assigned strings, f-strings, and a behavioral
line replaced by a newly introduced docstring remain analyzable.

The adapter must not emit a probe for the one-line `def` header of a new
function (no removed counterpart) whose body holds another added behavior
line: the body lines carry its behavior. A header with a default value, a
changed header, a one-line `def f(x): return x`, and a multi-line header keep
their probe.

In diff mode the adapter must not emit a probe for a line of a multi-line
`def` header that only names parameters or opens or closes the header
(`self,`, `key: int,`, `*args,`, `*,`, `def name(`, `):`, `) -> bool:`),
when the paired old line, if any, has the same shape. A parameter default, an
annotation with a call, a trailing comment, and a `)` that closes a call in a
body keep their probe.

When the adapter cannot classify, it emits one of the `static_limit_kind`
values defined in RIPR-SPEC-0026:

- `dynamic_dispatch` (e.g., `getattr(obj, name)(...)` or mapping lookups such as `dict[key]` followed by invocation)
- `metaprogramming` (e.g., `class ...(metaclass=...)`, `type(...)`,
  or `__getattr__` indirection)
- `missing_import_graph` (the symbol is imported from a module the
  adapter cannot resolve syntactically, or the changed line uses dynamic import
  syntax such as `importlib.import_module(...)` / `__import__(...)`)
- `decorator_indirection` (the decorator changes the call semantics in a
  way the syntax-first adapter cannot follow; simple route decorators such as
  `@app.get(...)`, `@api.post(...)`, or `@router.api_route(...)` may be treated
  as static route metadata when the changed behavior itself is a supported
  repair shape; `functools.lru_cache` (bare or called with arguments), and
  bare `functools.cache` / `functools.cached_property`, including the same
  names qualified as `functools.*`, are not this limit when they bind from
  `functools`)
- `mocked_module` (e.g., `@patch(...)`, `patch.object(...)` (#4565) or `monkeypatch.setattr(...)`
  observed at the related-test call site)
- `opaque_custom_assertion_helper` (e.g., a related test observes the changed
  owner only through an `assert_*(...)` helper body the adapter does not
  inspect; a same-file helper that is only `assert` statements, called with
  literal arguments from a plain test, is not this limit: see the helper
  inlining amendment below)
- `property_based_test` (e.g., a related test uses Hypothesis `@given(...)`
  generated inputs whose concrete discriminator coverage is not statically
  known)
- `unresolved_pytest_fixture` (e.g., a related pytest test uses fixture-sourced
  input or expected values whose concrete discriminator coverage is not
  statically known)
- `unsupported_syntax`
- `python_transitive_reach_unresolved` (RIPR-SPEC-0201: a test constructs or
  calls into the owner's class and a bounded same-class `self.` / `cls.` path
  may reach the changed method; classification stays `no_static_path`)

## Canonical Gap Identity

For non-static-limit Python preview findings, the adapter emits an optional
canonical gap identity that avoids line-number-only matching:

```text
gap:python:<normalized/path.py>:<owner_path>:<behavior_kind>:<probe_kind>:<normalized_discriminator>
```

The identity parts are also available as a structured `canonical_gap` object in
JSON output. `behavior_kind` is derived from the Python probe family, such as
`predicate_boundary`, `return_value`, `exception_path`, `field_value`, or
`call_or_output_effect`. `normalized_discriminator` is syntax-derived from the
changed predicate, return expression, raised exception, field assignment, or
call/output text after whitespace and punctuation normalization.

Static-limit findings keep their named `static_limit_kind` and may omit
`canonical_gap_id` until the repair-routing lane adds typed non-actionable
gap-state projection. This prevents dynamic or unsupported Python cases from
being mistaken for bounded repair work before repair cards and stop reasons
exist.

## Python RIPR Evidence

For non-static-limit findings, the adapter must express the same RIPR evidence
spine as other languages:

- reachability: related Python tests, direct calls, import-alias calls, or
  conservative proximity links
- infection: the changed Python behavior family, such as predicate, return
  value, exception path, field/object state, or call/output effect
- propagation: whether the changed behavior is already at an output boundary,
  can flow through an exception/control boundary, or can only weakly propagate
  through unresolved control, object, or side-effect flow
- revealability: the strongest extracted pytest or unittest oracle and whether
  it discriminates the changed behavior. A strong oracle is credited as
  discriminating â€” and the finding classified `exposed` â€” only when its
  assertion observes the changed sink: it must reference the changed owner (by
  name or import alias) or a changed-sink identifier/literal from the changed
  line. A strong oracle that reaches the owner but observes a *different* value
  (for example a wrapper's return value rather than the changed boundary)
  downgrades to `weakly_exposed` with a typed reason. Reach plus a strong oracle
  alone must not credit `exposed`, or the classification degrades into coverage.

A changed relational predicate (`<`, `<=`, `>`, `>=`) additionally follows the
Rust activation boundary rule. When a strong related test calls the owner with
at least one literal argument, `exposed` requires one of those calls to bind
both comparison operands to equal values: each operand is a literal or an owner
parameter bound to a literal argument or literal default (positional, keyword,
import-alias, and method calls after `self`). Literal owner arguments, and the
boundary equality when observed, are recorded in `observed_values`. Otherwise
the finding fails closed to `weakly_exposed` and the infection stage reads
`weak`. A rewrite that keeps the predicate true on exactly the same integers
(`amount >= 100` -> `amount > 99`, one comparison of a name against an
integer literal, the rest of the condition unchanged) changes no behavior,
so a test must notice a wrong version of the new line. Python's mutation
tool shifts a number literal up by one (mutmut `n` -> `n + 1`), and for
`v > C` and `v <= C` (and their mirrors) that shifted bound first differs
at `v == C + 1`. There `exposed` also needs a strong call that places the
name at `C + 1`; otherwise the finding is `weakly_exposed`, and names
`v == C + 1` only when all relevant calls can be placed (#10842). `v >= C` and `v < C`
shift onto the equality point and need no further row. A behavior change
keeps the equality rule, because the equality row notices the edit itself
(RIPR-SPEC-0238 example 39). When every relevant strong call can be placed and
none witnesses the boundary, the boundary is named as the missing discriminator
(`amount == threshold`, with the observed
operand values in its reason). A resolved boundary witness remains valid even
if another call cannot be placed. If no witness is resolved, an unreadable
call or an operand ripr cannot bind (an attribute such
as `item.on_hand`, a computed `len(name)`, a comprehension local, or a line
with several comparisons) never counts as observed, and such a boundary is not
named as a typed repair target: the test input may already sit on it at runtime
(`reserve(Item("a", 3), 3)`), so the finding states the unresolved operand
instead of producing a repair card. A module-scope name bound once to a scalar
literal (`DISCOUNT_THRESHOLD = 10_000`) resolves like a literal operand, the
way the Rust and TypeScript adapters resolve a same-file constant; the missing
discriminator keeps the name (`amount == DISCOUNT_THRESHOLD`) and its reason
names the value and declaring line. A test argument that names the constant,
imported from the owner's module (`discounted_total(DISCOUNT_THRESHOLD)` or
`pricing.DISCOUNT_THRESHOLD`), binds to that value unless the test's own scope
or its module binds that name again. The name stays unresolved when anything
can rebind it: a second module-scope binding (including inside
`if`/`try`/`for`/`with`), a `global` declaration or walrus target anywhere in
the module, a star import, a module-scope `match`, a dynamic
namespace writer in code (`exec`, `eval`, `globals`, `vars`, `locals`,
`setattr`, `__setattr__`, `delattr`, `reload`, `patch`, `sys.modules`,
`__dict__`), whether called, named in an f-string, or named as a
whitespace-free string such as `getattr(m, "exec")` (a word in a comment or in
prose, like a docstring saying "env vars", is not a writer, #10979; a module
the lexer rejects falls back to the whole-text scan; for the names a
`parametrize` decorator reads at import, that is the `pytest` root, a `@cases` name and
argnames, a writer inside the body of a `test*` function or a `test*` method of a
`Test*` class is not counted because it runs after every decorator, while module
scope, class bodies, helpers, fixtures and decorator arguments still count, and a
test function named outside its own `def` keeps the whole-module scan; constants,
object and table bindings read at call time keep it too, #11018),
a non-literal value, a parameter or local binding
of the same name in the owner, a nested `def`/`class`/`lambda` in the owner,
or a related test file that assigns the attribute (`pricing.DISCOUNT_THRESHOLD
= 5`). A call inside a `@pytest.mark.parametrize` (or `@mark.parametrize`)
test whose argument is a
parametrize argname (`bulk_discount(quantity)`) expands into one call per
generated case, binding each case's literal argvalue (#4559). Cases are
recorded only when statically certain: string or list/tuple argnames, a
list/tuple argvalues literal with rows of the right arity (`pytest.param(...)`
unwrapped), stacked decorators as a product of at most 256 cases. A
single-name list/tuple argnames takes tuple rows, as pytest unpacks them.
Another decorator named `parametrize` (a plugin or project helper) records
no cases; the mark is read through imports and module names as
RIPR-SPEC-0238 L8 describes, and a mark whose root that read cannot pin to
pytest records none for the whole test (#10867). Argvalues named by a variable, `indirect=`, starred rows, a `pytest.param`
with any keyword but `id=` (a `marks=` skip or xfail case may never run), a
unittest method (pytest does not parametrize it), and an argname the test
body may rebind leave the call unresolved. A test whose first statement after
a docstring unpacks one argname (`quantity, rate = case`, alone on its lines)
reads as parametrized by the target names, each case's row split by arity,
and the unpack is not a rebinding (#10832). Every row must be a tuple of
scalar literals (a bare target also takes a scalar literal) and every
decorator a pytest mark; a list row (mutable, so a fixture may change it), a
row naming a value (which may shadow the owner), a row of another arity, a
target that is another parameter, and a later statement that names the
argname or rebinds a target keep the argname's cases as they are. A rebinding is read from the
parsed test body: any assignment, tuple, loop, `with` or `except` target, an
import or `del` of the argname drops it from the cases, and a nested `def` or
`class`, a `match` or a star import drops all cases. Any `lambda` in the test,
or a comprehension whose `for` target names the argname, also leaves the call
unresolved. When no strong related call binds a literal
argument (test locals, `*args`, a construct-call passing a dict), static
evidence cannot see the activating input either way: the oracle verdict stands
and an `exposed` finding carries a `boundary_activation_unresolved` evidence
line naming that limitation.

For an adjacent `>`/`>=` or `<`/`<=` while-bound edit with an unchanged
integer bound, a closed generator walk can resolve a rebound parameter. It
accepts only an owner consisting of the loop, whose body is `yield name`
followed by one unconditional `name -= 1` (descending) or `name += 1`
(ascending). Every strong call must bind a readable integer and occur in a
direct whole-list or whole-tuple equality against an integer-literal sequence.
The builtin constructor and imported free-function owner must retain their
identity. A near-side or equal input reaches the bound; a past-bound input
does not (#9733). Partial/lossy consumers such as `next` and `sum`, slices,
unobserved calls, assertion-message calls, nested calls, shadowed constructors,
and unsupported assertion shapes retain the unresolved limitation: potential
iteration alone does not establish observation of the changed final element.
Step-two, conditional, early-exit, pre-loop rebinding, other statements and
unreadable calls retain the existing unresolved boundary limitation. This is
not general loop evaluation.

Static-limit findings must fail closed. They keep any observed reachability and
oracle facts, but their infection and propagation stages remain `unknown`, the
finding class is `static_unknown`, a typed stop reason is emitted, and no
canonical repair-gap ID or repair recommendation is emitted.

Direct weak findings may also carry activation-level missing discriminator
facts for the first preview repair classes. For example, a changed
`if amount >= threshold:` predicate can emit `amount == threshold`; a changed
`return 42` expression can emit `return value == 42`; a changed
`raise ValueError("positive required")` path can emit
`raises ValueError matching "positive required"`; a changed
`self.status = "paid"` assignment can emit `self.status == "paid"`; and a
changed `logger.warning("coupon expired")` call can emit
`log contains "coupon expired"`. A missing discriminator never restates the
changed production expression as its own oracle: an assertion such as
`result == sum(i.quantity for i in self.items) + 1` passes for every mutant of
that expression. The expected side of a returned value, returned-dict field,
constructor keyword, or plain assignment is kept only when it is one
independent literal (a single string, including one triple-quoted string,
number, `True`, `False`, or `None`; a string-delimited compound such as
`"Hello, " + name + "!"` is not one, and adjacent-string concatenation such
as `"a" "b"` is conservatively not one either).
Otherwise it is the `<expected value>` placeholder
(`return value == <expected value>`, `self.total == <expected value>`,
`result.total == <expected value>`), and no concrete expected value is
claimed. These facts are evidence only until a later
repair-card contract supplies the test shape, verify command, receipt command,
and edit boundaries. Heuristic-only links, no related-test paths, and static
limits must not emit repair guidance. "Repair guidance" here means an
actionable repair target: a repair card, agent packet, placement, test node,
or verify/receipt command. It does not preclude generic non-repair-ready
advice: for a directly resolvable plain-function owner with no statically
linked test call, the adapter may name the owner and the honest ordered
route (add a relevant test/assertion, run the project's native verifier,
then refresh) while repair readiness stays suppressed (#10708). A decorated,
routed, command-registered, or otherwise indirected owner stays silent, and a
later transitive-reach limitation silences the advice again.

### Sink alignment visibility

The adapter already decides `exposed` vs `weakly_exposed` by whether a strong
oracle observes the changed sink (the revealability rule above). It must also
**surface** that decision so a consumer can see *why* a strong oracle did or did
not credit `exposed`, via four additive optional output fields. They are a pure
read-out: the boolean the classifier uses is derived from the surfaced
`oracle_alignment`, so the visible value can never disagree with the decision.

- `changed_sink` â€” the comma-joined significant tokens of the changed line.
- `observed_sink` â€” the strongest related oracle's assertion text.
- `oracle_alignment` â€” a controlled enum mapping to the existing branches:
  `direct` / `alias` / `changed_sink_token` appear only on `exposed` findings
  (strong oracle observes the owner name, an import alias, or a changed-sink
  token); `orthogonal` appears only on the fail-closed `weakly_exposed` branch
  (strong oracle observes a different sink); `unknown` covers every other
  finding (no strong oracle, or a `<module>` owner with no usable token).
- `alignment_reason` â€” a stable snake_case token explaining the value
  (e.g. `strong_oracle_observes_different_sink`).
  `strong_oracle_observes_owner_call_through_module` (`direct`) credits a free
  function whose strong oracle calls it through a module-identified spelling
  (`utils.sign(0) == 0`, `pkg.utils.sign(...)`, a function-local import) or
  asserts a local the same test bound once to such a call as its whole value
  (`result = utils.sign(0)`, not `result = utils.sign(0) or 1`; #4567). The
  call or local must be an asserted operand: nested in another call
  (`always_true(utils.sign(0))`) it is not the compared value. A later import
  of the same name inside the test replaces the earlier binding, and the
  test's own `import pkg.mod as alias` is not a rebinding of `alias`. Those calls also bind boundary activation, so the
  relational-boundary gate still applies to them.
  `strong_oracle_observes_owner_protocol_entry` (`direct`) credits a dunder
  method owner whose strong oracle takes the builtin protocol entry on a
  receiver bound to its class as an asserted operand
  (`list(reversed(Range(0))) == []`), directly or under `list`/`tuple` only.
  `sorted`, `set` and `frozenset` erase the ordering the dunder determines and
  are not transparent here; any other wrapper may ignore the entry
  (`always_true(reversed(Range(0)))` stays `orthogonal`) (#8563).
  `strong_oracle_observes_removed_type_guard` (`direct`) credits an `__eq__`
  edit that deletes the leading `isinstance(other, <own class>) and` from the
  method's only `return`, when the remaining expression is exactly
  `self.<a> == other.<b>` (no call, boolean operator or conditional that could
  skip the read) and `<b>` is an attribute the class stores itself
  (`self.<b> = ..`) that no builtin literal has, the class defines neither
  `__ne__` nor `__getattr__` and, read from the parsed class, names no base and
  is plain (a builtin base answers `!=` itself), and a protocol-entry test with
  no `try`, `with` or `raises` that could catch the error asserts
  `Class(...) != <int, float, str, bool or None literal>` (or `not ... ==`).
  Calling the edited method with such a literal raises `AttributeError`, so the
  assertion fails after the edit. A literal on the left, a list or custom
  operand, an early exit before the `return` or an `==` read that expects a
  match keep the `dynamic_dispatch` abstention. The builtin-attribute list is
  a finite catalogue of the Python 3.14 literal types, not a general claim
  about Python semantics (#9619, #9421).
  `strong_oracle_observes_changed_effect_channel` (`direct`) credits a
  changed write (`print`, a file write, a call on a passed collaborator, or
  a `self` field) whose strong oracle compares the channel that write
  reaches: captured output, the file read back, a mock call pinned with
  arguments, or the field read through a bound receiver (RIPR-SPEC-0238
  rule 17).

Effect-channel observations use native assertion and compared-operand spans.
The owner call must finish before the operand is read, including statements at
different columns on one line. Failure messages and weaker, discarded conjuncts
cannot supply a later read for an earlier comparison. Resolved assertion helpers
retain their call-site identity and require the owner to complete before their
arguments are evaluated.

These fields are advisory preview evidence; they do not change the
classification and do not claim runtime maturity. The contract does not bump the
JSON `schema_version`. Python's support tier remains governed by
[Support tiers](../status/SUPPORT_TIERS.md).

## Required Evidence

The Python preview contract is supported only when the implementation
can show:

- a fixture corpus pinning at least one example per owner kind above
- a fixture corpus pinning at least one example per oracle kind above
- a fixture corpus pinning at least one example per probe kind above
- a fixture corpus pinning at least one example per `static_limit_kind`
- fixtures cover plain `def`, `async def`, classes, methods, decorated
  methods, and module-level fixtures
- fixtures proving broad `pytest.raises` / `self.assertRaises` are recognised
  as weak error-path oracles while `pytest.raises(..., match=...)` /
  `self.assertRaisesRegex(...)` are recognised as exact exception observers
- a fixture proving unittest assertion argument shapes can identify field,
  output, and status-code oracles
- a fixture proving pytest and unittest related tests produce
  framework-shaped verify commands
- fixtures proving test-name and fixture-name proximity are related-test
  heuristics but remain explicitly uncertain
- a fixture proving a same-stem test that only calls a sibling owner is not
  related to an owner it never references
- fixtures proving Python preview findings carry stable canonical gap IDs
  across human, JSON, GitHub annotation, and SARIF output while static-limit
  findings remain limitation evidence rather than repair gaps
- fixtures proving non-static-limit Python findings carry RIPR infection and
  propagation evidence instead of placeholder unknowns
- fixtures proving static-limit Python findings fail closed as `static_unknown`
  with typed stop reasons and no repair recommendation or canonical repair-gap
  ID
- a fixture proving `functools.lru_cache` / `cache` / `cached_property` do not
  emit `decorator_indirection`, so a memoized owner with a direct exact test is
  credited and a twin whose test misses the change remains a gap, while `@retry`
  stays limited
- a corpus row (`py-shapes-decorator-wrapped-return`) and unit tests proving a
  workspace-local forwarding decorator does not emit `decorator_indirection`
  (#9619). Accepted: a bare `@name` over a module-level `def name(fn)` bound
  once in the owner's file whose wrapper takes only `*args`/`**kwargs`, may run
  side-effect calls that neither touch the arguments nor call or reset `fn`
  (plain `fn.__name__`-style reads are allowed), is optionally decorated by
  plain `functools.wraps(fn)`, and ends with `return fn(*args[, **kwargs])`.
  Rejected, keeping the limit: called or grouped decorator spellings,
  transforming, swallowing or retrying wrappers, `yield`/`await`, a wrapper
  name or parameter that shadows `fn`, rebound, imported or method-scoped
  decorators, and repo-mode classification (no owner source)
- fixtures proving the first repair classes carry activation-level missing
  discriminators for predicate boundaries, return values, exception paths,
  field/object values, and output/log/call effects
- fixtures proving strong-oracle, no-path, heuristic-only, and static-limit
  cases suppress repair guidance rather than becoming repair-ready work
- a check-output gap-ledger fixture proving Python repair cards can become
  bounded agent packets with verify and receipt commands while remaining
  preview/advisory
- outcome fixtures proving Python canonical gaps can close, remain unchanged,
  reopen, strengthen without closing, and weaken across check-output snapshots
  while preserving static/advisory receipt language
- a field/object fixture proving a returned constructor keyword such as
  `return User(active=True)` can route to a syntax-only object-field repair
  card with a discriminator such as `result.active == True`, a direct
  object-field assertion shape, and a stop condition when the returned object
  does not expose that keyword as a public field or attribute
- a non-boundary return-value outcome fixture proving a weak broad assertion can
  become an exact return assertion and close the canonical Python gap
- a non-boundary exception-path outcome fixture proving a weak broad exception
  observer can become exact message evidence and close the canonical Python gap
- a non-boundary field/object outcome fixture proving a broad object truthiness
  assertion can become an exact field assertion and close the canonical Python
  gap
- a non-boundary output/log outcome fixture proving a broad output observer can
  become exact output text evidence and close the canonical Python gap, while
  containment checks such as `"..." in caplog.text` remain conservative weak
  evidence
- fixtures proving direct weak related pytest and unittest tests are preferred
  as `strengthen_existing_test` repair targets instead of redundant new tests
- output tests proving eligible Python repair cards are projected into GitHub
  annotations and diff-scoped SARIF as advisory repair context, and proving
  no-action/static-limit Python findings project explicit no-repair-card and
  no-agent-packet annotation context, not gate or receipt authority
- a fixture proving `mock.assert_called*` is recognised as a
  side-effect oracle
- a fixture covering parametrized `pytest` cases
- a fixture covering pytest fixture parameters and a non-exact
  output/log oracle shape
- generated CI fixtures cover Python preview output visible only when
  `[languages]` declares `python`
- LSP protocol smoke covers a Python seam diagnostic, hover, code
  action, and evidence packet
- VS Code e2e smoke covers opening a Python file when the adapter is
  enabled
- `cargo xtask dogfood` records a checked Python preview receipt
- the capability matrix gains `Python preview static facts` at `alpha`,
  marked preview, with metrics from RIPR-SPEC-0026 plus Python-specific
  counts where the language adds vocabulary

## Non-Goals

- No type checking (`mypy`, `pyright`, `pytype`).
- No `pyproject.toml`, `setup.py`, or `requirements.txt` parsing.
- No virtualenv resolution or installed-package introspection.
- No runtime test runner integration beyond syntax pattern recognition.
- No `@dataclass` semantic expansion beyond detecting the syntactic
  shape.
- No mocking framework introspection beyond syntactic recognition.
- No automatic source edits, generated tests, or provider calls.
- No claims of parity with Rust evidence.

## Acceptance Examples

1. Function with boundary gap:

   ```python
   def apply_discount(amount: float, threshold: float) -> float:
       if amount >= threshold:
           return amount * 0.9
       return amount
   ```

   Existing tests:

   ```python
   def test_discount_applies_above_threshold():
       assert apply_discount(100, 50) == 90

   def test_no_discount_below_threshold():
       assert apply_discount(10, 50) == 10
   ```

   Expected static evidence:

   - owner: `apply_discount` (`function`)
   - predicate probe: `amount >= threshold`
   - oracle: `exact-value` via `assert ... == 90` and `assert ... == 10`
   - missing discriminator: `amount == threshold`
   - finding emits `language = "python"`,
     `language_status = "preview"`, `owner_kind = "function"`

2. Error path with `pytest.raises`:

   ```python
   def test_rejects_negative_amount():
       with pytest.raises(ValueError):
           apply_discount(-1, 50)
   ```

   Expected static evidence:

   - oracle: `error-path`
   - the `pytest.raises` context manager is recorded as the discriminator
     surface

3. Decorator indirection limit:

   ```python
   @retry(times=3)
   def fetch_total(client):
       return client.get_total()
   ```

   Expected static evidence:

   - probe emits `static_limit_kind = "decorator_indirection"`; finding
     stays conservative.

4. Functools memoization is not decorator indirection:

   ```python
   from functools import lru_cache

   @lru_cache
   def format_year(year):
       return "%03d" % (year % 100)

   @lru_cache(maxsize=None)
   def century_index(year):
       return year % 1000
   ```

   Expected static evidence:

   - neither owner emits `decorator_indirection`;
   - a direct exact-value test on `format_year` can credit the owner;
   - a related test that reaches `century_index` without observing the
     changed return remains a gap.

5. Typing markers are not decorator indirection:

   ```python
   from typing import final, override

   class Child(Base):
       @override
       def rate(self, n):
           return n + 1
   ```

   Expected static evidence:

   - `@final`, `@override` and `@no_type_check` written as plain names (and
     `typing.` / `typing_extensions.` qualified or aliased spellings; the
     `typing_extensions` spelling covers `final` and `override` only) return the
     function unchanged, so the owner emits no `decorator_indirection`;
   - a rebound receiver, a competing alias, a local same-named decorator, a
     marker written as a call (`@final(wrapper)`), a file where the marker name
     appears anywhere other than a `@name` decorator line or a `typing` /
     `typing_extensions` import (a `def`, `class`, assignment, tuple or loop
     target, nested or class-scope rebind, `try`/`except` import fallback), a
     star import from another module, or any decorator in the file that is not
     a plain dotted name (`@registry["x"]`, `@(lambda f: g)`) stays
     `decorator_indirection`. This is a lexical fail-closed screen, not a scope
     analysis, so an unrelated mention of the name (a docstring, a parameter, a `typing.` annotation for the qualified spelling) or a trailing comment on a decorator line also keeps the limit, and a rebind with no literal identifier (`globals().update(...)`, `exec`, a Unicode-normalised spelling) is not seen; a decorator line must be a dotted name or one call closing at the end of the line, so a subscripted, conditional, lambda, triple-quoted or multi-line call decorator withholds trust for the whole file (parentheses inside single-line string literals are skipped), as does a lone carriage-return line break (the lexical screen assumes the parser's pre-3.12 string lexing); `typing.overload`, `abc.abstractmethod`,
     `contextlib.contextmanager` and `property` also stay limited.

6. Simple route decorator repair:

   ```python
   @api.post("/checkout")
   def checkout(coupon_expired):
       response.status_code = 422
   ```

   Expected static evidence:

   - route decorator is preserved as owner context rather than treated as
     arbitrary decorator indirection;
   - probe can emit a field/object repair card with missing discriminator
     `response.status_code == 422`;
   - finding remains `preview` / advisory and does not execute route
     registration.

## Test Mapping

Default-prefix regression cases live in
`crates/ripr/src/analysis/language/python/owners_tests/tests.rs`. They exercise
module functions, pytest methods, unittest methods, and async definitions;
exclude near-miss names and helpers; and check that collected tests retain
framework-specific selectors and relate only to the referenced owner.

Functools memoization transparency (`lru_cache` / `cache` /
`cached_property`) is covered by
`crates/ripr/src/analysis/language/python/static_limits.rs` tests and
`fixtures/python_functools_memoization_not_indirection`. Typing-marker
transparency (`final` / `override` / `no_type_check`) is covered by the
`typing_marker_*` tests in the same file. Arbitrary
decorators such as `@retry` remain limited via
`fixtures/python_decorator_indirection_limit`.

Follow-up fixtures and tests cover the owner, test, assertion, related
test, probe, and static-limit cases listed under Required Evidence, plus
generated CI behavior and LSP smoke coverage. The CLI first-use path also
checks that `ripr pilot` can surface a top Python repair card from diff-scoped
preview evidence without requiring a Cargo workspace, that pilot's next
commands then follow that card's route instead of the diff-first
`ripr check` route (`ripr first-pr` before the edit to name the receipt
command, unless the card carries one; then the test edit, the card's verify
command, and the receipt command), and that `ripr first-pr`
can route an existing Python preview GapRecord into a preview-limited
start-here packet for a Python project root. The first-PR mapping also covers
the direct `--check-output <check.json>` bridge that materializes the
check-output-derived gap decision ledger before selecting the same preview
Python repair card. When that start-here packet ends with a
`ripr receipt write` receipt, which records only the verify status it is
given, and the ledger names its check-output input report, the packet also
carries `selected.static_recheck_command`: a `ripr check --worktree` run from
the same merge base compared with that input report by `ripr outcome`. The
comparison is static movement, not runtime or mutation evidence, and it is
omitted when the ledger's input report is unnamed or absent. The repo-ops PR summary also projects the top eligible
Python preview repair card from `actionable-gaps.json` so local reviewer
packets preserve the same canonical gap, missing discriminator, verify command,
receipt command, and advisory boundary. Editor projection accepts bounded
`python -m pytest ...` (and the bare `pytest ...` form earlier artifacts
carry) and `python -m unittest ...` verify commands from Python
GapRecords, can copy a bounded Python agent packet from current actionable
GapRecords, can copy a full repair card with a current validated GapRecord
freshness cue, can copy a fail-fast pytest skeleton, and can open the
suggested test file when the repair route carries a bare test name.

The single-literal expected-value rule (including one triple-quoted string,
and excluding compounds and adjacent-string concatenation) is covered by
`crates/ripr/src/analysis/language/python/tests.rs::classify_change_never_restates_changed_expression_as_discriminator`.

The pre-parse nesting budget (`parse_budget.rs`, #4109) counts brackets inside
an open string literal so a deeply bracketed literal still declines, but a
closed literal cannot leave a bracket open for the code after it. Unbalanced
brackets in separate literals such as `"\x1b[31m"` therefore do not accumulate
into file nesting (#8561): covered by
`crates/ripr/src/analysis/language/python/parse_budget.rs::tests::unbalanced_brackets_in_separate_strings_do_not_accumulate`
(including real nesting after the table still declining) and the CLI test
`crates/ripr/tests/python_parse_nesting.rs::unbalanced_bracket_string_literals_do_not_trip_the_nesting_budget`.

Package re-export reach is covered end to end, in diff and repo mode, by
`crates/ripr/src/analysis/language/python/reexport_tests.rs`: attribute calls
through an explicit `__init__.py` re-export, `from package import name`
module identity, a star re-export honoring `__all__`, and the negative
controls (a name `__all__` omits, a name quoted elsewhere but not in
`__all__`, a sibling name, a renamed re-export, a shadowed package alias, an
initializer that binds the name twice, an initializer that reassigns, deletes
or conditionally rebinds it, and an `__all__` replaced by an import), a
binding of another name that keeps the re-export, and a docstring that
mentions `__all__` without binding it.

The curated upstream Werkzeug #2065 case lives under
`fixtures/python-real-repo-evals/werkzeug-multiple-cookie/` and is referenced by
the existing Python eval corpus. Its native broken/fixed and assertion-removal
controls establish the `getlist("a") == ["b", "c"]` answer key independently
of analyzer output. Fixture-contract checks bind retained bytes and execution
records; the documented pytest replay executes the native controls separately.
The recorded historical `cached_property` limitation is not a permanent
expected capability boundary. Future promotion requires resolved descriptor
reach, effective-versus-weakened observer separation, and conservative
unknown/rebound-decorator controls before actionable guidance.

## Implementation Mapping

Follow-up implementation belongs to Campaign 27 work item
`analysis/python-preview-adapter`. The boundary, router, repo config,
and additive output metadata land first under RIPR-SPEC-0026 work items.
This spec PR records the per-language contract; no analyzer behavior
changes in the spec PR.

## Later Amendment

RIPR-SPEC-0238 (2026-10-04) owns the Python assertion table, per-test
oracle selection, the error-path gate, relation order and static-limit
precedence. It corrects three oracle lines in place above: `!=`,
`isinstance` and `assertNotEqual` were listed as broad, broad-type and
exact-value oracles, but the code records all three as
`relational_check` / weak, and no broad-type kind exists.

## Helper Inlining Amendment (2026-10-22, #9636)

A plain test that calls a same-file assertion helper with literal arguments
(`check_pass(50, True)` over `def check_pass(score, want): assert passes(score) == want`)
reads as the helper's `assert` statements with each parameter replaced by its
literal argument, on the call's own line (every line number is unchanged). The
rewrite is credit or unknown only: the substituted asserts pass through the
ordinary oracle admission, so a weak or unrecognised assert is no stronger than
when written inline; no new finding kind or repair rule is added and the
ordinary classifier applies. It applies only to
test files, and only when all of these hold; otherwise the call reads as before:

- the helper is a module-level, undecorated, synchronous `def`, spelled
  `def NAME` exactly once in the file (and never also a `class NAME`), with plain positional parameters (no
  default, `*args`, keyword-only or `**kwargs`), whose body is only `assert`
  statements (and an optional leading docstring), one per line; every mention
  of its name is its definition or a direct call;
- the file does not use `globals(`, `setattr(`, `exec(`, `eval(`, a star import
  or `__getattr__`;
- the call is a top-level expression statement, alone on its line, of a
  module-level `test*` function with no parameters and no decorators, whose
  body holds only expression and `assert` statements and no walrus, with
  exactly one argument per parameter, each a string, number, boolean or `None`
  literal (a signed number is parenthesised);
- a parameter is used in the asserts only as a plain value: not as an
  attribute name, keyword, inside a string, or as the object of a member access
  or call when the argument is a number, or directly followed by a quote
  (a string prefix); the asserts hold no `lambda`, comprehension, walrus,
  `await`, `yield`, backslash, or any `[` or `{` (every one is refused). A
  rewrite that does not parse is discarded and the original source is read.

A helper in another file (#9419), a call under `if`/loop/`with`, a computed
argument or a helper with control flow stays `no_static_path`.

This differs from `helper_assertions` (#9419, #9711): it is a same-file,
literal-argument, multi-assert source rewrite that runs first, so
`helper_assertions` then sees no call to resolve.

## Predicate Swap Amendment (2026-10-30, #9186)

A changed predicate that swaps one single-operand test of an owner parameter
for another (`x is None` to `not x`) is `weakly_exposed`, not `exposed`, when
every strong related call binds that parameter to a literal on which both
tests agree. The rule is a closed allow-list that keeps the existing verdict
on anything unrecognized: the parameter may be mentioned in the owner body
only in plain read statements (the changed line included), and a related test
body may make only recognized calls (the owner, the `assert` statement, named
`self.assert*` methods, known builtins and `pytest` helpers) with no
non-ASCII identifier letters, f-strings in the owner, subscript or call-result
callees, f-strings, or non-call mentions of the owner. A function owner is
recognized only under its imported name or a module alias the reader
resolves, never through a receiver alias (`m = src.lib; m.pick(0, 3)`,
`self.lib.pick(0, 3)`); an owner docstring holding `>>>` refuses. The test
file source is not read, so these shapes are known limits: an owner call at
module level (`R = pick(0, 3)`), in a class attribute, in `setUp`, in a
decorator (`@pytest.mark.parametrize('v', [pick(0, 3)])`), or in a conftest
or autouse fixture, a module-docstring, README or `.txt` doctest, a
`--doctest-modules` run over test or source files, a helper or runner table defined at module level
and seen only through its call in the test body, a discriminating call under
a non-strong oracle (`assert not pick(0, 3)`), and an owner doctest the
reader does not see. The change is only `exposed` to `weakly_exposed`, the
conservative direction.

## Metrics

In addition to the cross-language metrics in RIPR-SPEC-0026, the Python
adapter contributes:

- `language_adapter_python_findings_preview`
- `language_adapter_python_owner_function`
- `language_adapter_python_owner_method`
- `language_adapter_python_owner_class_method`
- `language_adapter_python_owner_module_function`
- `language_adapter_python_oracle_exact_value`
- `language_adapter_python_oracle_error_path`
- `language_adapter_python_oracle_side_effect`
- `language_adapter_python_oracle_smoke`
- `language_adapter_python_oracle_broad_type`
- `language_adapter_python_canonical_gap_identity`
- `language_adapter_python_ripr_evidence_model`
- `language_adapter_python_missing_discriminator_boundary`
- `language_adapter_python_first_pr_start_here_path`
- `language_adapter_python_repair_class_predicate_boundary`
- `language_adapter_python_repair_class_return_value`
- `language_adapter_python_repair_class_exception_path`
- `language_adapter_python_repair_class_field_value`
- `language_adapter_python_repair_class_output_or_call_effect`
- `language_adapter_python_repair_guidance_suppressed_non_actionable`
- `language_adapter_python_repair_ranking_noise_control`
- `language_adapter_python_test_placement_verify`
- `language_adapter_python_repair_card_v1`
- `language_adapter_python_existing_test_strengthening`
- `language_adapter_python_route_decorator_repair_card`
- `language_adapter_python_agent_packet_v1`
- `language_adapter_python_swarm_queue_stale_packets`
- `language_adapter_python_gap_receipt_from_check_output`
- `language_adapter_python_real_repo_eval_receipt`
- `language_adapter_python_repair_routing_quality_metrics`
- `language_adapter_python_pilot_first_use_path`
- `language_adapter_python_pr_summary_repair_card_projection`
- `language_adapter_python_lsp_agent_packet_action`
- `language_adapter_python_lsp_repair_card_action`
- `language_adapter_python_lsp_pytest_skeleton_action`
- `language_adapter_python_static_limit_stop_reasons`
- `language_adapter_python_static_limit_dynamic_dispatch`
- `language_adapter_python_static_limit_decorator_indirection`
- `language_adapter_python_static_limit_missing_import_graph`
- `language_adapter_python_static_limit_metaprogramming`
- `language_adapter_python_static_limit_mocked_module`
- `language_adapter_python_static_limit_opaque_custom_assertion_helper`
- `language_adapter_python_static_limit_property_based_test`
- `language_adapter_python_static_limit_unresolved_pytest_fixture`
- `language_adapter_python_static_limit_unsupported_syntax`
- `language_adapter_python_static_limit_transitive_reach`
