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
runner, or an import graph. Syntax facts are the contract; semantic
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
(`mocked_module` narrowing) and the cross-file limits in
[#9226](https://github.com/EffortlessMetrics/ripr-swarm/issues/9226). Decided by
the agent lane under the owner's delegation ("make reasonable decisions").

### Why amend

The Context above says the adapter must not depend on "an import graph", and
RIPR-SPEC-0028 maps that to `missing_import_graph` and `mocked_module`. Two
attempts to shrink `mocked_module` without any cross-file knowledge failed:
the name-disjointness check in #9053 was unsound, because a patched name can
reach the owner through an argument or injected collaborator, through stdlib
internals, or through a fixture or `conftest.py` patch that the test body never
mentions (#9098 comments). Without module-level reachability the limit can only
stay as broad as it is. The package re-export index (`reexports.rs`) already
reads `__init__.py` imports, so the line "no import graph" is already looser in
practice than the text. This amendment makes the boundary explicit and bounded
instead of leaving it implicit.

### Decision

The Python adapter may compute a **workspace import-edge index** and a **sealed
reach set** from it, under these rules. Nothing else about the adapter's
no-runtime, no-type-checker, no-subprocess stance changes.

**Index.** An edge is any `import` or `from ... import` statement the parser
sees anywhere in a file: module level, inside functions and classes,
conditional, `try`/`except ImportError`, and `if TYPE_CHECKING` (all
over-approximated as edges). Relative imports resolve against the file path.
Importing `a.b.c` also adds the parent package `__init__` chain, because Python
executes it. Each edge resolves to exactly one of:

- a workspace module (a single candidate file under the configured Python
  roots that is not in an excluded or generated directory);
- the standard library, by a module-name table compiled into the binary (no
  interpreter);
- external or unresolved (everything else, including ambiguous candidates,
  namespace packages with several roots, and any path that needs `sys.path`
  knowledge).

**Reach set.** For one test and one owner, `S` is the closure over workspace
edges from: the owner module, the test module, every `conftest.py` on the
test's directory chain, and every module defining a fixture the test requests
or that is autouse. `S` is **sealed** only when all of these hold; otherwise it
is **open** and every consumer falls back to today's behavior:

1. no external or unresolved edge in `S`;
2. every stdlib edge in `S` names a module on a reviewed *inert* allowlist
   (modules that cannot load a workspace module by name from data); anything
   not on the list is open, so new stdlib modules fail closed;
3. no reflective loading site in `S`: `importlib`, `__import__`, `pkgutil`,
   `runpy`, `pickle`/`marshal` loads, `eval`/`exec`, module `__getattr__`,
   `sys.modules` or `sys.path` writes, `entry_points`, and dotted-name
   factories such as `logging.config`;
4. no test-harness surface that can load code outside `S`: `pytest_plugins`,
   `-p` or `addopts` plugin flags, `pytest11` entry points, and `sitecustomize`
   or `.pth` files in the workspace, unless the plugin module is itself in `S`;
5. every module in `S` was parsed inside the existing parse and read budgets,
   and `S` stays under a fixed module cap. Exceeding any budget is open, never
   truncated-and-trusted.

**First consumer: `mocked_module`.** `mocked_module` may be dropped for a test
only when every mock site in the test's effective scope is a single-line string
literal target `pkg.mod.name` (via `patch`, `mock.patch`, `mocker.patch`,
`monkeypatch.setattr("pkg.mod.name", ...)`), the target's module resolves
uniquely to a workspace module `T`, `S` is sealed, and `T` is not in `S`. The
effective scope is the test body, its decorators and class decorators,
`pytestmark`, `setUp`/`setUpClass`, requested and autouse fixtures, the
`conftest.py` chain, and any patch-family call shape anywhere in `S`. Every
other shape stays limited: `patch.object`, `patch.multiple`, `patch.dict`,
`.start()` patchers, non-literal, concatenated, prefixed or multi-line
targets, stdlib and third-party targets, unresolved fixtures, and any site the
scanner cannot classify. Dropping the limit only restores the verdict the test
would have earned without the patch; it never raises oracle strength and never
grants `exposed` by itself.

### Safety argument

Claim: if `S` is sealed and `T` is not in `S`, nothing that runs during the
test can observe the patched attribute on `T`. Observing a module attribute
needs a reference to the module object or a binding in it. A reference arrives
only through (a) a static import edge, all of which the index records, parent
`__init__` side effects included, so `T` would be in `S`; (b) a dynamic load,
excluded by conditions 3 and 4; or (c) an object passed in from outside `S`,
which conditions 1 and 2 exclude because the test, its fixtures and the owner
are all inside `S` and nothing external or unmodeled is. The injected
collaborator, fixture-side patch, re-export and `__init__` re-binding cases
that broke #9053 each put `T` in `S` or make `S` open.

Residual by design, stated as non-claims: environment-level injection that is
not in the workspace (an installed `sitecustomize`, C extensions that import by
name, `PYTHONSTARTUP`), and extractor bugs. The second is the reason the
slices below are corpus-first with adversarial negatives and a fresh-reviewer
soundness pass on each one.

### Alternatives considered

- **Keep `mocked_module` broad (status quo).** Safe, and it stays the fallback
  for every open case. Rejected as the end state: it limited 6 of 68 caught
  mutants in the #9226 audit, and its stop reason gives an agent nothing to act
  on.
- **Name-disjointness between patch target and owner (#9053).** Rejected:
  unsound for injected collaborators, stdlib internals and fixture patches.
- **Execute or trace Python (`-X importtime`, `settrace`, coverage).**
  Rejected: breaks the no-runtime rule and makes facts depend on the
  interpreter on PATH.
- **Use `pyright`, `jedi`, `import-linter` or another resolver.** Rejected:
  a new runtime or dependency, and a type checker is excluded above.
- **Assume third-party packages never call back into the workspace.**
  Rejected: plugin systems (pytest plugins, Django settings strings, Celery
  and entry points) do exactly that. Third-party edges make `S` open.
- **Whole-program call graph.** Rejected: that is a second type checker. Only
  module-level reachability is needed to prove a module is absent from `S`.
- **Stdlib and third-party patch targets via a curated dependency table.**
  Deferred: patching `os.getcwd` is the motivating example, but proving the
  owner cannot reach it needs a pinned per-version stdlib call-dependency
  table. It can follow as its own slice and ADR note if the workspace-target
  slice earns trust.
- **A new ADR instead of an amendment.** Rejected: the rule being relaxed
  lives here, and ADR 0020 sets the precedent of dated in-place amendments.

### Slice plan

Each slice is its own reviewed PR, fixture-first
([ADR 0003](0003-fixtures-before-analyzer-rewrites.md),
[ADR 0010](0010-fixture-first-evidence-confidence.md)). A slice does not start
until the one before it has merged, and each gets a fresh-reviewer pass aimed
at building a counterexample to the seal.

1. **Spec and corpus stubs.** RIPR-SPEC-0028 text for the index, seal and
   `mocked_module` rule, traceability entries, and verdict-corpus cases with
   truth labels: a positive (unrelated workspace patch) and the negatives
   (injected collaborator, autouse `conftest` patch, re-export chain,
   function-local import, `importlib`, third-party edge, `pickle`, ambiguous
   module name, `patch.object`, `patch.dict`, non-literal and multi-line
   targets).
2. **Edge extraction and resolution.** Pure function from parsed files to
   edges with budgets and deterministic order; unit tests including relative
   imports, parent `__init__` chain and ambiguity. No consumer.
3. **Seal predicate.** `S` and sealed/open with a typed open reason, the
   reflective-loader table and the inert stdlib allowlist, and harness-surface
   scanning. No consumer.
4. **Mock-site enumerator.** Covers the full effective scope. Differential
   test: it finds every site the current body-text detector finds. No
   behavior change.
5. **Narrow `mocked_module`.** The only slice that moves verdicts. All
   negatives from slice 1 must stay `static_unknown` with `mocked_module`, and
   no corpus row may move from limited to `exposed` where runtime labels show
   the patch matters. Revert is one PR: the old detector remains the open-case
   path.
6. **Explain output.** Open reasons and the sealed witness (target, resolved
   module, `S` size) in the stop reason, so an agent sees why a test is still
   limited.

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
