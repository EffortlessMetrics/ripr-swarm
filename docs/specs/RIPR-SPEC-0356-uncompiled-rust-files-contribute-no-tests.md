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
- #14768 (benchmark inputs that contain module-generating syntax)
- #14962 (tests compiled only into a target `cargo test` skips: `[lib]
  test = false`, `[[bin]] test = false`, a bin's unmet default-feature
  `required-features`)
- #14972 (targets `cargo test` never runs: a conventional crate root under
  `autolib`/`autobins`/... `= false`, and an example's default `test = false`)

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
   `path`, a `build =`). Names compare ASCII-case-insensitively. No source
   declares a `mod` of its stem, and the file is not a `mod.rs`.
4. The Rust mount-token scan finds no uncertain mount operand in another
   indexed source, such as a macro-generated module or a computed `include!`.
   A literal `mod` inside a macro invocation may be forwarded into an
   out-of-line declaration.
   A macro binding named `$mod` is an ordinary metavariable. An uncertain
   operand inside the candidate still counts when judging other files.
   Escaped mount operands in other sources and escaped manifest paths
   also preserve uncertainty.
5. The workspace has no unresolved `include!`, and every lexical-fallback file
   or unknown `#[path]` is itself unlinked: a file no root reaches never
   compiles its own declarations either.
6. Manifest reads establish the Cargo context. A missing root manifest is
   allowed for a standalone nested package. An unreadable or malformed
   manifest preserves uncertainty. For a candidate's governing workspace,
   including one nested beneath the analysis root, the bounded path inventory
   must be covered by the index and its manifests examined. A member can use
   `#[path]` into another member without a Cargo dependency. Explicit source
   roots and build paths in the relevant manifests must also be covered,
   including roots that discovery skips. The path inventory still skips
   conventional Cargo build output.

The executable tests registered in such a file are withheld from the index, so
they are neither related tests nor oracles, and a parser refusal of the file
adds no lexical-fallback limitation. The file and its functions stay indexed,
and an owner inside it still reads as outside the module tree.
The workspace assertion macro-binding scan omits parser-refused files with
this source-role verdict. A parser-refused reachable or unresolved file
retains its assertion-binding uncertainty.

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
   writes a mount) or declares a `mod` of its stem; when any source may mount
   through a file the scan cannot name or read, judged on Rust tokens (a
   small lexer that drops comments and keeps string, raw-string and char
   literals whole, so neither a comment nor a `"http://x"` string can hide or
   fake a mount): a `macro_rules!` or `macro` body holding a `mod`, `include`
   or non-`::` `path` token (`mod /* c */ $n`); any `include` token other
   than an `include!` (any delimiter) whose operand is a string literal
   naming a `.rs` file (`include!(concat!(env!("OUT_DIR"), ..))`,
   `include!("gen.inc")`, and an alias such as `use std::include as inc;`
   whose `inc!` no rule could follow); a
   `path` attribute whose value is a metavariable or a non-`.rs` string
   (`#[path = "m.txt"]`); or `mod $name` anywhere; an ordinary
   `macro_rules!` (`($e:expr) => { $e * 2 }`) has none of those tokens and
   is no doubt; when a source spells a mount operand with escapes (an
   `include!` operand or `path` value holding a backslash names no `.rs`
   file the scan can match, however the tokens are spaced); when any file of
   the holder's ancestry other than its skipped roots is mounted from
   outside the ancestry by the same name-or-stem test (a `.../<dir>/mod.rs`
   is keyed on its directory: a `mod <dir>` declaration, or any spelling of
   `mod.rs`, since a relative operand such as `"../mod.rs"` need not spell
   `<dir>`; an unmounted `mod.rs` layout keeps the skip), or any ancestry
   file, the holder
   included, is listed by cargo metadata as a live or unmodeled target
   (`[[test]] path`, `build`, an example), so a `[[test]]` rooted at an
   intermediate module keeps the leaf's tests; or when
   the walk cannot finish (an unreadable file, more than 20,000 entries).
   So the verdict does not change with `--mode`.

