# ADR 0025: Go Parser Substrate

Status: proposed

Date: 2026-11-04

Artifact ID: RIPR-ADR-0025

Linked issues:

- [#5534](https://github.com/EffortlessMetrics/ripr-swarm/issues/5534) (this decision)
- Parent campaign [#5520](https://github.com/EffortlessMetrics/ripr-swarm/issues/5520)
- Programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582)

## Context

Go is the next planned language adapter. #5520 requires the parser to be
chosen by ADR before any Cargo dependency lands (ADR 0006, 0008, 0009 and
`policy/dependency_allowlist.txt`). The adapter needs deterministic,
in-process syntax facts: functions, methods and receivers, generics, imports,
comments and directives (`//go:build`, generated-code headers), table tests,
and exact source spans for changed expressions. Parser types must stay inside
`analysis::language::go`.

Two crates.io candidates were measured, as the issue required:
`tree-sitter` 0.27.1 with `tree-sitter-go` 0.25.0, and `gosyn` 0.2.15. No
candidate may need a Go runtime or subprocess to parse.

## Spike

A disposable harness, kept outside the workspace, parsed the full Go standard
library tree that ships with the local Go toolchain (6,035 `.go` files, skipping
`testdata`) with both crates on Rust 1.99.1, release profile. The independent
oracle was Go's own `go/parser`.

| Check | tree-sitter-go 0.25.0 | gosyn 0.2.15 |
| --- | --- | --- |
| Files parsed without error (6,035) | 6,035 | 6,035 |
| Top-level function and method count equals `go/parser`, per file | 6,035 of 6,035 | 6,035 of 6,035 |
| Wall time for the corpus (parse only, one thread) | about 16.7 s | about 5.4 s |
| Generics (`Max[T Num]`, `Stack[T any]`) | parsed; `type_parameter_list` nodes | parsed |
| Exact span of a changed expression | start and end byte, row and column | start offset only on AST nodes, counted in Unicode scalars; `tokenize_source` returns token start and end in the same units, not bytes |
| Comments and directives (`//go:build`, `Code generated`) | first-class `comment` nodes with spans | collected on the file and as doc comments |
| Raw string containing `//` not mistaken for a comment | correct | correct |
| Malformed file (`return a +` then a valid function) | `has_error()` true, an `ERROR` node spans the bad token, the next function parses intact | whole-file `Err` with line, column and reason; no partial tree |
| Complete parse distinguishable from a partial one | yes: `has_error()` and `ERROR` or missing nodes | yes: `Result` |
| Clean release build of a one-dependency binary | about 14 s, 0.84 MB | about 8 s, 0.82 MB |
| New crates beyond the current lockfile | `tree-sitter`, `tree-sitter-go`, `tree-sitter-language`, `streaming-iterator`, `regex` family; build dependencies `cc` and `serde_json` are already locked | `gosyn`, `regex` family, `strum`, `unic-*`, `matches` |
| License | MIT | MIT |
| MSRV | tree-sitter 0.27 needs Rust 1.90; fits the 1.95 workspace | not declared; edition 2021 |
| Build requirement | C compiler through a `cc` build script | pure Rust |
| Maintenance signal | official grammar from the tree-sitter organization | single maintainer, 0.2.x |

Span example (tree-sitter, `a > b` inside `Max[T Num]` in the spike's
generic fixture): bytes 164..169, row 10, column 4, text `a > b`; the
`got != c.want` expression in the table-test fixture reports bytes 543..556.
The stdlib corpus exercises methods with receivers, composite literals,
switches and table-driven tests; the targeted fixture adds generics, a
`//go:build` header, a `Code generated` comment, a raw string containing `//`
and a malformed file.

Reproduction: the harness is disposable and is not kept in the repository.
To repeat it, create an empty crate with `tree-sitter` 0.27.1,
`tree-sitter-go` 0.25.0 and `gosyn` 0.2.15, walk `$(go env GOROOT)/src`
skipping `testdata`, compare per-file function and method declaration counts
with a small `go/parser` program, and time the parses. An independent
re-run on 2026-11-04 reproduced the counts, the error behavior and the timing
ratio (about 3.2x).

Both parsers are correct on the structural counts. The deciding differences
are spans, recovery, and maintenance, not coverage.

## Decision

Use **`tree-sitter` with `tree-sitter-go`** as the Go syntax substrate.

Reasons, in order of weight:

1. RIPR probes are exact-span objects. Invariant 5 of #5520 requires exact
   parser spans from the first Go probe. Tree-sitter provides start and end
   byte offsets plus row and column on every node. `gosyn` gives only a start
   offset, in Unicode scalar units, so end spans and byte spans would be
   reconstructed by a second tokenizer pass, which is a new source of drift.
2. Recovery fits fail-closed analysis. After a syntax error, tree-sitter
   still yields the intact functions around it, with the damage isolated in
   an `ERROR` node. The adapter can analyze the undamaged owners and mark
   only the damaged region unknown. `gosyn` rejects the whole file, which
   would turn one bad line into a file-wide limitation.
3. The grammar is the official one and follows the language. `gosyn` is a
   single-maintainer 0.2.x crate; a Go language change would depend on one
   person.

Parser-specific types, including `tree_sitter::Node`, stay inside
`analysis::language::go`. The adapter projects into RIPR-owned facts
immediately. The `lang-go` feature owns the dependency; default features stay
unchanged until a separate support-tier decision.

### Required adapter behavior

- A parse with `has_error()` is never a complete parse. Owners overlapping an
  `ERROR` or missing node get a typed static limitation; owners outside it
  may be analyzed. A `parse_budget`-style bound on tree size applies, as for
  the Python adapter.
- `unsafe_code = "forbid"` applies to the `ripr` crate. The `tree-sitter`
  crate contains `unsafe` internally. No tree-sitter pointer or raw API may
  cross the adapter boundary, and `cargo xtask check-public-api` must keep
  parser types out of the public surface.
- Build tags, filename suffixes and generated headers are read from comment
  nodes and file names. Mutually exclusive files are never merged into one
  program (#5537).

## Alternatives Considered

- **`gosyn` 0.2.15.** Pure Rust and about 3x faster, and equally correct on
  structural counts. Rejected because AST nodes carry only a start offset in
  Unicode scalars (end offsets exist only on the separate token stream), and a
  single syntax error rejects the whole file with no partial tree. Revisit if
  it gains end spans and recovery.
- **Go toolchain facts (`go/parser`, `go list`) as the syntax source.**
  Rejected for baseline analysis: #5520 requires a usable path without a Go
  toolchain. `go list` stays an optional, bounded enrichment (#5554).
- **A RIPR-owned Go parser.** Rejected: the cost and the risk of
  repair-shaped false positives from an incomplete grammar outweigh any gain.

## Consequences

Positive:

- Exact byte and line/column spans for probes from day one.
- Local error isolation matches how RIPR already treats unknowns.
- Grammar follows upstream Go.

Negative and accepted:

- The first non-Rust-native parser in the analyzer. Builds need a C compiler
  (`cc` is already in `Cargo.lock`).
- About 3x slower than `gosyn` on the stdlib corpus; still about 2.8 ms per
  file. The adapter should keep the per-file fact cache pattern used by the
  Python and TypeScript adapters.
- Binary and build-time cost is small (about 6 s of clean build time and
  about 25 KB in the spike binary), but must be re-measured inside the real
  `lang-go` feature build.

## Follow-up Specs / Plans

- #5533: Go preview static-facts and fail-closed evidence spec.
- #5535: `lang-go` scaffold, which adds the Cargo dependency.
- #5537: module, package and build-constraint identity.
- #5538: owner and exact-span probe extraction on this substrate.

## Revisit criteria

Reopen this decision if any of the following occurs:

- the `lang-go` packaged-feature matrix fails on Windows or macOS because of
  the C build step (#5520 requires Linux, macOS and Windows proof);
- `cargo deny`, the process policy, or the release pipeline rejects the
  build script;
- `gosyn` gains end spans and partial-tree recovery and the C dependency
  proves costly;
- a measured parse-time regression appears on a large Go repository pilot.

Policy checks run on this change: `check-doc-index`, `check-doc-artifacts`,
`check-dependencies`, `check-file-policy` (all pass). `cargo deny` is not run
here because no Cargo dependency is added; the scaffold PR (#5535) must run it.

## Not in this ADR

No `GoAdapter`, Cargo feature, routing, findings or support claim. The Cargo
dependency is added in the scaffold PR (#5535). This PR records the approved
crate decision in `policy/dependency_allowlist.txt` only.
