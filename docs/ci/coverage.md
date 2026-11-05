# Coverage Reports

Coverage reporting is **execution-surface evidence only**. It shows whether changed code paths executed during testing, but does not prove correctness, conformance, safety, completeness, or mutation adequacy.

## What coverage measures

`ripr`'s coverage reports use `cargo-llvm-cov` to generate line and branch coverage over:
- Product code: `crates/ripr/src/`
- Automation code: `xtask/src/`

Coverage artifacts are excluded for:
- `target/` (build output)
- `fixtures/**/target/` (fixture build output)
- `editors/vscode/**/node_modules/` (extension dependencies)
- `xtask/src/reports/release.rs` (release automation)

## Claim boundaries

The Codecov badge and coverage reports do **not** prove:
- Test discriminator adequacy (coverage does not equal mutation killing)
- Seam classification completeness
- Oracle strength across all five RIPR stages
- Static analysis correctness (dynamic mutation testing required)
- Reproducibility across different versions of `cargo-llvm-cov`

Coverage is **advisory** and does not block merges. Codecov status checks are informational by default.

## Baseline targets

As of 2026-05-07, the project coverage baseline is 75.5%:
- **Product code** (`crates/ripr/src/`): 94.8% coverage; target 94% (project), 94% (patch)
- **Automation** (`xtask/src/`): 59% coverage; target 59% (project), 75% (patch)

Thresholds:
- **Project**: 1% for product, 1% for automation
- **Patch**: 3% for product, 10% for automation

## Manual verification

Use `workflow_dispatch` on the `Coverage` workflow to verify after changing:
- `.github/workflows/coverage.yml`
- `codecov.yml`
- Coverage-related policy entries
- Test topology that may affect `cargo-llvm-cov`

Expected artifacts after a coverage run:
- `lcov.info`, uploaded by GitHub Actions as the `rust-lcov` artifact.
- `target/ripr/reports/coverage-diagnostics/`, uploaded as
  `coverage-failure-diagnostics` only when coverage generation fails, including
  failures before profile merging. This bounded
  artifact records profile-list inventory, validated replay status, limited
  malformed-profile prefixes, and tool diagnostics; it does not change the
  failed coverage result. Tool-stream UTF-8 validity flags describe only the
  retained bounded prefix; bytes beyond that prefix are discarded. Exact
  retained bytes drive warning-path matching, while display text is sanitized.
  The required Linux LLVM replay test fails if Rust or matching LLVM discovery
  fails; unsupported platforms exercise the adapter refusal control.

The failure manifest accepts LLVM raw64 little-endian profile versions 10 and
11 for binary-ID extraction. Version 10 has a 128-byte header; version 11 has a
152-byte header. The ID block starts after that version's complete header, and
available or partial ID results report `llvm_raw64_le_v10` or
`llvm_raw64_le_v11`. Other versions, variant flags and magic values remain
explicitly unsupported; accepting an ID does not validate the rest of a profile.

Replaying an older profile with newer LLVM can report a raw profile version
mismatch. The manifest retains that warned profile's bounded prefix as
`raw_profile_version_mismatch` only when the warning names an exact validated
snapshot input. Unknown warnings and unrelated paths remain unclassified.
This retains diagnostic evidence without converting the profile or changing
the failed coverage result.

