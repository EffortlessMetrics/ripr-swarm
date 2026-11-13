# Rust 1.100 compiler upgrade

Governing issue: [#14539](https://github.com/EffortlessMetrics/ripr-swarm/issues/14539).

## Compiler and source compatibility

The development and release compiler is Rust **1.100.0**. The source minimum
remains **1.95**. These are separate promises:

| Setting | Authority | Meaning |
| --- | --- | --- |
| Build compiler | Root `rust-toolchain.toml`, exact CI inputs | Contributors and release builders use 1.100.0. |
| Zed build compiler | `editors/zed/rust-toolchain.toml` | Same compiler, plus the `wasm32-wasip2` target. |
| Source minimum | `[workspace.package] rust-version` in `Cargo.toml` | Published source remains buildable with Rust 1.95. |
| Clippy API suggestions | `clippy.toml` and `policy/clippy-lints.toml` | Suggestions must respect the source minimum. |
| Language edition | Workspace manifests | Rust 2024; changing compilers does not change the edition. |
| Formal verifier | Existing Kani version and its compiler bundle | Independent compiler identity; a stable compiler upgrade is not a verifier upgrade. |

Rust 1.100 was [released on November 12, 2026](https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/).
This implements the decision to adopt it before the release if available.
Keeping the source minimum avoids an unnecessary source-install restriction
while obtaining compiler and Cargo improvements in released binaries.
Using APIs stabilized after 1.95 requires a separate minimum-version decision
and compatibility proof; a newer build compiler alone does not permit them.

## What the five releases contribute

| Release | Relevant change | RIPR consequence |
| --- | --- | --- |
| [1.96](https://blog.rust-lang.org/2026/05/28/Rust-1.96.0/) | Standard `assert_matches!`, new range types, stricter WebAssembly linking and Cargo security corrections | Review assertion ownership and parser coverage; build the locked Zed WASI extension. New APIs are optional source changes. |
| [1.97](https://blog.rust-lang.org/2026/07/09/Rust-1.97.0/) | v0 mangling becomes the default; Cargo gains `build.warnings` | Check ordinary symbol/backtrace consumers. Cargo warning policy is a possible later cache improvement, not a replacement for the current strict Clippy contract. |
| [1.98](https://blog.rust-lang.org/2026/08/20/Rust-1.98.0/) | Buffered integer formatting and source-relative substring/slice helpers | Potential measured output-formatting and span-handling improvements after an MSRV decision. They do not reduce output tokens by themselves. |
| [1.99](https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/) | LLVM 23; Cargo changes inherited default-feature behavior and CI incremental defaults | Recheck compiler/LLVM identity, feature selection and cache assumptions. Retain explicit repository settings and the locked dependency graph. |
| [1.100](https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/) | New Cargo intermediate layout; general never type; allocator and cleanup APIs | Repair layout-dependent consumers. Compile older-edition fixtures and inspect `Infallible`/inference assumptions. Allocator and `DropGuard` adoption remains separate work. |

The [1.96.1 compiler, Cargo networking and libssh2 corrections](https://blog.rust-lang.org/2026/06/30/Rust-1.96.1/),
[1.97.1 LLVM miscompilation correction](https://blog.rust-lang.org/2026/07/16/Rust-1.97.1/),
[1.98.1 vtable correction](https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/)
and [1.99.1 FFI/linker corrections](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/)
are included in the selected compiler. Their existence is a reason to use the
complete current release; it does not establish that each intermediate
regression affected 1.95.

Rust 1.96 also fixes two Cargo vulnerabilities affecting third-party registries;
the announcement explicitly excludes crates.io users from those two issues.
Cargo 1.100's optional dependency cooldowns are a separate policy decision:
they affect new resolution, preserve already-locked versions, and are not a
replacement for dependency review. This upgrade keeps the dependency graph
locked and does not enable a cooldown implicitly.

The full [release notes](https://doc.rust-lang.org/releases.html) also identify
stricter trait/inference checks, Unicode and debug-escaping changes, and new
rustdoc diagnostics. Compile, lint, doctest and output-contract evidence must
decide their effect on this repository. Do not suppress a warning or bless a
golden solely because the compiler changed. The workspace already uses edition
2024, so the never-fallback change to older editions primarily concerns its
dependencies and fixtures. Cargo install now uses a packaged lockfile by
default; retained `--locked` commands still communicate intent and support
older Cargo clients.

No build-time, memory, binary-size or throughput percentage is claimed by this
upgrade. Measure the same source and inputs with separately identified cold
and warm targets before attributing a performance change to the compiler.

### Source and analyzer audit

The unchanged baseline passes Rust 1.100 formatting, but strict library Clippy
reports 20 diagnostics. The upgrade repairs the reported control-flow,
iteration and ownership patterns without adding suppressions. Fixed-size byte
chunks retain little-endian ordering and the padded final hash mix; module-walk
queue transitions transfer their vectors without an unnecessary drain/collect
allocation. Cargo's new `redundant_homepage` warning is resolved by retaining
the authoritative repository URL and removing duplicate homepage metadata.
`manual_pop_if` is recognized by the new compiler and moves from the planned
lint ledger into the active warning policy, with API suggestions still bounded
by the 1.95 source minimum.

The all-target pass also exposes 12 release-tooling Clippy diagnostics, repaired
with the same ownership and default-value semantics. Two LSP test calls need
explicit elimination of their impossible error value: stringifying `Infallible`
becomes an unreachable call after it aliases `!`. The empty-match elimination
compiles with warnings denied on both 1.95 and 1.100; it preserves the service
calls and session-end assertions.

A bounded source audit found no inherited `[workspace.dependencies]` feature
configuration, competing `Infallible` implementations, or local legacy-symbol
demangler requiring changes. The two manual `Ord` implementations compare the
same field as equality and delegate `PartialOrd` to that ordering. This source
review complements compilation and tests; it is not exhaustive dependency
compatibility proof.

The assertion oracle already recognizes standard `assert_matches!` and
`debug_assert_matches!` spellings, including qualified paths. Its conservative
name-scope list does not yet admit those forms everywhere that existing
assertions are admitted. Likewise, existing debug-profile and divergence
analysis must not be described as complete support for every newly stabilized
form. These are separately scoped analyzer opportunities, not compiler-upgrade
requirements or newly claimed language support.

## Cargo artifact compatibility

Cargo's [build-cache contract](https://doc.rust-lang.org/cargo/reference/build-cache.html)
distinguishes final outputs from intermediates. Final executable paths remain
usable; intermediate directories are explicitly unstable. Prefer Cargo JSON
`compiler-artifact.executable` and `filenames` wherever this repository owns the
build command. Existing native libtest and server-archive producers already
consume emitted paths.

The coverage-failure observer does not own or capture the coverage command's
streams. It retains a bounded, no-follow filesystem adapter for known layouts.
The 1.100 adapter must inspect native Cargo-produced test artifacts without
moving them into a legacy `deps` directory. Its existing read, entry, time and
JSON budgets, ambiguity handling and original failure status remain binding.
Unsupported build roots remain explicit limitations rather than a claim that
the executable does not exist. See [coverage diagnostics](coverage.md) and
[RIPR-SPEC-0266](../specs/RIPR-SPEC-0266-coverage-binary-id-diagnostics.md).

`doctor` also recognizes a Cargo source build when the profile has the new
build markers instead of `.fingerprint` and `deps`. An installed copy still
uses the installed-binary path. This affects diagnosis, not permission to
execute arbitrary workspace configuration.

Cargo test executable basenames still carry their hash in 1.100. The native
probe and the exact Cargo `use_extra_filename` implementation establish that
the Windows transcript parser does not need a speculative basename rewrite.

## LLVM and coverage identity

Native toolchains and their exact upstream LLVM sources establish this change:

| Component | Rust 1.95.0 | Rust 1.100.0 |
| --- | --- | --- |
| LLVM | 22.1.2 | 23.1.5 |
| Raw profile version | 10 | 11 |
| Indexed profile version | 13 | 14 |
| Coverage mapping version | 6 | 6 |
| Raw header size used by binary-ID diagnostics | 128 bytes | 152 bytes |

Definitions: [1.95 LLVM](https://github.com/rust-lang/llvm-project/blob/1cb4e3833c1919c2e6fb579a23ac0e2b22587b7e/llvm/include/llvm/ProfileData/InstrProfData.inc)
and [1.100 LLVM](https://github.com/rust-lang/llvm-project/blob/0a5579eb649d3634a9c30ac873300cc2944fa070/llvm/include/llvm/ProfileData/InstrProfData.inc).
RIPR already reads both binary-ID headers and rejects unknown/flagged formats.
It does not decode the full counter payload. LLVM remains the profile
conversion/export authority.

Always use the selected compiler's `llvm-tools-preview` component. The
[compiler coverage documentation](https://doc.rust-lang.org/rustc/instrument-coverage.html)
warns that raw-profile processing may require the exact LLVM version used to
compile. Never reuse an old raw profile as new-compiler proof. Coverage already
used v0 symbol mangling before this upgrade; the ordinary-symbol default change
is a separate compatibility question.

The marked recipe in [Validate locally](coverage.md#validate-locally) deliberately
retains **previous baseline 1.95.0 versus current stable**. It exercises both raw
versions after the repository build pin moves. Exact 1.100 proof also runs with
1.100 explicitly selected, so a later stable release cannot rewrite its meaning.

## Maintaining the upgrade

1. Read the root pin, declared source minimum, Zed pin and independently pinned
   verifier versions before changing anything. Preserve historical receipts and
   synthetic older-version fixtures.
2. Resolve every operational CI compiler selector. A generated dtolnay action
   can hard-code its old compiler and ignore `with.toolchain`; use the reviewed
   generic action revision with an explicit exact version. Preserve permissions,
   triggers, target-specific setup and publication boundaries.
3. Keep the MSRV lane explicit: `RUSTUP_TOOLCHAIN=1.95.0` and
   `cargo +1.95.0 check --locked --workspace --all-targets`. Installing an older
   compiler without overriding the repository pin does not test the minimum.
4. Use [the checked provisioner](../../CONTRIBUTING.md#cloud-install-and-restored-task-recipes)
   to refresh an environment from repository requirements. Installing the new
   pin locally does not update a separately published cloud image.
5. Run matching rustfmt and strict workspace/all-feature Clippy, affected
   policy and integration tests, doctests, and the existing golden/fixture gates.
   Reproduce a new failure on the relevant exact base before calling it inherited.
6. Run real compiler/LLVM controls, including a native Cargo-layout executable
   discovery case. Retain test counts, native exits, compiler identities and
   source SHA. An offline tool shim is not native compiler proof.
7. Let the selected PR complete the installed hosted qualification route,
   including its platform and editor selections. Candidate package/install and
   release-source integration still follow their separate existing authorities.

Current runner routing uses capability labels (`em-ci`, model and capacity),
not a compiler-version label. The toolchain action prepares the exact compiler
inside the job. A missing self-hosted capability falls back through the current
router; the upgrade does not change live runner registrations or settings.

Kani 0.68.0 carries its own
[nightly compiler pin](https://github.com/model-checking/kani/blob/kani-0.68.0/rust-toolchain.toml).
Keep that identity separate from the compiler running `xtask`; a passing
stable build does not establish a verifier result.

## Optional follow-up opportunities

Benchmark integer `format_into` in a demonstrated formatting hot path before
replacing existing formatting. Evaluate cleanup guards against failure,
rollback and resource-custody tests before changing lifecycle code. Treat
custom allocators as measured memory/locality experiments; the existing
`unsafe_code = "forbid"` contract remains intact. New standard assertion forms
need the same qualification, shadowing and debug-build checks as existing
oracles. Avoid algebraic floating-point shortcuts in deterministic scoring.

These are opportunities, not missing prerequisites for the compiler bump.
Their adoption requires its own source-minimum and verifier-compatibility
judgment. Exact implementation and qualification receipts belong to #14539
and its PR; this document is the maintenance rationale, not a release receipt.