The file and its functions stay indexed. Doctests are a separate rustdoc run
(`[lib] doctest`) and register after this pass, so a doc example keeps its
credit. An integration test under `tests/` is its own `test` target, so it
keeps its credit while the library's unit tests are withheld. A module that a
tested target also compiles (a `src/main.rs` declaring the library's module)
keeps its tests. Unavailable metadata, a root cargo does not list (outside
the auto-discovery rule below), and a `bench` or build-script root never
establish a skip.

#14972 extends rule 2 to two more shapes, through the same metadata owner:

- An `example` target is modelled like a `bin`: metadata reports an
  example's default `test = false`, and plain `cargo test` then builds the
  example as an executable without running its `#[test]` functions. An
  `[[example]] test = true` (with its `required-features` met) runs them, so
  that example `Runs` and keeps the credit. `cargo test --examples` is an
  explicit opt-in, as `cargo test --lib` is for a `test = false` library.
- A root cargo metadata does not list is a skip only when its package
  manifest sets a `[package]` `autolib`, `autobins`, `autoexamples`,
  `autotests` or `autobenches` key to `false` and cargo metadata lists that
  package as a workspace member
  (`ManifestInventory::crate_root_undiscovered`). Cargo did not discover the
  file as a target, so `cargo test` builds no crate from it. A root the
  manifest declares explicitly (`[lib] path`, `[[bin]] path`) is listed and
  keeps its credit.

Rule 3's probe premise widens accordingly: a false `auto*` key, an
`[[example]]` table, or a holder or root under the package's `examples/`
directory also spends the probe. Rule 5's mount scan and ancestry check
cover both shapes in full: unlike a metadata-listed skipped `lib` or `bin`
root, neither an undiscovered root nor a skipped example root is exempt from
rule 5's name-or-stem test. An undiscovered root is no listed target, and a
skipped example is no target another crate builds, so a source outside the
ancestry that spells its file name or declares a `mod` of its stem keeps its
tests even when the run's index does not hold that source or records no edge
for it. Examples: a `#[path = "../../a/src/main.rs"]` in another member, and
`mod ex { include!("../examples/demo.rs"); }` in `src/lib.rs` or
`tests/it.rs` (the index records no edge for an `include!` inside a module
body). The exemption for skipped `lib` and `bin` roots is unchanged from
#14962.

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
- A benchmark input with module-generating syntax in its own text stays out
  of related tests and assertion refusals. A declared `[[test]]`
  target and a `#[path]` module keep the same unusual path as related evidence.
- A sibling member outside Draft selection, a literal `mod` forwarded by a
  declared target's macro, and an unreadable or malformed root Cargo manifest
  each preserve the potential test in the shared source-role pass.
- A nested governing workspace and a declared test root under `target/` also
  preserve possible mounts. Removing the declared target restores the data-only
  verdict with the same generated-output directory on disk.
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
  without Rust sources. Round 5: an `include!("gen.inc")` and a
  `#[path = "m.txt"]` mount of a non-`.rs` file, and a macro mount
  `mod /* c */ $n` after a `"http://x"` string, read in every mode what
  their tests-on controls read (`exposed`).
- Undiscovered and example targets (#14972): `#[test]` in `src/main.rs`
  under `autobins = false`, in `src/lib.rs` under `autolib = false`, and in
  `examples/demo.rs` (default `test = false`) does not credit a
  discriminator in any `--mode`. The autodiscovered controls and the same
  root declared explicitly under `auto* = false` read `exposed` in every
  mode. `[[example]] test = true` reads `exposed` in every mode except
  `instant`, which does not index `examples/` and reads `no_static_path` on
  base too. An `autobins = false` `src/main.rs` that another member mounts
  through `#[path]` reads `exposed` in every mode. The withheld shapes read
  exactly `no_static_path` in every mode: the undiscovered bin and lib, a
  default example, a multi-file `examples/demo/` directory, an `[[example]]
  test = true` whose `required-features` are unmet, and an example whose
  `required-features` are on by default but which leaves `test` at `false`.
  An example root that `src/lib.rs` (`#[cfg(test)] mod ex { include!(..) }`)
  or `tests/it.rs` (`mod demo { include!(..) }`) compiles credits a
  discriminator in every mode except `instant`, as on base (#14972
  review).

## Known Limits

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
- Skipped targets, under-credit (#14962 review n8): macros defined outside
  the scanned sources are not modelled. A proc-macro (for example a
  workspace proc-macro member), or a macro or `include` re-export from a
  non-member path dependency (`ext::inc!(concat!(env!("OUT_DIR"), ..))`),
  that emits a `mod` declaration or a `#[path = ...] pub mod m;` built by
  string formatting can mount the file into a tested target without any
  token the scan recognizes, and the file's tests are then withheld
  although `cargo test` of that target runs them.

- Undiscovered and example targets (#14972): a file in another package
  compiled only through `#[path]` by a skipped target is still judged by its
  own package's premise (the #14962 limit). Within one package, a `src/`
  module mounted through `#[path]` only by an example that `cargo test`
  skips also keeps its credit when the package manifest has no `[[example]]` table and no
  false `auto*` key, because the holder's package then sets no skip premise;
  with either, the probe runs and the module's tests are withheld. A bench
  root is not modelled.
- Undiscovered and example targets, over-credit (#14972 review): the mount
  scan stays textual for `include!`, so an example `include!`d only by
  another target that `cargo test` does not run (another skipped example, a
  `test = false` lib or bin, a bench, or an undiscovered test file) keeps
  its credit, as on base. The same mount through `#[path]` is an indexed
  edge, so the skipped holder is seen and the credit is withheld.
- Undiscovered and example targets, under-credit (#14972 review): the
  `required-features` check reads only the package's own default feature
  closure, the same owner bins use (#7266). A `test = true` example (or a
  `[[bin]]`) whose required feature is enabled only through another
  workspace member's dependency edge (`a = { path = "../a", features =
  ["special"] }`) runs under a workspace `cargo test` by feature
  unification, but reads as skipped here and loses its credit. Tracked for
  both target kinds in #15079.
- `autolib` needs Cargo 1.83 or later; an older Cargo ignores the key and
  lists the library, which keeps the credit.
- Undiscovered and example targets, over-credit (#14972 review): cargo
  metadata does not report `harness`, so an `[[example]] test = true
  harness = false` or a `[[bin]] harness = false` reads as a running target,
  and its `#[test]` functions keep their credit on base and here alike,
  although the harness-less binary runs none of them.

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
- `crates/ripr/src/analysis/classify/owner_pin.rs`: `scan_macro_sites` (#14768).
- `crates/ripr/src/analysis/workspace/cargo_targets.rs`:
  `ManifestInventory::crate_root_unit_tests_skipped` (#14962).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
