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

To generate coverage and inspect artifacts locally, run the same supported
recipe as `.github/workflows/coverage.yml`: export the instrumentation
environment first, clean after that environment, run a plain instrumented
build and plain test run, then report. The explicit absolute
`RIPR_TEST_BINARY` binds the analyzer-consuming xtask tests to the actual
instrumented executable; a configured coverage target that does not name the
instrumented analyzer fails closed instead of substituting an ordinary
binary. `report` takes no build flags and, at this virtual workspace root,
covers all members even without `--workspace`.

```bash
set -euo pipefail
llvm_cov_env="$(cargo llvm-cov show-env --export-prefix)"
eval "$llvm_cov_env"
cargo llvm-cov clean --workspace
cargo build --workspace --all-features
export RIPR_TEST_BINARY="$CARGO_LLVM_COV_TARGET_DIR/debug/ripr"
test -f "$RIPR_TEST_BINARY"
"$RIPR_TEST_BINARY" --version
sha256sum "$RIPR_TEST_BINARY"
cargo test --workspace --all-features --tests
cargo llvm-cov report --lcov --output-path lcov.info
ls -lh lcov.info
```

The show-env output is captured in a standalone assignment before `eval`
so a producer failure propagates under `set -e` instead of being masked
by `eval`.

To view the LCOV report in a browser (if you have `genhtml` installed):

```bash
genhtml lcov.info -o coverage-report/
open coverage-report/index.html
```

## Future calibration

Threshold ratcheting should follow the strategy documented in [IMPLEMENTATION_CAMPAIGNS.md](../IMPLEMENTATION_CAMPAIGNS.md). Allow real data to accumulate on `main` before raising targets or making Codecov status blocking for branch protection.
