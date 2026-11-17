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
- #14962 (tests compiled only into a target `cargo test` skips: `[lib]
  test = false`, `[[bin]] test = false`, a bin's unmet default-feature
  `required-features`)

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
3. No Rust source or `Cargo.toml` of the workspace mentions its file name as a
   substring anywhere (a `#[path]`, an `include!`, a `[[bench]]`/`[[test]]`/`[lib]`
   `path`, a `build =`), no `path` attribute or key, or `include!` operand, contains a
   backslash (escape, line continuation or separator), names compare
   ASCII-case-insensitively, no source declares a `mod` of its stem, no source builds a `mod`
   or `include!` operand from a macro metavariable (any `$` beside a
   `mod`/`include` word, whatever comments or strings intervene) or passes a
   computed `include!` operand, and the file is not a `mod.rs`.
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

### Targets `cargo test` skips (#14962)

A file Cargo does compile can still hold tests that never run. `[lib] test =
false` keeps the library's unit-test binary out of `cargo test`, which prints
`running 0 tests`, so a `#[cfg(test)] mod tests` in `src/lib.rs` or in any
module the library declares is not a discriminator. The executable tests of a
file are withheld, exactly as for an uncompiled file, when all of these hold:

1. Composition knows the file's every module ancestry (no include edge,
   lexical fallback, or unresolved, ambiguous or cyclic edge), so each
   execution root is known.
