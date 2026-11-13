# Passive formal source prerequisites

Owner: #14472, shared by the native producer #14545 and registry #14473.
Contract: [RIPR-SPEC-0371](../specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md).
This is the bounded source-preparation part of the existing native-custody
adoption plan, not a second verifier, registry, result parser or CI authority.

## Maintained API

`xtask::formal_source_inputs` has two passive functions:

```rust,ignore
extend_source_inputs(root, &mut paths, packages) -> Result<BTreeSet<String>, String>
validate_harness_configuration(source, bare_name, expected_unwind) -> Result<(), String>
```

The first extends a supplied ordinary source inventory and returns its explicit
include/path/build-script inputs. The caller owns the initial module/dependency
inventory and effective compiler configuration. The second checks one uniquely
named ordinary proof function and its exact optional local unwind declaration.
Neither function runs Cargo, Git, Kani, a shell or a network process, emits a
native result, authenticates a producer, or grants assurance.

Both use the existing shared native path/read helpers and the existing
`ra_ap_syntax` dependency. The `formal-source-inputs` feature enables this module
without the product library. Default `automation` also enables it. The existing
`formal-evidence-audit` command remains declaration-only and does not call this
module or gain source-currentness authority merely because it is available.

## Supported source preparation

Selected packages are `ripr` and `ripr-contract`. For the selected package source,
its dependent contract source, and runner source supplied by the caller, the
scanner follows literal `include!`, `include_str!`, `include_bytes!`, and external
module `#[path = "..."]` attributes. It also includes existing selected-package
and runner `build.rs` inputs and their literal includes. A file's name alone,
including a name containing `test`, never excludes it.
Direct includes in documentation attributes enter the inventory as well.

Whitespace and comments between Rust syntax tokens do not change recognition.
Raw identifiers normalize only as identifiers; whitespace and comment-looking
bytes inside string literals are retained. The accepted literal subset is a
single double-quoted string decodable by the existing JSON string decoder.
Raw string syntax and Rust-only escape forms outside that subset refuse rather
than being guessed. Conditional and inline module path semantics remain outside
this profile. Macro-generated and dynamic include arguments refuse explicitly.
Visible includes nested inside another macro's token tree also refuse; text
inside a string literal does not count as a visible macro invocation.

Explicit `cfg(test)` ancestry propagates through known external module edges.
Any production incoming edge defeats test-only exclusion. The ancestry pass
parses all candidate Rust inputs; a comment-bearing module declaration must not
vanish because a lexical search expected `mod` followed by a literal space.
Malformed module ancestry refuses. A pure test-only descendant's unsupported
include need not enter the non-test input set, but a production use restores the
obligation to inspect it.

The ordinary-harness profile requires exactly one `kani::proof`, at most one
positive local `kani::unwind`, and equality with the declared override. Only
actual `doc` attribute forms and the named lint attributes are permitted beside
those Kani attributes. A name merely starting with `doc` is not a documentation
attribute and cannot authorize a transforming macro. Other verifier attributes,
stubbing, expected-panic inversion, ambiguous function names and mismatched
bounds remain refused.

## Caller obligations and nonclaims

This helper does not discover every ordinary module/dependency root, expand
procedural or declarative macros, evaluate arbitrary cfg/build scripts, or
reproduce rustc's full input resolution. In particular, newly included Rust that
introduces additional ordinary modules or context-sensitive resolution needs an
independently complete seed or a separately admitted closure rule. A successful
call alone is not permission to claim complete consumed-source custody.

Registration and native capture must use the same shared helper, supply their
independently collected root inventories, admit effective configuration, and
compare actual captured source manifests. They must not trust the output of one
side as an independent observation of the other. Before/after equality is not an
immutable snapshot and cannot rule out intervening changes.

`paths` can be partially extended when a later input refuses. On **any** error,
discard that partial set; never publish it as a complete manifest. Read/hash and
provider/result admission remain separate operations. Seed data files are not
all read or hashed here. No successful source scan is a proof of a proposition,
current result, producer identity or safe proof transition.

## Bounds and failure handling

The supplied and extended inventory is limited to 20,000 entries, including
checks before traversal and after adding build roots. Source reads are bounded
to 8 MiB per file. Each ancestry scan limits accumulated source bytes to 256 MiB.
Those are per-pass bounds: fixed-point discovery can read a file again, so this
is not a total I/O, wall-time or peak-memory guarantee. Extra parsing cost for
trivia-independent ancestry has not been measured.

Existing path helpers require repository-relative inputs. Source paths and
includes reject symlinks and out-of-root canonical targets. The caller still
owns filesystem race/custody and the complete non-Rust configuration inputs.
Unknown or unsupported source constructs return an error instead of pretending
that the missing input does not matter.

## Qualification and maintenance

Use the repository-pinned toolchain on the exact candidate:

```bash
cargo test --locked -p xtask --no-default-features \
  --features formal-source-inputs --lib formal_source_inputs::
cargo test --locked -p xtask --no-default-features \
  --features formal-source-inputs --lib
cargo clippy --locked -p xtask --no-default-features \
  --features formal-source-inputs --lib -- -D warnings
cargo build --locked -p xtask --no-default-features \
  --features formal-source-inputs --lib
cargo fmt --check
```

The focused module contains twenty-one controls on Unix; the symlink control is
Unix-only, so other platforms have twenty. Counts describe authored inventory,
not a claim that this candidate has executed. Thin binary or unrelated filtered
test runs do not qualify these controls. Follow with default-target composition,
traceability, doc/policy checks and the applicable protected route.

Retain donor controls for qualified includes, dynamic and macro inputs, symlinks,
test/production ancestry, build scripts and inline-path refusal. New controls
challenge commented/raw path and include syntax, conditional paths, a production
edge hidden from a test-only graph, malformed ancestry, the initial size bound,
ordinary-harness trivia, doc-prefix lookalikes, nested macro inputs, direct
documentation includes, recursive Rust inputs, malformed bytes and path escapes.
For the new refusal cases,
first observe the intended behavior against the donor or a reversible removal;
compiler/setup failure is not a discriminating negative result.

Changing the parser, accepted syntax, roots, exclusion law or harness profile
requires these controls and fresh collector/registry correspondence evidence
before renewing affected native results. Preserve source-bound historical runs
under their original identities; do not transplant a donor's native pass to this
reduced candidate.

## Adoption and rollback

This module comes from contribution `1730337646b6d4550eafbc5037351e0ec64f7794`,
with repaired syntax recognition and direct controls. The existing native DTO
and parser precursor supply its helpers; native property/profile activation is
not a dependency of this passive stage. Later producer and registry composition
must import this module, replacing the donor's nested source scanner rather than
keeping both implementations. No new binary, workflow or result authority is
introduced here.

Preserve #14469's stronger cover law and #12581's bounded execution when those
separate changes compose. #14545 retains the producer; #14473 retains registry
and currentness; #14474/#14500 retain selection and actual-group execution;
#6452/#6456 retain proof-transition protection. This stage closes none of them.

Rollback is an ordinary protected revert of the module, feature and associated
contract material, coordinated with any later callers. It does not delete
historical evidence, retire a claim or change repository protections.
