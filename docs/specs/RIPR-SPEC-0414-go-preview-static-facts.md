# RIPR-SPEC-0414: Go Preview Static Facts

Status: proposed

Created: 2026-11-04

Linked issues:

- [#5533](https://github.com/EffortlessMetrics/ripr-swarm/issues/5533)
- Parent [#5520](https://github.com/EffortlessMetrics/ripr-swarm/issues/5520)

Linked authorities:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) language adapter contract
- [ADR 0025](../adr/0025-go-parser-substrate.md) Go parser substrate
- [ADR 0019](../adr/0019-language-adapters-reuse-shared-packet-contract.md) shared packet contract
- [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526) multi-language architecture, including
  [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) capability manifest,
  [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542) activation law and
  [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546) completeness

## Problem

Go is the next planned language adapter. Its facts differ from the existing
adapters in ways that can silently produce false gaps or false exposure:

- test oracles are control flow (`if got != want { t.Fatalf(...) }`), not
  assertion macros;
- which files form a package depends on build tags, filename suffixes and cgo;
- package and module identity come from `go.mod` and `go.work`, not paths;
- Go caches successful test results, so "this test will run" needs an explicit
  statement about `-count=1`;
- interfaces, method values, reflection and cgo hide call edges.

This spec defines the normative preview contract before any parser dependency
or adapter code lands. It extends RIPR-SPEC-0026 and adds no new output schema.

## Behavior

Initial Go support is preview and advisory only. The spec grants no gate,
badge, baseline, RIPR Zero, default-feature, repair-loop or publication
authority.

Baseline analysis needs no Go toolchain. Optional toolchain enrichment
(`go list -json -test -e`) is a separate bounded producer (#5554) and may add
package facts only; it never upgrades a limitation to a pass by itself.

### Soundness invariant

Unresolved Go semantics are never a clean unreached gap. Build-context
ambiguity, interface dispatch, method values, reflection, cgo, assembly,
`go:linkname`, concurrency, dynamic subtests and opaque helpers produce typed
limitations and `infection_unknown`, `propagation_unknown` or `static_unknown`
classifications. They do not produce `no_static_path` merely because an edge
could not be resolved.

### Identity and routing

| Concern | Contract |
| --- | --- |
| Wire language | `go` (no `golang` alias) |
| Feature gate | `lang-go` |
| Status | `language_status = "preview"` |
| Routing | `.go` routes to `GoAdapter` only when Go is enabled in `[languages] enabled` and `lang-go` is compiled |
| Disabled or not compiled | `.go` stays an unanalyzed source language and the run records a typed limitation, never a clean zero |
| Test source | `*_test.go` is test source, never production source |
| Project markers | `go.mod`, `go.work` |

```toml
[languages]
enabled = ["rust", "go"]
```

Explicit configuration is authoritative. Auto-detection from `go.mod` is out of
scope until preview calibration (#5555) and must never override an explicit
`enabled` list.

Package identity derives from module and package facts, never from absolute
paths:

```text
go:<module_path>/<package_dir>::<package_name>
```

`package_name` is part of every owner identity (below), so `TestX` in
`package n` and `TestX` in `package n_test` never collide.

- Same-package tests (`package foo`) and external tests (`package foo_test`)
  are distinct identities. An external test reaches only exported API.
- Nested `go.mod` files define separate modules; a file belongs to the nearest
  enclosing module root.
- `go.work` binds modules into a workspace; membership comes from its `use`
  directives and is recorded, not inferred from directory layout.
- A nested `go.mod` makes its directory a separate module: files below it are
  not part of the parent module's packages, and a route for the parent module
  never names them.
- A change to `go.mod` or `go.work` is project-graph movement. It is reported
  as a changed-input fact and can never disappear as "non-source".

### Build and source law

Default is fail closed: the preview does not inherit the host `GOOS`/`GOARCH`
as semantic truth.

| Input | Contract |
| --- | --- |
| `//go:build` and legacy `// +build` | Parsed from comment nodes. Constraint expressions are recorded verbatim with their span |
| `_GOOS`, `_GOARCH`, `_GOOS_GOARCH` filename suffixes | Recorded as implicit constraints. A trailing `_test` is stripped first: `b_linux_test.go` is a linux-constrained test file, while `c_test_linux.go` is linux-constrained production source (its stem does not end in `_test`). `d_unix.go` and a bare `linux.go` carry no suffix constraint (`unix` is a tag only; a bare name has no underscore) |
| `//go:build` placement | Counts only before the `package` clause and followed by a blank line. A later `//go:build` comment is an ordinary comment |
| `cgo` tag and `CGO_ENABLED` | Never read from the host environment. `cgo` is just another tag a configured context sets or leaves unset; with no context it is unresolved |
| No configured build context | Constrained files are analyzed individually and tagged `go_build_context_unresolved`; mutually exclusive files are never merged into one program |
| Configured build contexts | A context is an explicit list of `GOOS`, `GOARCH` and tags in repo configuration. Evidence is reported per context |
| `import "C"` (cgo) | `go_cgo_boundary` for owners touching C-linked code |
| `.s` files and `//go:linkname` | `go_assembly_or_linkname` for the affected owner |
| Generated source | Detected only from a line matching `^// Code generated .* DO NOT EDIT\.$` placed before the first non-comment text; excluded from mutation seams and counted as skipped |
| `vendor/`, `testdata/`, and files or directories starting with `_` or `.` | Not production or test evidence; counted as skipped with role |
| Build contexts | Configured explicitly, never inherited from the host: `[languages.go] contexts = [{ goos = "linux", goarch = "amd64", tags = ["integration"] }]`. Absent means no context |

Every changed `.go` file that is skipped, generated, vendored, or excluded by the
configured context is a typed fact, never silence. Such a file makes the run
partial (`not_established` for that file), is counted with its role or the
reason `go_excluded_by_context`, and feeds the shared completeness accounting
(#5546). A diff in which every changed file was skipped or excluded is never
reported as clean.

### Owners and probes

Portable owner identity is repository-relative and language-qualified:

```text
go:<module_path>/<package_dir>::<package_name>::<owner_path>[@<file>]
```

Owner paths: `Func`, `Type.Method` for both value and pointer receivers
(receiver kind recorded as a field, not part of the identity), and generic
forms `Func[#n]` and `Type[#n].Method` where `n` is the type-parameter arity
(the parameter name is arbitrary and never part of identity). Two methods with
the same name on different receiver types never collapse.

The `@<file>` discriminator is appended only when the same owner path is
defined in more than one file of the package (build variants such as
`calc_linux.go` and `calc_windows.go`, or the legal repeated `init`). `init`
additionally carries its in-file ordinal (`init#2`). Anonymous functions and
closures are not owners; their probes attach to the enclosing owner.

Shared `owner_kind` values used by Go: `function` and `method` (generic forms
use the same values). Whether an owner is a test, example, fuzz target,
benchmark or `TestMain` is recorded in the spec-level `test_kind` field, not in
the shared `owner_kind` vocabulary.

Every owner and probe carries an exact byte span plus line and column from the
first implementation (ADR 0025). Probe families:

- `predicate` (`if`, `for` condition, `switch` case guards, boolean expressions);
- `return_value` (`return` expressions, named results);
- `error_path` (error construction, `return err`, wrapping);
- `field_construction` (struct and composite literals, field assignment);
- `call_side_effect` (statically resolved calls whose effect is observable);
- `panic_behavior` (`panic`, `recover`).

When a construct cannot be classified, emit a limitation, not an invented probe.

### Tests and verification

Test admission:

| Surface | Contract |
| --- | --- |
| `func TestXxx(t *testing.T)` in `*_test.go` | Ordinary test. `Xxx` must not start with a lowercase letter. The `testing` import is resolved through its local alias, for example `import tt "testing"` with a `*tt.T` parameter |
| `t.Run("name", func(t *testing.T) {...})` | Subtest identity `Parent/name`. Literal names only; spaces become `_` as `go test -run` sees them. A literal name that is empty, contains `/`, or repeats a sibling name (Go renames these `#00`, `#01`) is `go_dynamic_table_or_subtest` and gets no selector |
| Table-driven loops | Rows are extracted only from a literal slice or map of struct literals. Each row is a distinct discriminator candidate. Computed or appended tables are `go_dynamic_table_or_subtest` |
| `func ExampleXxx()` with `// Output:` or `// Unordered output:` | Executable example; the output comment is an exact-text oracle (unordered output compares lines as a multiset). Without either comment it is compiled but not run and earns no oracle credit |
| `func FuzzXxx(f *testing.F)` | Fuzz target. Seed corpus entries from `f.Add` are rows. The fuzz engine itself is not modeled |
| `func BenchmarkXxx(b *testing.B)` | Separate evidence kind, never an ordinary test |
| `func TestMain(m *testing.M)` | Package-level setup that may skip `m.Run`; recorded, grants no reach and no oracle credit, and every test in the package carries `go_testmain_gate` |
| `func Testfoo(t *testing.T)` (lowercase after `Test`) | Not a test; `go vet` rejects it, so it surfaces as a build problem, not a skip |

Focused verification routes name the established package and test:

```text
go test -count=1 -v -run '^TestName$' ./<package_dir>
```

- The command runs from the module root of the owning module (or the `go.work`
  root for a workspace member). A route for a nested module names that module's
  root, never the parent's.
- `-v` is required: `-run` matching zero tests exits 0 with
  `no tests to run`, so "the test executed" is established only from a
  `=== RUN` line for it, never from exit status alone.
- A test in a file constrained by a tag or `_GOOS`/`_GOARCH` suffix is not run
  by a plain `go test`. The route must carry the configured context
  (`-tags`, `GOOS`, `GOARCH`) or be refused.

- `-count=1` is mandatory whenever RIPR says a test will execute rather than be
  served from Go's cache. A command without it must not claim execution.
- `-run` selectors are anchored and regex-escaped from literal names only.
  Subtests use `-run '^TestName$/^sub_name$'`.
- No route is emitted when the test name or package is unresolved.

### Oracle model

The contract does not assume assertion macros. The primary standard-library
oracle is a comparison or error predicate whose failing branch reaches a
`testing` failure call.

```go
got := F(x)
if got != want {
    t.Fatalf("F(%v) = %v, want %v", x, got, want)
}
```

Bounded credit:

| Shape | Oracle kind |
| --- | --- |
| `if got != want` or `==` on a value derived from the changed call, failing branch calls `t.Error`, `t.Errorf`, `t.Fail`, `t.FailNow`, `t.Fatal`, `t.Fatalf` on the governing `t` | `exact_value` when `want` is a literal or fixed local, otherwise `relational_check` |
| `<`, `<=`, `>`, `>=` against any value | `relational_check` (a boundary check, never `exact_value`) |
| `if err != nil` then fail | `broad_error`. It credits only a probe that changes whether an error is returned; it never credits a changed error value or variant |
| `errors.Is(err, X)` / `errors.As(err, &X)` with a resolvable sentinel or type `X`, failing branch fails | `exact_error_variant` |
| `reflect.DeepEqual(got, want)` (import resolved to `reflect`), `cmp.Equal` (import resolved to `github.com/google/go-cmp/cmp`) | `exact_value` when `want` is fixed; otherwise `relational_check`. Name-only matches earn nothing |
| `defer func(){ r := recover(); ... }()` comparing the recovered value | `exact_value` over the panic value |
| Example `// Output:` or `// Unordered output:` | `exact_value` over printed text |
| Failure call outside any branch condition, or not on the governing `t` | No credit |
| Any other shape (inverted `if got == want { return }` then fail, `if !ok`, `cmp.Diff(...) != ""`) | No credit; the finding is `static_unknown` with `unsupported_syntax`, never an actionable gap |

The governing `t` is the lexically nearest enclosing test or subtest parameter
of the failure call. The outer `t` called from inside a `t.Run` closure does not
govern a branch inside that closure.

Credit requires data flow from the changed call to the compared value. A
failing call elsewhere in the test, a test name, or file proximity cannot
credit an unrelated changed observable. Common assertion libraries (`testify`,
`go-cmp`) are recognized by resolved import path only (#5549); a name match
alone earns nothing.

### Related-test reach

Relation strength, strongest first:

1. Direct call to a resolved package function, including through an import alias.
2. Method call on a receiver whose concrete type resolves from a composite
   literal or constructor call in the same test.
3. Helper chain of at most two hops with literal arguments.

A method call on a receiver whose type is not statically resolved is matched by
name at most as `dynamic_dispatch`, never as reach. Embedded-struct promotion,
type aliases and dot-imports are `missing_import_graph` until the package graph
resolves them. Anything beyond the helper bound is `go_transitive_helper_unresolved`.

### Limitation vocabulary

Reuse shared `StaticLimitKind` values when truthful: `dynamic_dispatch`,
`missing_import_graph`, `unsupported_syntax`, `mocked_module`,
`opaque_custom_assertion_helper` (project helper wrapping `t.Fatal`),
`metaprogramming` (a `reflect` call on the production path), and
`property_based_test` (fuzz targets). Go-specific tokens, added only where the
shared vocabulary cannot express the stop:

| Token | Stop |
| --- | --- |
| `go_build_context_unresolved` | Constraints present and no configured build context |
| `go_module_graph_unresolved` | Module or package graph incomplete (missing `go.mod`, unresolved `replace`, unreadable `go.work`) |
| `go_interface_dispatch` | Call through an interface value |
| `go_function_value` | Method value or function variable call |
| `go_cgo_boundary` | cgo reached |
| `go_assembly_or_linkname` | Assembly or `go:linkname` reached |
| `go_concurrency_observer` | Result observed through goroutines or channels the adapter does not follow |
| `go_dynamic_table_or_subtest` | Table or subtest names/rows are computed |
| `go_testmain_gate` | Package has `TestMain`, which may skip `m.Run` |
| `go_excluded_by_context` | Changed file excluded by the configured build context |
| `go_transitive_helper_unresolved` | Helper chain beyond the bounded depth |

Shared output metadata in RIPR-SPEC-0026 lists `go` as an allowed `language`
value but does not yet list these tokens. A later additive schema change
(#5535) extends the shared vocabulary with them before the adapter emits them;
until then this spec is the normative contract for the tokens only.

### Capability and authority boundary

Register cells under #5531: `go/any/diff/static_facts`, `go/any/repo/static_facts`
and `go/any/diff/repair_routing`. All remain `not_proven` and tier `preview`
until fixture proof lands. No cell implies that `lang-go` is compiled.

Go output uses the one shared schema family: `language = "go"`,
`language_status = "preview"`, the existing finding, SARIF, LSP, MCP and agent
surfaces. Any actionable packet passes the shared
`validate_agent_gap_record_packet` (ADR 0019); there is no Go-specific validator,
classifier or renderer.

## Required Evidence

This slice is documentation only. Satisfaction requires:

- this document with a positive and a negative example for each contract area;
- the RIPR-SPEC-0026 vocabulary pointer to this spec;
- spec index, traceability, capability and doc-artifact rows;
- `cargo xtask check-spec-format`, `check-doc-index`, `check-static-language`
  and `check-traceability` clean on the landing PR.

Later implementation must add fixture-backed proof per #5536 and a judged
corpus per #5555 before any cell leaves `not_proven`.

## Non-Goals

- No parser dependency, adapter, Go subprocess, test execution or generated tests.
- No support-tier promotion, release, gate, badge, baseline or RIPR Zero authority.
- No Go-specific output schema, classifier or packet validator.
- No automatic `go generate`, dependency download or `gopls` use.

## Acceptance Examples

Each example states static evidence vocabulary only; none implies a runtime
mutation outcome.

### 1. Control-flow oracle with a literal expectation (positive)

```go
func Clamp(x, max int) int {
	if x > max {
		return max
	}
	return x
}
```

```go
func TestClamp(t *testing.T) {
	if got := Clamp(10, 5); got != 5 {
		t.Fatalf("Clamp(10, 5) = %d, want 5", got)
	}
}
```

Expected: owner `Clamp`, `predicate` probe on `x > max`, `exact_value` oracle
(the compared value flows from `Clamp`; `want` is a literal). The boundary
`x == max` is not discriminated and stays a gap.

### 2. Failure call not tied to the changed value (negative)

```go
func TestClampOther(t *testing.T) {
	_ = Clamp(10, 5)
	if len(os.Args) > 99 {
		t.Fatal("unreachable")
	}
}
```

Expected: no oracle credit. The failure branch does not depend on the changed
call's result.

### 3. Aliased `testing` import (positive)

`import tt "testing"` with `func TestX(t *tt.T)` is admitted as an ordinary
test. A parameter typed `*other.T` is not.

### 4. Same-package versus external test package (positive and negative)

`package calc` test may call unexported `clamp`; `package calc_test` calling
unexported `clamp` is a compile-level impossibility and yields no relation. The
two packages keep distinct identities.

### 5. Table test rows (positive and negative)

A literal `[]struct{in, want int}{{1,1},{5,5}}` loop yields one candidate per
row. `tests := buildTable()` yields `go_dynamic_table_or_subtest`, not a gap.

### 6. Build constraints (positive and negative)

`calc_linux.go` and `calc_windows.go` define the same function. With no
configured context both are analyzed separately and tagged
`go_build_context_unresolved`; they are never merged, and their owner
identities differ by `@<file>`. With `contexts = [{ goos = "linux" }]` only
`calc_linux.go` participates and the report names the context. If the diff
changes only `calc_windows.go`, it is reported as `go_excluded_by_context` and
the run is partial, never clean.

### 7. Interface dispatch (negative)

A test calls `var s Shape = Circle{}; s.Area()` and the changed owner is
`Circle.Area`. Expected: `go_interface_dispatch`, `propagation_unknown`; never
`no_static_path`.

### 8. Example with output (positive and negative)

`func ExampleClamp()` with `// Output: 5` (or `// Unordered output:`) is an
exact-text oracle. The same example without either comment earns no credit.

### 9. Focused command (positive and negative)

Expected route, run from the module root:
`go test -count=1 -v -run '^TestClamp$' ./pkg/calc`. A route without `-count=1`
or `-v`, built from a computed test name, or naming a test in a tag-constrained
file without its context, is refused. A run that prints `no tests to run` did
not execute the test.

### 10. Generated and vendored files (negative)

A change under `vendor/` or in a file with a `Code generated ... DO NOT EDIT.`
header produces no seams and increments the skipped count with its role.

### 11. Module files as movement (positive)

A diff touching only `go.mod` reports a changed project-graph input; it is not
reported as "no analyzable changes".

### 12. Exact spans (positive)

Every probe cites start and end byte and line/column from the parse. A probe
without an exact span is not emitted.

### 13a. Shadowed or outer `t` (negative)

```go
func TestClampSub(t *testing.T) {
	got := Clamp(10, 5)
	t.Run("case", func(t *testing.T) {
		if got != 5 {
			t.Fatal("bad") // governing t is the inner t; got was computed outside the subtest
		}
	})
}
```

and the same shape calling the outer `t.Fatal` from inside the closure: the
outer `t` does not govern the inner branch and earns no credit.

### 13b. Ordering operator and unrecognized shapes (negative)

`if got < 5 { t.Fatal(...) }` is `relational_check`, never `exact_value`. The
inverted `if got == 5 { return }; t.Fatal(...)` earns no credit and yields
`static_unknown`, not an actionable gap.

### 13c. Filename suffixes (positive and negative)

`b_linux_test.go` is a linux-constrained test file; `c_test_linux.go` is
linux-constrained production source; `d_unix.go` and `linux.go` carry no
suffix constraint.

### 13d. Directive placement (negative)

A `//go:build linux` comment after the `package` clause is an ordinary comment
and constrains nothing.

### 13e. Subtest names (negative)

`t.Run("a/b", ...)`, `t.Run("", ...)` and a repeated `t.Run("x", ...)` yield
`go_dynamic_table_or_subtest` and no selector.

### 13f. Same name, different package (positive)

`TestX` in `package n` and `TestX` in `package n_test` have distinct identities
(`::n::TestX` and `::n_test::TestX`). Two `init` functions in one package are
`init#1` and `init#2`.

### 13g. Nested module and workspace (positive and negative)

A file under a nested `go.mod` belongs to the nested module; a parent-module
route never names it. A `go.work` `use` entry places a module in the workspace;
a directory merely adjacent to it is not a member.

### 13h. Fuzz, benchmark, TestMain, cgo (negative)

`FuzzXxx` seeds are rows but carry `property_based_test`; `BenchmarkXxx` earns
no ordinary-test credit; a package with `TestMain` carries `go_testmain_gate`;
an owner reaching `import "C"` carries `go_cgo_boundary`.

### 13i. `errors.Is` and `recover` (positive)

`if !errors.Is(err, ErrNotFound) { t.Fatal(...) }` with a resolvable sentinel is
`exact_error_variant`. A deferred `recover()` whose value is compared with
`!=` is `exact_value` over the panic value.

### 13. Disabled adapter (negative)

With Go not enabled, a changed `.go` file yields the unanalyzed-language
limitation and a partial result. It never yields a clean empty result.

## Test Mapping

Planned proof (not in this docs slice):

- `crates/ripr/src/analysis/language/go/tests.rs` pins each acceptance example;
- `fixtures/go-preview-static-facts/` golden check output (#5536);
- `cargo xtask check-fixture-contracts` and `cargo xtask goldens check` once
  adapter code exists.

## Implementation Mapping

Planned (not in this docs slice):

- `crates/ripr/src/analysis/language/go/` for fact extraction (#5535, #5537-#5540);
- `crates/ripr/src/config/` for `[languages]` and build contexts;
- shared renderers consume language metadata only; no Go fork.

## Metrics

- `language_adapter_go_spec_registered`: spec, traceability, capability and doc
  index rows present;
- future: `language_adapter_go_findings_preview` and per-limitation counters
  once fixtures exist.