2. Every execution root is the `src_path` of at least one cargo metadata
   target, and `cargo test` runs none of them: each is a `lib`-family or
   `bin` target whose metadata `test` flag is false or whose
   `required-features` are unmet under cargo defaults. The query is
   `ManifestInventory::crate_root_unit_tests_skipped`, the same metadata
   owner that already decides `[[test]] test = false` (#7265) and
   `required-features` (#7266). A skipped `[[test]]` target keeps those
   existing owners (the harness verdict and the #8016 file drop) and never
   establishes a skip here; a live `[[test]]` whose `path` is the root keeps
   the root's tests.
3. The holder's package manifest spells `test = false` on `[lib]` or a
   `[[bin]]`, or a `[[bin]]` names `required-features`. This only gates the
   metadata probe so an ordinary workspace spends no `cargo metadata`
   subprocess.
4. No `Cargo.toml` is dirty in a committed-history check: the probe reads the
   working tree, not the `HEAD` bytes the files were loaded from (#8016).
5. No other target can mount the file, judged from every workspace source on
   disk rather than from the run's index, because a draft or instant run
   indexes less and a build-script `include!` never enters any index (#14962
   review). The walk covers the workspace root and every cargo metadata
   member, skipping only Cargo's build directories (`target` directly under
   the root or a member, and `CARGO_TARGET_DIR`) and `.git`, so a module
   directory named `target` or a hidden source directory (`#[path =
   ".gen/m.rs"]`) is read; in a committed-history check it also reads the
   `HEAD` text of dirty files. A symlinked `.rs` file, or a symlinked
   directory that holds a `.rs` file or another symlink (read up to 1,000
   entries), can alias the file under a name no text ties back to it, so it
   is doubt; a directory link without Rust sources is not. The
   file keeps its tests when any source outside its own skipped ancestry
   spells its file name (a `#[path]`, a static `include!`, a build script that
   writes a mount) or declares a `mod` of its stem; when any source builds a
   `mod`/`include!` operand dynamically (an `include!` whose `(`, `{` or `[`
   delimiter is not followed by a plain string literal, such as
   `include!(concat!(env!("OUT_DIR"), ..))`, or a macro metavariable as a
   `mod` name or `path` value: `mod $name`, `path = $p`, read with whitespace
   ignored and again with comments stripped, so `mod /* m */ $n` counts) or
   spells such an operand with escapes; an ordinary
   `macro_rules!` (`($e:expr) => { $e * 2 }`) is no doubt; when
   the file is a `mod.rs`; when cargo metadata lists the file itself as a
   live or unmodeled target (`[[test]] path`, `build`, an example); or when
   the walk cannot finish (an unreadable file, more than 20,000 entries).
   So the verdict does not change with `--mode`.

The file and its functions stay indexed. Doctests are a separate rustdoc run
(`[lib] doctest`) and register after this pass, so a doc example keeps its
credit. An integration test under `tests/` is its own `test` target, so it
keeps its credit while the library's unit tests are withheld. A module that a
tested target also compiles (a `src/main.rs` declaring the library's module)
keeps its tests. Unavailable metadata, a root cargo does not list, and an
`example`, `bench` or build-script root never establish a skip.

The withheld changed owner reads `no_static_path` like any owner without a
running test; no named stop reason is added on this issue.

## Required Evidence

- Positive: a haystack `#[test]` is not a related test of the changed owner,
  parsed or parser-refused, and the refused file adds no limitation.
- Negatives: the same file named by a `[[bench]]` path, by a sibling package's
  manifest, by a `#[path]` module, by an inline-module `#[path]`, by an
  `include!`, by a same-stem `mod`, below a macro that builds `mod` operands,
  or beside a lexical-fallback crate root keeps its tests; so does a package
  whose crate root is not indexed.
- Skipped targets (#14962): `[lib] test = false` unit tests, in the root or a
  child module, and `[[bin]] test = false` unit tests do not credit a
  discriminator (nor, as before, a `[[test]] test = false` target), while
  the same tests with `test` on read
  `exposed`; an integration test and a doc example of the `[lib] test =
  false` crate still read `exposed`, as does a module a tested bin also
  declares; and a same-run index extension that revives withheld tests
  matches a fresh build.
- Mounts (#14962 review): a `#[path]` mount of the file from another member,
  from a `tests/` target, from a build-script-generated `include!`, from a
  module directory named `target` and through a symlinked alias keep
  `exposed` in every `--mode`; a mount from a hidden source directory reads in
  every mode what the same tree reads with the tests left on, with the test
  still related (that layout's assertion binding stays `static_unknown`
  either way); a generated `include!{..}`/`include![..]` mount and a macro
  mount `mod /* m */ $n` into a tested bin read in every mode what their
  tests-on controls read (`exposed`); the plain skip reads the same in every
  mode, also beside an ordinary `macro_rules!` and a symlinked directory
  without Rust sources.

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
- Skipped targets (#14962): a non-member package outside the workspace root
  that `#[path]`-mounts the file is not walked. The mount scan is textual, so
  an unrelated mention of the file name or a same-named `mod` anywhere keeps
  the credit (more doubt, never less). A file whose skipped ancestry the run
  did not index keeps its tests (`--mode instant` can then credit what
  `ready` withholds, the over-credit direction the base already had).
  `[lib] harness = false` is not modelled. Further over-credit limits, all
  in the doubt direction: a same-stem `mod` anywhere keeps the file (the stem
  is not tied to a directory), a mention inside a comment counts, a file
  named `tests.rs` is matched by name like any other, and a tree with more
  than 20,000 entries fails open to credit.
- Skipped targets, under-credit (#14962 review n4): the walk does not read
  Cargo's build directories, so a source kept under a member-root `target/`
  directory that mounts the file (`#[path = "../target/m.rs"]` whose
  `m.rs` names `a/src/shared.rs`) is missed, and the file's tests are
  withheld although `cargo test` of the mounting crate runs them. Sources
  under a build directory are unusual and Cargo may delete them on
  `cargo clean`.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- A general module resolver. The haystack rule makes no `cargo metadata`
  call. The skipped-target rule uses the existing `ManifestInventory` probe,
  but in a new instance per composition pass: a repository whose manifest
  switches a lib or bin `test` off (or a bin names `required-features`) pays
  one `cargo metadata --no-deps --offline` per index build (a check builds up
  to two), plus one walk of the workspace sources once a candidate passes the
  metadata checks. Other repositories pay nothing.

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
- `crates/ripr/tests/lib_test_false_credit.rs` (#14962).
- `crates/ripr/src/analysis/workspace/cargo_targets.rs`:
  `crate_root_unit_test_skip_follows_metadata_test_flags` (#14962).
- `crates/ripr/src/analysis/facts/role_composition.rs`:
  `mount_operand_doubt_needs_a_dynamic_mount_operand` (#14962 review).
- `crates/ripr/src/analysis/seam_inventory.rs`:
  `reuse_reviving_cargo_test_skipped_tests_matches_fresh` (#14962).

## Implementation Mapping

- `crates/ripr/src/analysis/facts/stray_files.rs`: `is_stray_shape`,
  `never_compiled`.
- `crates/ripr/src/analysis/facts/role_composition.rs`:
  `files_outside_module_tree`, `withhold_tests_outside_module_tree`,
  `files_in_skipped_unit_test_targets` (#14962).
- `crates/ripr/src/analysis/workspace/cargo_targets.rs`:
  `ManifestInventory::crate_root_unit_tests_skipped` (#14962).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
