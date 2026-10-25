# RIPR-SPEC-0356: A Rust file that no Cargo target compiles contributes no related test

Status: proposed

Owner: product / analysis

Created: 2026-10-25

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8999

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory; this only stops text
  that Cargo never builds from being credited as a test.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No output schema change.

## Problem

A benchmark or corpus haystack such as `benchmarks/haystacks/code/rust-library.rs`
is ordinary Rust text. It sits outside `src/`, `tests/`, `benches/` and
`examples/`, so no Cargo target compiles it and a `#[test]` in it never runs.
ripr indexed it like any other file: its tests were listed as related tests of
changed owners, and when the parser refused its text the run became
`partial_with_limitations` with a lexical-fallback note about a file the diff
never touched.

## Behavior

A file is positively never compiled (and is marked `outside_module_tree`, the
provenance #7426 introduced for `src/<name>.rs` orphans) when all of these hold:

1. It is a `.rs` file at least one directory below its package directory
   (the nearest `Cargo.toml`), and its first component is not `src`, `tests`,
   `benches` or `examples`. Root-level files such as `build.rs` never qualify.
2. No known module or include edge reaches it, it is not a crate root, and
   its package's `src/lib.rs` or `src/main.rs` is indexed.
3. No Rust source or `Cargo.toml` of the workspace names its file name
   anywhere (a `#[path]`, an `include!`, a `[[bench]]`/`[[test]]`/`[lib]`
   `path`, a `build =`), no source declares a `mod` of its stem, and no source
   builds a `mod` or `include!` operand from a macro metavariable.
4. The workspace has no unresolved `include!`, and every lexical-fallback file
   or unknown `#[path]` is itself unlinked: a file no root reaches never
   compiles its own declarations either.

The executable tests registered in such a file are withheld from the index, so
they are neither related tests nor oracles, and a parser refusal of the file
adds no lexical-fallback limitation. The file and its functions stay indexed,
and an owner inside it still reads as outside the module tree.

Any doubt keeps the file as before: it stays indexed with its tests. The
change only removes credit; it never adds a relation, raises a class or
strengthens an oracle.

## Required Evidence

- Positive: a haystack `#[test]` is not a related test of the changed owner,
  parsed or parser-refused, and the refused file adds no limitation.
- Negatives: the same file named by a `[[bench]]` path, by a sibling package's
  manifest, by a `#[path]` module, by an inline-module `#[path]`, by an
  `include!`, by a same-stem `mod`, below a macro that builds `mod` operands,
  or beside a lexical-fallback crate root keeps its tests; so does a package
  whose crate root is not indexed.

## Known Limits

- A parser-refused file still makes the workspace-wide macro-binding scan
  ambiguous when it spells `assert_eq!`, so a finding can stay
  `static_unknown` with `rust_assertion_context_unresolved` (fail closed,
  disclosed); it does not become a related test or a limitation. A follow-up
  can let the scan skip never-compiled files.
- Only the `.rs` file name is matched, not its directory: another file of the
  same name that is referenced keeps every same-named file (more doubt).
- Build scripts that write sources into the tree, `cargo script` files and
  other non-Cargo compilers are not modelled; a file only they compile loses
  its tests here.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- A general module resolver or a `cargo metadata` call.

## Acceptance Examples

1. `benchmarks/haystacks/code/rust-library.rs` holds a `#[test]` that calls the
   changed `discount`; the finding relates only `tests/api.rs`, and the run is
   `complete_with_findings` with no limitation naming the haystack.
2. The same file reached through `#[cfg(test)] #[path = "../benchmarks/..."] mod
   haystack;` is a related test and the refused parse is disclosed.
3. The same file named by a `[[bench]] path` keeps its tests.

## Test Mapping

- `crates/ripr/tests/outside_target_haystack.rs`.
- `crates/ripr/src/analysis/facts/stray_files.rs` unit and index tests.

## Implementation Mapping

- `crates/ripr/src/analysis/facts/stray_files.rs`: `is_stray_shape`,
  `never_compiled`.
- `crates/ripr/src/analysis/facts/role_composition.rs`:
  `files_outside_module_tree`, `withhold_tests_outside_module_tree`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
