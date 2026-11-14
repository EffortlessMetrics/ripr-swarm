# Language Adapter Preview Workflow

Use this guide to get TypeScript, JavaScript, or Python evidence from `ripr`
without changing the Rust default path.

Preview findings are advisory. They do not match Rust maturity, do not run a
typechecker or test runner, and do not make generated CI blocking.

## Current Shape

| Language | Status | Default | Evidence scope |
| --- | --- | --- | --- |
| Rust | reference path | enabled | Rust static exposure evidence on every CLI, CI, editor, report, and gate surface. |
| TypeScript and JavaScript | preview | enabled when TypeScript or JavaScript project markers are detected and no `ripr.toml` exists ([detection rules](CONFIGURATION.md#languages)); otherwise only when configured | Syntax-first owners, tests, assertions, probes, related tests, and visible static limits for `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, and `.cjs`. |
| Python | preview static facts, plus a scoped repair-routing lane (tier in the registry projection below) | enabled when Python project markers are detected and no `ripr.toml` exists ([detection rules](CONFIGURATION.md#languages)); otherwise only when configured | Syntax-first owners, tests, assertions, probes, related tests, RIPR-stage evidence, selected repair-class missing discriminators, fail-closed static limits, and bounded repair cards/packets for selected pytest/unittest routes. |

## Capability Registry Projection

The block below is generated from the capability registry (`ripr capabilities
--json`) and is the only source of per-cell support states and tier ceilings.
An absent cell is `not_proven`. Change states in the registry, never here;
`cargo xtask capability-docs --check` rejects hand edits.

<!-- capability-registry:begin (generated from domain::language_capability::CAPABILITY_CELLS; do not edit) -->

| Cell (language/framework/mode/family) | Support state | Tier ceiling |
| --- | --- | --- |
| `rust/any/diff/static_facts` | `supported_with_required_fallback` | `usable_alpha` |
| `rust/any/repo/static_facts` | `supported_with_required_fallback` | `usable_alpha` |
| `rust/any/diff/repair_routing` | `supported_with_required_fallback` | `usable_alpha` |
| `python/any/diff/static_facts` | `supported_with_required_fallback` | `preview` |
| `python/any/repo/static_facts` | `supported_with_required_fallback` | `preview` |
| `python/pytest/diff/repair_routing` | `supported_with_required_fallback` | `usable_alpha` |
| `python/unittest/diff/repair_routing` | `supported_with_required_fallback` | `usable_alpha` |
| `typescript/any/diff/static_facts` | `supported_with_required_fallback` | `preview` |
| `typescript/any/repo/static_facts` | `supported_with_required_fallback` | `preview` |
| `typescript/any/diff/repair_routing` | `supported_with_required_fallback` | `preview` |
| `javascript/any/diff/static_facts` | `supported_with_required_fallback` | `preview` |
| `javascript/any/repo/static_facts` | `supported_with_required_fallback` | `preview` |
| `javascript/any/diff/repair_routing` | `supported_with_required_fallback` | `preview` |
| `perl/any/diff/static_facts` | `not_proven` | `preview` |
| `perl/any/repo/static_facts` | `not_proven` | `preview` |
| `perl/any/diff/repair_routing` | `not_proven` | `preview` |
| `go/any/diff/static_facts` | `not_proven` | `preview` |
| `go/any/repo/static_facts` | `not_proven` | `preview` |
| `go/any/diff/repair_routing` | `not_proven` | `preview` |

<!-- capability-registry:end -->

Preview findings use the same output schema and review surfaces as Rust, with
additive `language`, `language_status`, `owner_kind`, and `static_limit_kind`
fields when the adapter can fill them.

## Enable Preview Adapters

With no `ripr.toml`, `ripr` turns on Python and TypeScript preview when it
detects their project markers ([detection rules](CONFIGURATION.md#languages)).
For explicit control, list languages in the repo-root `ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript", "python"]
```

`javascript` is accepted as an alias for `typescript`; list only one of them.
To opt out, or to roll back, set `enabled = ["rust"]`. An explicit `ripr.toml`
always overrides detection.

Build features are separate from this runtime switch. The default published
build includes both preview adapters; `cargo build -p ripr --no-default-features
--features lang-rust` builds a Rust-only binary. On that binary:

- with no `ripr.toml`, detected Python markers are a configuration error, while
  detected TypeScript markers are ignored silently;
- a `ripr.toml` that lists `python` or `typescript` fails closed with an error
  naming the missing Cargo feature, and the editor shows the adapter as
  unavailable instead of publishing preview diagnostics.

Check the result with `ripr doctor --root .`. It prints `Detected languages`;
add `--verbose` to see `Enabled languages`. On a root without `Cargo.toml` or
`.rs` files, the Rust toolchain checks are skipped rather than failed.

Bun stable-byte review has an opt-in `[profiles.bun_ub]` profile; see the
[Bun UB TypeScript preview runbook](BUN_UB_TYPESCRIPT_PREVIEW_RUNBOOK.md).

## What Diff Analysis Refuses

Vendored, built, and generated files never count as changed source:
`node_modules`, `dist`, `build`, `coverage`, `vendor`, `__generated__`,
`*.generated.*`, minified bundles (`*.min.js`), TypeScript-family files under
`docs/`, and generated Python such as `*_pb2.py`. Admit hand-written source
under an excluded directory with `handwritten_files`
([configuration](CONFIGURATION.md#languages)).

## Run The Local Preview Loop

```bash
ripr pilot --root .
ripr check --root . --format human
mkdir -p target/ripr/reports
ripr check --root . --json > target/ripr/reports/check.json
```

Read preview findings as syntax-first advisory evidence:

- `language` names the adapter (`typescript`, `javascript`, `python`). Rust
  findings may omit it; a missing value means Rust.
- `language_status = "preview"` marks advisory evidence.
- `owner_kind` gives the syntactic owner shape when known.
- `static_limit_kind` names a known static limitation instead of hiding it.
- Python `missing_discriminators` are syntax-derived. Only direct weak findings
  become repair work; heuristic, no-path, and static-limit findings do not.

Python repair routing for selected pytest/unittest workflows has its own row in
[Support tiers](status/SUPPORT_TIERS.md). It covers direct weak findings with a
canonical gap, missing discriminator, test target, verify command, stop
conditions, bounded agent packet, and before/after receipt. It does not promote
other Python evidence, generated CI, gates, badges, baselines, or RIPR Zero.

Exposure classes keep RIPR's conservative vocabulary: `exposed`,
`weakly_exposed`, `reachable_unrevealed`, `no_static_path`,
`infection_unknown`, `propagation_unknown`, and `static_unknown`.

## Interpret Static Limits

A static limit marks where syntax-first analysis sees a shape but cannot infer
runtime behavior. It is context on a finding, not a verdict, and does not erase
a useful related test or strong oracle. See [Static limits](STATIC_LIMITS.md).

| Static limit | How to read it |
| --- | --- |
| `dynamic_dispatch` | The call target is chosen at runtime, such as a computed member call (`obj[name]` then invoked) or `getattr(obj, name)(...)`. |
| `metaprogramming` | Decorators, proxies, or metaclasses may change behavior. |
| `missing_import_graph` | The adapter did not resolve a full project import graph. |
| `decorator_indirection` | A Python decorator may change the callable boundary. Simple route decorators such as `@api.post(...)` and `functools` caching decorators are not this limit. |
| `mocked_module` | A test replaces or mocks the module or symbol under review. |
| `opaque_custom_assertion_helper` | A Python test uses a custom assertion helper whose body is not inspected. |
| `property_based_test` | A Python test uses generated inputs, such as Hypothesis `@given(...)`. |
| `unresolved_pytest_fixture` | A pytest test depends on fixture values that are not known statically. |
| `unsupported_syntax` | The parser found syntax outside the current preview contract. |

## Read Preview Confidence

TypeScript, JavaScript, and Python preview `confidence` is a per-class constant
(`0.6` exposed, `0.4` weakly exposed or no static path, `0.2` static unknown):
an ordinal, not a probability, and not on Rust's scale. `confidence_basis` in
JSON lists the capping reasons and human output prints `Why capped:`; neither
changes the class, actionability, or repair-packet readiness.

## Read Mixed-Language Reports

Preview findings share JSON arrays with Rust findings; filter by `language` and
`language_status` to separate them. Human output prints the preview label and
static limit before the evidence. Field contracts are in
[Output schema](OUTPUT_SCHEMA.md), maturity in
[Support tiers](status/SUPPORT_TIERS.md), and limits in
[Static limits](STATIC_LIMITS.md).

## Repo-Mode Analysis: Rust Full, Python and TypeScript Bounded Preview

A zero-seam repo report for a TypeScript or Python workspace is not clean
coverage. The seam formats below render only Rust seams; preview findings come
from diff-scoped `ripr check` or the shared repo analysis result.

Repo mode for Python and TypeScript/JavaScript analyzes a capped working set of
production files (default 800, `RIPR_MAX_REPO_INDEX_FILES`). A capped or
partial run discloses a partial `language_runs` entry and can never back an
all-clear, badge, or baseline claim. Repo-mode findings carry
`analysis_scope: "repo"` and get no repair routing:
python/any/repo/repair_routing is not_proven, and
typescript/any/repo/repair_routing is not_proven, in the registry. Diff mode
caps the Python walk too (`RIPR_PYTHON_MAX_WORKSPACE_FILES`, default 4000), and
refused files appear as named limitations, never silent skips.

| Format | Rust repo | TypeScript repo | Python repo |
| --- | --- | --- | --- |
| `repo-exposure-json` / `repo-exposure-md` / `repo-sarif` | full | seams: none rendered; `typescript_diff_first` limitation | seams: none rendered; `python_diff_first` limitation |
| `repo-seams-json` / `repo-seams-md` | full | seams: none rendered; evidence via repo analysis result | seams: none rendered; evidence via repo analysis result |
| `repo-badge-json` / `repo-badge-shields` | full | seams: none rendered; evidence via repo analysis result | seams: none rendered; capped/partial runs never badge-eligible |
| `agent-seam-packets-json` | full | seams: none rendered; evidence via repo analysis result | seams: none rendered; evidence via repo analysis result |

`review-comments` projects only seam-backed findings, so a Python or TypeScript
diff yields `comments: 0` even when `ripr check` finds evidence. Render preview
findings as review comments through a gap ledger:

```bash
mkdir -p target/ripr/reports target/ripr/review
ripr check --base <pr-base> --json > target/ripr/reports/check-output.json
ripr reports gap-ledger --check-output target/ripr/reports/check-output.json --root . --out target/ripr/reports/gap-decision-ledger.json
ripr review-comments --base <pr-base> --head <pr-head> --root . --gap-ledger target/ripr/reports/gap-decision-ledger.json --out target/ripr/review/comments.json
```

## Editor Workflow

The VS Code extension activates on TypeScript, TSX, JavaScript, JSX, and Python
files, but analysis follows the repository's language selection: detection when
no `ripr.toml` exists, otherwise `[languages]`. A language the config leaves
out produces no preview diagnostics.

Preview hovers show the syntax-first advisory boundary and any static limit
before the RIPR evidence; actions use structured diagnostic data. If
diagnostics look stale, run `ripr: Show Status`, `ripr: Show Output`,
`ripr: Restart Server`, or `ripr: Refresh Full Analysis`.

## Generated CI Workflow

`ripr init --ci github` generates an advisory workflow. When preview languages
are enabled, its job summary adds a `Language preview grouping` section with
one group per enabled preview language and `gate_impact = none`. Enabling
`typescript` adds both a `typescript` and a `javascript` group; Rust-only
repositories do not see the section. The summary is not
a gate: pass/fail authority stays with explicitly configured gate decisions.

## Roll Back Preview Evidence

Set `enabled = ["rust"]` (see [Enable Preview Adapters](#enable-preview-adapters)),
then rerun `ripr doctor --root .` and `ripr pilot --root .`. In CI, commit the
change and rerun the workflow; in the editor, save a file or run
`ripr: Refresh Full Analysis`. Older reports are stale evidence from the prior
configuration; you do not need to delete them.

## What Not To Infer

Preview language evidence does not mean:

- runtime mutation execution happened;
- TypeScript or Python reached Rust maturity;
- a typechecker, import graph, virtual environment, package install, or test
  runner was invoked;
- generated CI became blocking;
- preview findings are eligible for RIPR Zero, baseline debt, or gate policy by
  default;
- RIPR edited source files or generated tests.

The useful claim is narrower: for enabled preview languages, RIPR surfaces
syntax-first static evidence, related tests, missing discriminators, static
limits, and repair-oriented next actions in the same review surfaces as Rust.