The failure manifest also associates supported raw-profile binary IDs with GNU
build IDs in instrumented Linux ELF files found only as direct entries in the
coverage target's `debug`, `debug/deps`, `release`, and `release/deps`
directories. It captures bounded
identifiers and file metadata, never whole binaries. Exact ID equality and LLVM
profile sections are required; distinct instrumented candidates with the same
ID remain ambiguous, while duplicate directory entries of one executable
(cargo's hard-linked `debug/<name>` copy of `debug/deps/<name>-<hash>`) collapse
to a single candidate by file identity.
Unavailable, unmatched within the inspected scope, and truncated results remain
explicit. Inspection is limited to 1,024 directory entries, 256 files, 2 MiB of
reads (64 KiB per file), 16 matches, 32 KiB of mapping JSON, and two seconds.
The profile parser retains the valid prefix when a later ID exceeds the 32-ID
limit, reports that profile as partial, and marks the aggregate mapping partial
so unmatched retained IDs are not treated as definitive misses. An unavailable
profile-ID input also keeps unmatched IDs inconclusive. A readable,
structurally parsed executable without LLVM profile sections is counted as a
noncandidate; malformed, unsupported, or unreadable inputs remain unavailable
and can make the scope partial. Malformed/unsupported layouts and mapping
failures do not discard profiles or change the original failed coverage
result. A build-ID association does not
identify an owning test, process termination, or the cause of a short profile.

Codecov upload requires the `CODECOV_TOKEN` secret and runs only for trusted
pushes, manual dispatches, and same-repo pull requests. Fork pull requests still
generate `lcov.info` and upload the `rust-lcov` artifact, but skip Codecov
upload because repository secrets are unavailable.

### Storage observations during coverage

The Linux workflow invokes the prebuilt repo-policy run-coverage adapter. It
runs exactly cargo llvm-cov --workspace --all-features --lcov --output-path
lcov.info with inherited stdin, stdout, stderr and environment. The native
exit code is preserved; a signal is recorded and returned as shell status
128 plus that signal. Observation errors and sampling limits do not stop
Cargo. The existing 45-minute workflow timeout remains the execution limit.

Before the command, at most 46 times at 60-second intervals, and at completion,
the adapter emits coverage_storage.v1 records prefixed ripr-coverage-storage.
Summary, bucket, inspected-profile-entry and end records share a sequence
number. Each record starts on a new line, uses one write of at most 4 KiB, and
flushes immediately so it stays identifiable beside concurrent test output.
A complete sample uses at most 16 KiB; all 48 possible samples use at most
768 KiB. Oversized records, omitted buckets and short writes remain explicit.
Missing end records mean the sample was interrupted. Worker loss can still
prevent delivery of a final record.

The metadata-only scan separates plain build output, instrumented build/state,
direct raw profiles, target/ripr artifacts, fixture native-target directories,
and remaining target scratch. Cargo force-routes temporary files into target,
so these fixture roots are separate from raw profiles that outlive fixture
cleanup. Other declared roots are the checkout Git directory, Cargo home,
Rustup home and RUNNER_TEMP when available. Root labels and device IDs connect
the measurements without dumping environment values. These fixed roots are
not a whole-filesystem inventory and do not discover arbitrary Cargo config
overrides or every fixture-specific output directory.

Allocated bytes use filesystem blocks and count each device/inode once across
the sample, assigning shared allocation to its first inspected bucket.
Logical file bytes count regular-file directory entries, including hard links;
duplicate-inode counts explain that distinction. Capacity reports available
bytes and inodes from fstatvfs on each observed device, with checked block-size
multiplication; available blocks exclude space reserved from ordinary users.
The eight largest inspected raw-profile entries retain only bounded names
and metadata, never profile contents or producer attribution.

Each sample inspects at most 100,000 directory entries, descends at most
32 levels, and checks a five-second elapsed budget before processing entries.
Capacity uses a safe query on a held directory, at most once per observed device; it
spawns no process. Filesystem operations are not individually preempted.
Directory descriptors and no-follow opens prevent following filesystem
symlinks; the trusted proc descriptor namespace anchors traversal.
Missing roots, detected races, errors, overflow and exhausted limits make the
sample partial. Counts then describe inspected entries rather than totals,
and an absent bucket is not a measured zero. All samples are non-atomic observations
while the workload continues writing.

The observer creates no diagnostic files, removes nothing and changes no
profile pattern, test selection, threshold or runner capacity. It supplies
storage evidence for #4978; it does not establish the dominant producer or
repair the capacity failure by itself. The existing failure-profile adapter
and LCOV upload remain in place.

### Coverage build debug information

Only the Generate coverage step sets `CARGO_PROFILE_TEST_STRIP=debuginfo`.
Ordinary builds, the plain spawned-binary prebuild and the diagnostic preflight
retain their normal configuration. The full workspace/all-features command,
profile pattern, test population and strict report generation stay unchanged.
This setting removes linked debug information; it preserves ordinary symbols.
Coverage binaries therefore lose debugger line information even though their
coverage source mappings must remain usable.

This is a bounded response to measured build pressure, not an attribution of
the final ENOSPC write. In diagnostic run
[42015956653](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/42015956653),
the last complete sample recorded 57,278,578,688 allocated bytes in
`instrumented_build` and 93,016,064 in `raw_profiles`. The log ended before the worker's
terminal ENOSPC failure, so subsequent growth remains unobserved. The bucket
does not identify the share occupied by debug information. A smaller fixture
does not establish a particular workspace reduction or sufficient capacity;
acceptance still requires a complete native coverage run, its test results and
LCOV artifact, with the same population and comparable storage measurements.

The mandatory diagnostics preflight includes
`native_test_profile_strip_preserves_coverage_and_reduces_fixture_size`. It
builds the same dependency-free Cargo test fixture with explicit `none` and
`debuginfo` strip modes, separate target directories and identical explicit
coverage instrumentation. Child-only compiler/wrapper/encoded-flag overrides
prevent the outer cargo-llvm-cov environment from changing either comparison.
The native compiler invocation must show the expected coverage/debug flags and
candidate strip setting. The baseline binary must contain line tables; the
candidate must contain no debug sections and be smaller.

Each variant runs the exact test twice, with a post-assertion acknowledgement,
one-test success and a fresh nonempty profile per invocation. Matching-sysroot
LLVM tools must merge both profiles without warnings, match each merged
profile's ID to its own executable, and export usable JSON and LCOV. The
control requires identical owned source regions and line counts, the known
function count of two, a hot return count of two and a cold return count of
zero. IDs may differ between the two binaries. Tool absence, malformed output,
zero selection, warning-only partial merging and missing source evidence fail
the Linux control. Existing malformed-profile and size-bound controls remain
selected by the same preflight.

The finite native commands use the observer's non-reaping process owner,
30-second per-command deadlines and 256 KiB per-stream capture limits. Fixture
outputs and captures stay under the owned root; the commands preserve the
parent environment and clean their owned groups before reaping. A compact
native receipt records actual tool versions, sizes, IDs and expected counts.
These bounds apply to the control; they do not shorten the coverage workload's 45-minute limit.
Before explicit success cleanup, the native fixture retains and reports its
root on an error or panic, including uncertain child cleanup. Removal follows
successful commands and assertions, before the passed receipt is emitted.

The pinned interface basis is Rust 1.95.0
[`strip` codegen documentation](https://github.com/rust-lang/rust/blob/59807616e1fa2540724bfbac14d7976d7e4a3860/src/doc/rustc/src/codegen-options/index.md#strip),
its Cargo revision's
[profiles](https://github.com/rust-lang/cargo/blob/f2d3ce0bd7f24a49f8f72d9000448f8838c4e850/src/doc/src/reference/profiles.md#strip)
and [profile environment variables](https://github.com/rust-lang/cargo/blob/f2d3ce0bd7f24a49f8f72d9000448f8838c4e850/src/doc/src/reference/environment-variables.md),
and [cargo-llvm-cov 0.9.1](https://github.com/taiki-e/cargo-llvm-cov/blob/v0.9.1/src/main.rs).
Native execution, including the matching LLVM 22.1.2 tools on the pinned
workflow, remains the compatibility authority.

### Validate locally

On Linux, before adopting a newer Rust toolchain, run the small diagnostics package with
both the repository pin and stable. Install matching LLVM tools for each
selected compiler; a system `llvm-profdata` from another LLVM release is not a
substitute for the tool in that compiler's sysroot.

<!-- ripr:coverage-upgrade:start -->
```bash
(
    set -e
    rustup toolchain install 1.95.0 --profile minimal --component clippy --component llvm-tools-preview
    rustup update stable
    rustup component add --toolchain stable clippy llvm-tools-preview

    ripr_cargo_for() (
        if (( $# < 2 )); then
            printf '%s\n' 'usage: ripr_cargo_for TOOLCHAIN CARGO_ARGUMENTS...' >&2
            exit 2
        fi
        ripr_channel=$1
        shift
        export RUSTUP_AUTO_INSTALL=0
        ripr_rustc=$(command rustup which --toolchain "$ripr_channel" rustc) || exit $?
        ripr_cargo=$(command rustup which --toolchain "$ripr_channel" cargo) || exit $?
        if [[ $ripr_rustc != /* || $ripr_cargo != /* || ! -x $ripr_rustc || ! -x $ripr_cargo ]]; then
            printf '%s\n' 'The selected toolchain must provide executable absolute rustc and cargo paths.' >&2
            exit 1
        fi
        if [[ $1 == clippy ]]; then
            ripr_clippy=$(command rustup which --toolchain "$ripr_channel" cargo-clippy) || exit $?
            if [[ $ripr_clippy != "${ripr_rustc%/*}/cargo-clippy" || ! -x $ripr_clippy ]]; then
                printf '%s\n' 'The selected compiler directory must provide executable cargo-clippy.' >&2
                exit 1
            fi
        fi
        ripr_identity=$("$ripr_rustc" -vV) || exit $?
        ripr_sysroot=$("$ripr_rustc" --print sysroot) || exit $?
        ripr_host=$(printf '%s\n' "$ripr_identity" | sed -n 's/^host: //p') || exit $?
        if [[ $ripr_sysroot != /* || -z $ripr_host ]]; then
            printf '%s\n' 'The selected rustc did not identify its sysroot and host.' >&2
            exit 1
        fi
        ripr_llvm_bin=$ripr_sysroot/lib/rustlib/$ripr_host/bin
        export RUSTUP_TOOLCHAIN="$ripr_channel" RUSTC="$ripr_rustc" \
            PATH="${ripr_rustc%/*}:$ripr_llvm_bin:$PATH" \
            LLVM_COV="$ripr_llvm_bin/llvm-cov" LLVM_PROFDATA="$ripr_llvm_bin/llvm-profdata"
        printf 'Selected toolchain: %s\n%s\n' "$ripr_channel" "$ripr_identity"
        "$LLVM_COV" --version >/dev/null || exit $?
        "$LLVM_PROFDATA" --version || exit $?
        "$ripr_cargo" "$@" || exit $?
        printf -v ripr_command ' %q' "$@"
        printf 'Completed: %s cargo%s\n' "$ripr_channel" "$ripr_command"
    )

    ripr_cargo_for 1.95.0 test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture
    ripr_cargo_for stable test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture
    ripr_cargo_for stable test --locked -p repo-policy --test frontdoor
    ripr_cargo_for stable clippy --locked -p repo-policy --all-targets -- -D warnings
)
```
<!-- ripr:coverage-upgrade:end -->

The explicit pin installation also works when activation disabled automatic
installation. `rustup update stable` deliberately refreshes the stable channel;
adding components alone does not establish that its compiler is current. Retain
the compiler and LLVM identities and the test's reported raw version/layout with
the results.
The subshell resolves the requested compiler and Cargo with `rustup which`, invokes
Cargo by its absolute path, and selects the same compiler for both `RUSTC` and
literal `rustc` subprocesses through `PATH`. The LLVM paths come from that compiler's
sysroot. This supports a shell that already activated either the pin or stable,
including activation that prepends SDK binaries and exports an absolute `RUSTC`.
Each command leaves the caller's activation unchanged. Missing tools and failed
version probes stop before Cargo runs; Cargo failures keep their original status
and stop the remaining checks. A `Completed:` line follows each successful Cargo
command; a missing check cannot be inferred from another command's success.

`RUSTUP_TOOLCHAIN` also follows subprocesses into temporary fixture repositories,
where a directory-local override alone does not apply. Setting it alone does not
replace an explicit `RUSTC` or a direct SDK binary already ahead of the Rustup
proxies on `PATH`; see [Rustup toolchain overrides](https://rust-lang.github.io/rustup/overrides.html)
and [Cargo's environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html).
Selecting these together keeps the policy helper's build-time and runtime compiler
identities aligned. The checks retain their compiler-identity refusals.

The `stable-repo-policy` job in the existing advisory Future Clippy workflow
executes this marked recipe with genuine pin/stable toolchains and matching
`llvm-tools-preview`, starting with the stable SDK first on `PATH` and an absolute
`RUSTC`. It first runs the workflow controls, then requires all four ordered
completion records, nonempty successful test runs, genuine LLVM association
observations for both diagnostics checks, and successful Clippy completion.
Missing, ambiguous or malformed recipe boundaries, partial execution and stale
run evidence fail the job. Native compiler, LLVM and profile observations stay
in the job output. This route replaces the duplicate stable Cargo commands. It
follows that workflow's existing main-push, manual-dispatch and
`clippy-future`/`full-ci` plus `merge-ready` PR routing. A failure stays visible;
the job does not become a required merge gate or run full workspace coverage.
The existing future-Clippy job continues to inspect deferred lints on Rust
1.95.0. This stable compatibility check does not change the repository pin or
MSRV.

To generate coverage and inspect artifacts locally:

```bash
cargo build -p ripr
cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info
ls -lh lcov.info
```

Build `ripr` first. Tests that spawn the binary look for a pre-built
`target/debug/ripr`, and `cargo llvm-cov` builds into its own target directory,
so from a clean checkout those tests fail without it.

To view the LCOV report in a browser (if you have `genhtml` installed):

```bash
genhtml lcov.info -o coverage-report/
open coverage-report/index.html
```

## Future calibration

Threshold ratcheting should follow the strategy documented in [IMPLEMENTATION_CAMPAIGNS.md](../IMPLEMENTATION_CAMPAIGNS.md). Allow real data to accumulate on `main` before raising targets or making Codecov status blocking for branch protection.
