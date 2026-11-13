# Release Checklist

This checklist is the canonical release procedure for ripr. It is intentionally
more conservative than the ordinary development loop: a release must preserve
source, artifact, metadata, and publication identity across every supported
surface.

## Release authority

The release owner should start from the current release-control issue and its
accepted candidate. Do not construct a release directly from an arbitrary local
worktree, an unreviewed branch, or an artifact whose source SHA is not retained.

Before beginning:

- confirm the release version and candidate SHA;
- confirm the candidate is reachable from the protected repository state used
  by the release process;
- confirm required hosted and platform qualification is terminal on that exact
  source;
- retain the release receipts and package/archive identities used for the
  decision.

## Version and public metadata

Verify all public version declarations and package metadata before running the
release gates.

- The version in the root `Cargo.toml` `[workspace.package]` is correct. This is
  the single release-version source; `crates/ripr/Cargo.toml` inherits it with
  `version.workspace = true` and declares no version of its own (#2711).
- `editors/vscode/package.json` and its `package-lock.json` match that version.
  `cargo xtask release-readiness` checks this as `extension-version-match`; a
  mismatch fails the marketplace publish (#1283). The check compares all four
  declarations — `[workspace.package] version`, `package.json`, and both places
  `package-lock.json` records the version — against the `--version` you pass, so
  running it before the bump lands is a hard fail rather than a pass.
- For the defaults-first public install line, the version is newer than
  `0.3.0`; `0.3.0` predates `ripr pilot` and `ripr outcome`.
- The root workspace uses Rust edition `2024`.
- The root workspace `rust-version` is `1.95`.
- The build/release compiler in `rust-toolchain.toml` is `1.100.0`; compiler
  identity and matching LLVM evidence follow [the upgrade guide](ci/rust-1.100-upgrade.md).
- Cargo package `repository` points at `https://github.com/EffortlessMetrics/ripr/`;
  no duplicate Cargo `homepage` field is emitted.
- The README says `ripr` is alpha software and does not claim mutation execution.

## Local Gates

Run from the repository root:

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo xtask precommit
```

The commands above are local evidence only. A release still requires the
protected hosted and platform routes identified by the release-control issue.
Do not treat an omitted, timed-out, or infrastructure-failed lane as a passing
product result.

## Package inspection

Inspect the exact package before publication:

```bash
cargo package -p ripr --locked --allow-dirty
cargo package -p ripr --list
cargo publish -p ripr --dry-run --locked
```

The release workflow must bind the generated `.crate`, its extracted tree, and
its lockfile to the accepted source candidate. Cargo 1.100 uses a packaged
lockfile by default during `cargo install`; `--locked` remains explicit intent
and compatibility for older Cargo clients, not a substitute for package-custody
proof. Validate the actual packaged archive rather than a separately rebuilt
workspace directory.

Check that:

- the package contains the intended source, license, README, tests, fixtures,
  and sample workspace inputs;
- generated build output, caches, local receipts, worktrees, and credentials are
  absent;
- the package version and source identity match the accepted release candidate;
- package metadata names the canonical repository without a redundant
  `homepage` field;
- an extracted package can be built and installed through the release rehearsal
  without network-dependent source drift.

## Binary and editor artifacts

Qualify every release-shaped artifact from the same accepted source:

- server archives for each supported Linux, macOS, and Windows target;
- checksums and extraction layout;
- native payload identity and target architecture;
- Linux glibc floor where applicable;
- npm launcher metadata and resolution behavior;
- VS Code extension package and managed-server contract;
- Zed extension `wasm32-wasip2` build;
- Python wheel metadata, RECORD, payload, and independent installed-consumer
  behavior when that distribution is included in the release.

Do not substitute a host-built binary for a target artifact or infer one
platform's result from another.

## Release qualification

The release-control issue defines the authoritative qualification set. At a
minimum, preserve evidence for:

- exact Rust build compiler and independent source-MSRV proof;
- strict Clippy, formatting, tests, doctests, golden and fixture contracts;
- coverage generation with matching LLVM tools;
- Windows and macOS terminal outcomes;
- formal-verification/Kani outcome under its own pinned compiler identity;
- package and archive rehearsals;
- source, package, and artifact identities tied to the candidate SHA.

Historical receipts remain historical. Never relabel an older compiler,
candidate, or artifact as current proof.

## Publication

Publish only after the release-control authority admits the exact candidate and
all required evidence is terminal.

Typical publication commands are:

```bash
cargo publish -p ripr --locked
```

Other channels use their dedicated protected workflows. Do not publish from a
local replacement artifact when a protected workflow owns that channel.

After publication:

- verify crates.io metadata and install behavior;
- verify GitHub Release archives and checksums;
- verify npm/editor manifests resolve the matching server release;
- verify public documentation and changelog point at the released version;
- retain the terminal release receipt and exact source/artifact identities.

## Rollback and partial publication

If any publication step fails, stop and record exactly which immutable assets
were published. Do not overwrite or silently replace a published crate or
archive. Follow the release-control issue's recovery procedure, using a new
version when ecosystem immutability requires one.

A release is not complete merely because one channel succeeded. Public status
must distinguish complete, partial, failed, and not-yet-attempted channels.