# ADR 0009: Python Parser Substrate

Status: accepted (superseding its own original decision recorded in commit
`d70f1802`). Amended 2026-10-22: permits one bounded, syntax-only workspace
import-edge index (see the Amendment section); the parser pick is unchanged.

Date: 2026-05-12

## Correction Note

The first version of this ADR picked `ruff_python_parser`. That pick
turned out to rest on a failed assumption about availability:
`ruff_python_parser` is a workspace crate inside the
[astral-sh/ruff](https://github.com/astral-sh/ruff) monorepo and is
marked `publish = false`, so it is not available on crates.io. The
only ways to depend on it are a git dependency (rejected by our
production dependency policy) or a third-party vendored fork
(supply-chain and maintenance risk we will not take). The original
ADR even named `rustpython-parser` as the documented natural fallback
under Revisit Criteria; we are exercising that fallback here.

The architectural envelope is unchanged: a small, Rust-native,
syntax-first parser quarantined behind `PythonAdapter`, with no
Python runtime, type checker, build graph, or git dependency. Only
the substrate name changes.

## Context

Campaign 27 (Language Adapter Preview) introduces a `LanguageAdapter`
seam inside the existing `crates/ripr` package and ships preview
adapters for TypeScript and Python alongside the Rust reference
adapter. The Python preview contract is pinned by
[RIPR-SPEC-0028](../specs/RIPR-SPEC-0028-python-preview-static-facts.md):
syntax-first owner, test, assertion, related-test, and probe extraction
from `*.py` files, with explicit static-limit reporting for things the
syntax-first adapter cannot classify.

The adapter must not depend on `mypy`, `pyright`, a runtime test
runner, or an import graph (the bounded workspace import-edge index in the
Amendment below is the one exception). Syntax facts are the contract; semantic
enrichment is explicitly out of scope. Within that constraint, the
adapter still needs a real parser — regex-driven token recognition
cannot robustly handle decorators, `async def`, `match`/`case`, f-strings,
type-parameter syntax (PEP 695), or the broader Python grammar surface.

The parser decision should be explicit before the dependency-allowlist
update and adapter implementation slices land so future PRs do not mix
parser choice with adapter shape, output metadata, or generated CI
behavior — the same separation
[ADR 0008](0008-typescript-parser-substrate.md) used for the TypeScript
adapter.

The choice also locks the workspace dependency surface (cargo-deny and
`policy/dependency_allowlist.txt` apply). Rust-native parser options
that exist today and are usable as a normal Cargo dependency include
`rustpython-parser` and `tree-sitter-python`. Astral's
`ruff_python_parser` would otherwise be a natural fit on technical
grounds but is intentionally unpublished from the `astral-sh/ruff`
workspace (`publish = false`); see the Alternatives Considered
section for details on why we treat that as a hard exclusion rather
than a soft trade-off.

This ADR records the parser pick. It does not add the dependency, write
adapter code, or change any fact-extraction behavior — those land in
the next slices of Campaign 27.

## Decision

Use **`rustpython-parser`** (published on crates.io) as the Python
syntax substrate for the Python preview adapter, following the same
pattern as [ADR 0006](0006-rust-syntax-substrate.md) (Rust syntax
substrate) and [ADR 0008](0008-typescript-parser-substrate.md)
(TypeScript syntax substrate): a small, Rust-native, syntax-first
parser quarantined behind the adapter implementation.

Parser-specific types stay inside the adapter implementation. The rest
of the product keeps consuming the language-neutral fact shells from
RIPR-SPEC-0026/0028 and the existing domain DTOs:

- `LanguageFacts`, `OwnerFact`, `OwnerKind`, `TestFact`, `AssertionFact`,
  `RelatedTest`, `StaticLimitKind` (per RIPR-SPEC-0026)
- existing `Probe`, `Finding`, `OracleKind`, `OracleStrength`,
  `FlowSinkFact` from `crate::domain`

The adapter must:

- handle `*.py` and (when the adapter later opts in) Jupyter Python
  cells via `*.ipynb` extraction routed through this parser;
- emit explicit `StaticLimitKind` values when syntax-first analysis
  cannot classify (no silent coercion to `no_static_path`);
- not invoke a Python type checker, import resolver, build graph, or
  runtime tooling by default (the Amendment below is the one bounded exception:
  a syntax-only workspace import-edge index computed from files the adapter
  already parses, never from running or importing Python);
- isolate `rustpython_parser` / `rustpython-ast` types behind the
  `PythonAdapter` impl so the trait surface stays language-neutral.

`rustpython-parser` joins `ra_ap_syntax` and `oxc_parser` as an
approved crate decision in `policy/dependency_allowlist.txt`; the
actual Cargo dependency is added in the next scoped PR.

## Consequences

Positive:

- Python syntax parsing stays in-process and Rust-native, matching the
  `RustAdapter` (`ra_ap_syntax`) and `TypeScriptAdapter` (`oxc_parser`)
  patterns.
- `rustpython-parser` is published on crates.io, so the workspace can
  declare a normal Cargo dependency without git URLs or vendored
  forks.
- AST shape follows the CPython AST closely, which keeps fact
  extraction recognisable to anyone who has used Python's own `ast`
  module as a reference.
- Decorator, `async def`, `class`, comprehension, `match`/`case`, and
  f-string syntax are supported by the released parser.
- Parser-specific API churn is quarantined behind `PythonAdapter`
  (matches the RustAdapter / TypeScriptAdapter pattern).
- Future Python typing-aware analysis (if ever needed) can sit *on top*
  of the same AST without re-litigating the parser choice.

Negative:

- `ripr` adds a production parser dependency, expanding the cargo-deny
  audit surface.
- `rustpython-parser` is the parser layer of the RustPython compiler
  project, so it carries a small amount of "compiler-shaped" weight
  relative to a parser whose only product is static analysis. We
  accept that in exchange for crates.io availability and a stable
  published API surface.
- Adapter code must translate parser nodes into repository-owned DTOs
  instead of leaking `rustpython_*` types across module seams.
- Syntax-backed extraction still cannot answer semantic questions
  requiring Python type inference (e.g., resolving a duck-typed
  attribute call). Those cases must emit
  `StaticLimitKind::missing_import_graph` /
  `StaticLimitKind::dynamic_dispatch` rather than guessing.

## Alternatives Considered

- **`ruff_python_parser`** (the original pick this ADR superseded).
  Designed for static analyzers, Rust-native, actively maintained
  inside [astral-sh/ruff](https://github.com/astral-sh/ruff). Rejected
  because the crate is marked `publish = false` in the Ruff workspace
  and is not available on crates.io. Depending on it would require
  either a git dependency (rejected by our production dependency
  policy and `policy/dependency_allowlist.txt`) or a third-party
  vendored fork (`littrs-ruff-python-parser`,
  `rustpython-ruff_python_parser`), which carries supply-chain and
  maintenance risk we will not take. If Astral later publishes the
  parser to crates.io under stable versioning, this ADR should be
  revisited.
- **`tree-sitter-python`**. Portable, language-neutral parser model
  used by Neovim, Helix, and GitHub's code-search. Rejected for the
  same reason ADR 0008 rejected `tree-sitter-typescript`: its CST /
  byte-range model differs from the AST-shaped extraction the spec
  implies, and porting the Rust and TypeScript adapter shapes (owner /
  test / assertion / probe family classification) to tree-sitter
  queries would diverge from the established adapter pattern without
  a benefit that matches the divergence.
- **Roll a syntax-aware regex extractor**. Rejected because
  RIPR-SPEC-0028 requires extracting owners, tests, assertions, probe
  families, and static limits across decorators, `async def`,
  comprehensions, `match`/`case`, parametrized tests, and f-strings.
  A regex/heuristic extractor cannot meet that bar without becoming a
  fragile parser of its own.
- **Invoke Python at build/test time (subprocess into `python -m ast`)**.
  Rejected because RIPR-SPEC-0028 requires no Python runtime
  dependency: `ripr` is a single Rust binary that must work on hosts
  without a Python install. A subprocess parser also breaks
  reproducibility — the adapter's facts would depend on whichever
  Python interpreter happens to be on PATH.
- **`littrs-ruff-python-parser` / `rustpython-ruff_python_parser`**
  (third-party re-publications of Ruff's parser). Rejected because
  neither is maintained by Astral. Picking either ties the adapter to
  a single contributor's pace and willingness to track upstream Ruff
  changes, which is exactly the supply-chain risk the Ruff workspace
  itself would solve if published canonically. If a canonical
  publication appears, this ADR should be revisited.
- **Defer parser choice until Python adapter implementation**. Rejected
  because the dependency-allowlist gate requires a documented rationale
  before adding the crate, and review burden is lower when the choice
  is recorded separately from the adapter code — matching how Campaign
  27 split [ADR 0008](0008-typescript-parser-substrate.md) and the
  TypeScript adapter scaffold (PR #759).

## Revisit Criteria

This ADR should be revisited if any of these change:

- `rustpython-parser` stops releasing, breaks its published AST
  contract without a clear migration path, or stops keeping pace with
  CPython grammar releases.
- Astral publishes `ruff_python_parser` to crates.io under stable
  versioning, eliminating the availability gap that drove this
  correction. At that point the lighter static-tooling-focused parser
  becomes available as a normal Cargo dependency and the original ADR
  rationale (Ruff parser → static-tool fit) regains its weight.
- A new Rust-native Python parser (a focused syntax-only crate spun
  out from Ruff, a new `tree-sitter-python` API generation, or another
  option) makes a measurable correctness or compile-footprint
  difference.
- The Python adapter discovers a class of seams that requires Python
  type information (not just syntax), at which point the static-limit
  reporting boundary itself needs revision via a follow-up ADR or
  proposal.
- A future Campaign 27 work item adds a typed Python pass (e.g., for
  duck-typed call resolution), which would suggest a parser whose AST
  carries the additional facts natively.

Operational watchpoint: [EffortlessMetrics/ripr#1430](https://github.com/EffortlessMetrics/ripr/issues/1430)
tracks the first two criteria (Ruff parser publication and `rustpython-parser`
decay) and the fact-contract lock that keeps a backend swap reviewable.

## Amendment (2026-10-22): bounded workspace import-edge index

Governs: [#9098](https://github.com/EffortlessMetrics/ripr-swarm/issues/9098)
(`mocked_module` narrowing) and, later and separately, the cross-file limits in
[#9226](https://github.com/EffortlessMetrics/ripr-swarm/issues/9226).
Authority: the owner delegated this decision on 2026-10-22 ("I mean, I don't
read the adrs. Make reasonable decisions"), so the agent lane decided it. The
owner has not reviewed the text; the slice plan below keeps every step
reversible.

### Why amend

The Context above says the adapter must not depend on "an import graph", and
RIPR-SPEC-0028 maps that to `missing_import_graph` and `mocked_module`. A
shrink of `mocked_module` that uses no cross-file knowledge failed: the
name-disjointness check in #9053 was unsound, because a patched name can reach
the owner through an argument or injected collaborator, through stdlib
internals, or through a fixture or `conftest.py` patch the test body never
mentions (#9098 comments). The package re-export index (`reexports.rs`)
already reads `__init__.py` imports, so the text is already looser than the
practice. This amendment makes the boundary explicit and bounded.

An independent review of the first draft of this amendment found that a sealed
import closure alone is not sound: a pytest run is one process, and code
outside the closure can hand the patched module into it through process-global
state, and `patch()` itself imports the target. The rules below close those
holes by making the target **isolated**, and they add a measured go/no-go gate
because the sound rule is narrow.

### Decision

The Python adapter may compute a **workspace import-edge index** and, from it,
a **sealed reach set** and an **isolated target** check, under these rules.
Nothing else about the adapter's no-runtime, no-type-checker, no-subprocess
stance changes. Every result is a static reachability statement under the
residual list below, not a runtime claim.

**Index.** An edge is any `import` or `from ... import` statement anywhere in a
file: module level, inside functions and classes, conditional, `try`/`except
ImportError`, and `if TYPE_CHECKING` (all over-approximated as edges).
Relative imports resolve against the file path. Further edges:

- importing `a.b.c` adds the parent `__init__` chain, which Python executes;
- `from pkg import name` and `from . import name` add the submodule `pkg.name`
  when one exists, and `from pkg import *` adds every submodule `__all__`
  names;
- an attribute access `pkg.X` where `pkg` is a workspace package and `X` is
  one of its submodules adds that submodule, because a `patch()` elsewhere can
  make the attribute appear.

Each edge resolves to exactly one of: a workspace module (one candidate file
under the roots the adapter already infers, not in an excluded or generated
directory); a standard-library module from a table compiled into the binary; or
external/unresolved. A workspace file that shares a name with a stdlib module
wins over the stdlib (pytest's default import mode puts the test directories
first), and any such collision is open. Compiled and stub modules (`.so`,
`.pyd`, `.pyc`, `.pyi`) and any resolution that needs `sys.path`,
`pythonpath`, `--import-mode`, editable-install or namespace-package knowledge
are unresolved, which is open. Slice 2 defines the roots concretely.

**Reach set.** For one test and one owner, `S` is the closure over workspace
edges from: the owner module, the test module, every `conftest.py` on the
test's directory chain, and every module defining a fixture the test requests
directly, transitively (fixtures requesting fixtures), through `usefixtures`,
`getfixturevalue` with a literal name or `parametrize(indirect=...)`, or that is
autouse. `S` is **sealed** only when all of these hold; otherwise it is
**open** and every consumer keeps today's behavior:

1. no external or unresolved edge in `S`, with one named exception: the
   harness set `pytest` and `unittest.mock`, which are not an edge but are
   covered by condition 4;
2. every stdlib edge in `S` names a module on the **inert allowlist**: modules
   that cannot load a workspace module by name from data *and* keep no
   process-global registry or callback list another module can write into.
   Therefore `logging`, `atexit`, `warnings`, `codecs`, `signal`, `threading`,
   `sys`, `builtins`, `gc`, `copyreg`, `abc` registration, `os.environ` and
   similar are not inert. The list is a reviewed, versioned artifact with a
   negative test per entry; an unlisted module is open;
3. no reflective loading or loader-surface site in `S`: `importlib`,
   `__import__`, `pkgutil`, `runpy`, `pickle`/`marshal` loads, `eval`/`exec`,
   module `__getattr__`, `sys.modules` reads or writes, `sys.path` writes,
   `entry_points`, `inspect.getmodule`, `pydoc.locate`, dotted-name factories
   such as `logging.config`, and `monkeypatch.syspath_prepend`;
4. no test-harness surface that can load code outside `S`: `pytest_plugins`;
   `-p`, `addopts`, `-c`, `--confcutdir`, `--rootdir`, `pythonpath`,
   `--doctest-modules` and `filterwarnings = error` in `pytest.ini`,
   `pyproject.toml`, `setup.cfg` or `tox.ini`; `PYTEST_PLUGINS` and
   `PYTEST_ADDOPTS`; ini options that name a module (`DJANGO_SETTINGS_MODULE`,
   `--cov=`); `pytest11` entry points; `conftest.py` files outside the
   directory chain; and `sitecustomize`, `usercustomize` or `.pth` files in the
   workspace. These are **unconditionally open** (a plugin or startup file runs
   whether or not `S` contains it). `pytest-mock`'s `mocker` fixture comes from
   a plugin the test never imports, so it is accepted only as the named
   harness exception when no other plugin surface is present;
5. every module in `S` was parsed inside the existing parse and read budgets
   and `S` stays under a fixed module cap; exceeding a budget is open.

**Isolated target.** A patch target is **isolated** only when all of these
hold:

6. its module `T` resolves uniquely to a workspace module by the longest module
   prefix, and the remainder is not a submodule, and no `__init__.py` on the
   target's chain binds the target's name by alias (`from . import impl as
   mod`), because then the patch lands on a different object than the file;
7. `T` is not in `S` and the closure of `T` (with its parent `__init__` chain)
   is disjoint from `S`;
8. no workspace module imports `T` or any module in its closure except from
   inside that closure itself: a test file outside `S` that imports `T` in the
   same pytest process can register `T`'s objects in a process-global registry
   (a logging handler, a codec, an `atexit` hook) that code in `S` then uses;
9. the closure of `T` and its parent `__init__` chain pass conditions 1 to 3
   (no reflective site, no `sys.modules` write, no process-global registry
   write), because `patch()` imports `T` during the test and runs its top
   level, and import side effects (`decimal.getcontext().prec = ...`,
   `os.environ[...] = ...`, `logging.disable`, `os.chdir`) can change what `S`
   observes.

**First consumer: `mocked_module`.** The limit may be dropped for a test only
when `S` is sealed and *every* mock site in the test's effective scope is a
single-line string literal target `pkg.mod.name` in a form the scanner
recognizes through the file's own imports (`patch`, `mock.patch`,
`unittest.mock.patch`, `mocker.patch`, `monkeypatch.setattr("pkg.mod.name",
...)`, including aliased names such as `from unittest.mock import patch as p`)
whose target is isolated. The effective scope is the test body, its decorators
and class decorators, `pytestmark`, `setUp`/`setUpClass`, every fixture in the
transitive set above, the `conftest.py` chain, and any patch-family call shape
anywhere in `S`. Every other shape stays limited: `patch.object`,
`patch.multiple`, `patch.dict`, `.start()` patchers, `setitem`, `delattr`,
`delitem`, `setenv`, `chdir`, non-literal, concatenated, prefixed or multi-line
targets, stdlib and third-party targets, unresolved fixtures, an aliased name
the scanner cannot tie to an import, and any site it cannot classify. Dropping
the limit only restores the verdict the test would have earned without the
patch, and only because conditions 6 to 9 make the patch's own import
inconsequential; it never raises oracle strength and never grants `exposed` by
itself.

### Safety argument

Claim (static): if `S` is sealed and the target is isolated, no static path
exists from the test, its fixtures or the owner to the patched attribute, so
the patch cannot change what they observe except through the residual list.

A module attribute is observed through a reference to the module object or a
binding in it. A reference arrives through (a) a static import edge, all of
which the index records, including `__init__` side effects, submodule binding
and attribute access, so `T` would be in `S` (condition 7); (b) a dynamic load
or loader surface, excluded by conditions 3 and 4 inside `S` and, for `T`
itself, by condition 9; (c) an object handed in from outside `S` through
process-global state, which conditions 2 and 8 exclude by keeping registries
out of `S` and by forbidding any other importer of `T`; or (d) `T` mutating
`S` at import time, excluded by condition 9 together with the disjoint
closure in condition 7. The cases that broke #9053 (injected collaborator,
fixture-side patch, re-export, `__init__` re-binding) each put `T` in `S` or
make `S` open.

Residual by design, stated as non-claims: environment-level injection that is
not in the workspace (an installed `sitecustomize`, installed third-party
pytest plugins, C extensions that import by name, `PYTHONSTARTUP`);
`multiprocessing` start methods; forward-reference evaluation through
`typing.get_type_hints` or `singledispatch`; and extractor bugs. Extractor bugs
are why every slice is corpus-first with runtime-labeled adversarial negatives
and a fresh-reviewer pass aimed at a counterexample.

### Honest payoff

Condition 8 is strict: most realistically patched modules are imported by
production code, so they fail it. The sound rule may therefore qualify only a
small share of `mocked_module` rows (6 of the 68 caught mutants in the #9226
audit were limited by `mocked_module` at all). The plan below measures this
before any analyzer code is written and stops if it is not worth the surface.

### Alternatives considered

- **Keep `mocked_module` broad (status quo).** Safe, and it stays the fallback
  for every open case. It remains the end state if the payoff gate fails.
- **Name-disjointness between patch target and owner (#9053).** Rejected:
  unsound for injected collaborators, stdlib internals and fixture patches.
- **Sealed closure of the owner and test only (this amendment's first draft).**
  Rejected after review: process-global registries and `patch()`'s own import
  of the target defeat it, hence conditions 2, 8 and 9.
- **Execute or trace Python (`-X importtime`, `settrace`, coverage).**
  Rejected: breaks the no-runtime rule and makes facts depend on the
  interpreter on PATH.
- **Use `pyright`, `jedi`, `import-linter` or another resolver.** Rejected:
  a new runtime or dependency, and a type checker is excluded above.
- **Assume third-party packages never call back into the workspace.**
  Rejected: plugin systems (pytest plugins, Django settings strings, Celery,
  entry points) do exactly that.
- **Whole-program call graph.** Rejected: that is a second type checker. Only
  module-level reachability is needed.
- **Stdlib and third-party patch targets via a curated dependency table.**
  Deferred: patching `os.getcwd` is the motivating example, but proving the
  owner cannot reach it needs a pinned per-version stdlib call-dependency
  table. It can follow as its own slice and ADR note.
- **A new ADR instead of an amendment.** Rejected: the rule being relaxed
  lives here, and ADR 0020 sets the precedent of dated in-place amendments.

### Slice plan

Each slice is its own reviewed PR, fixture-first
([ADR 0003](0003-fixtures-before-analyzer-rewrites.md),
[ADR 0010](0010-fixture-first-evidence-confidence.md)). A slice does not start
until the one before it has merged, and each gets a fresh-reviewer pass aimed
at building a counterexample.

1. **Spec, docs and corpus stubs.** RIPR-SPEC-0028 text for the index, seal,
   isolation and `mocked_module` rule, plus the docs and fixtures that say the
   adapter has no import graph (`docs/STATIC_LIMITS.md`,
   `docs/CAPABILITY_MATRIX.md`, `docs/LANGUAGE_ADAPTER_PREVIEW.md`,
   RIPR-SPEC-0238, RIPR-PROP-0001, and the `fixtures/python_*/SPEC.md` files),
   traceability entries, and runtime-labeled verdict-corpus cases: a positive
   (isolated workspace patch) and negatives for each hole found so far
   (injected collaborator, autouse `conftest` patch, re-export chain,
   function-local import, `importlib`, third-party edge, `pickle`, ambiguous
   module name, `patch.object`, `patch.dict`, non-literal and multi-line
   targets, a cross-module `logging`/`codecs`/`atexit` registry, a target whose
   top level sets `decimal`/`os.environ` state, a `sys.modules` alias in the
   target, `from pkg import sub` with an empty `__init__`, `pkg.sub` attribute
   access without an import, an `__init__` alias, a stdlib-shadowing workspace
   file, and an aliased `patch` name next to a literal one).
2. **Payoff gate.** A read-only measurement (script output attached to the
   PR, no analyzer change) of how many `mocked_module` rows in the verdict
   corpus and the four sdists from #9226 satisfy conditions 1 to 9 on paper.
   If fewer than a handful qualify, record the number here, keep the status
   quo, and stop; slices 3 to 7 are not built.
3. **Edge extraction and resolution.** Pure function from parsed files to
   edges with budgets and deterministic order, including the submodule,
   attribute and `__init__` alias rules and the stdlib-shadowing rule. No
   consumer.
4. **Seal and isolation predicates.** `S`, sealed/open and isolated/not with
   typed open reasons, the loader-surface table, the versioned inert
   allowlist, and harness-surface scanning. No consumer.
5. **Mock-site enumerator.** Covers the full effective scope and resolves
   aliased patch names through imports. Differential test against the current
   body-text detector in both directions: it finds every site the old one
   finds, and its extra sites (fixture, `conftest`, alias) are listed. No
   behavior change.
6. **Narrow `mocked_module`.** The only slice that moves verdicts. It is one
   branch in front of the existing `test_has_mocked_module` call, so revert is
   one PR. It can also newly add `mocked_module` where a fixture-side patch is
   found that today's body-only scan misses; that is the conservative
   direction, and the corpus carries rows for it. All negatives from slice 1
   must stay `static_unknown` with `mocked_module`, and no corpus row may move
   from limited to `exposed` where runtime labels show the patch matters.
7. **Explain output.** Open reasons and the sealed/isolated witness (target,
   resolved module, `S` size) in the stop reason, so an agent sees why a test
   is still limited. It reverts together with slice 6.

Further consumers (the public-API reach in #9226 part 2, `missing_import_graph`
for workspace-resolved callees) are not authorized by this amendment. Each
needs its own spec slice that cites it and carries its own safety argument.

### Claim boundary

After these slices ripr may claim syntax-level module reachability inside the
workspace under the budgets above. It may not claim a call graph, type
information, third-party behavior, or any runtime outcome; static findings keep
the conservative vocabulary of [ADR 0002](0002-static-exposure-language.md).

## Related Specs and Campaigns

- [RIPR-SPEC-0026: Language adapter contract](../specs/RIPR-SPEC-0026-language-adapter-contract.md)
- [RIPR-SPEC-0028: Python preview static facts](../specs/RIPR-SPEC-0028-python-preview-static-facts.md)
  (slice 1 of the Amendment's plan updates it)
- [RIPR-PROP-0001: Multi-Language Adapter Preview](../proposals/RIPR-PROP-0001-multi-language-adapter-preview.md)
- [ADR 0008: TypeScript Parser Substrate](0008-typescript-parser-substrate.md)
- Campaign 27: Language Adapter Preview (active in
  `.ripr/goals/active.toml` (deleted in #1701 PR 3) and the
  [implementation campaigns ledger](../IMPLEMENTATION_CAMPAIGNS.md)).

The Python adapter scaffold work item is tracked separately. This ADR
only records the parser pick.
