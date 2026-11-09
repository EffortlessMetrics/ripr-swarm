# Cargo build-artifact lifecycle

RIPR gives every checkout one explicit Cargo ownership boundary: its own `target/` directory. We do not share a mutable target or build directory across worktrees, and we do not use age as a proxy for whether an unknown workspace is live.

| Class | Location | Lifecycle |
|---|---|---|
| Cargo intermediates and final outputs | the checkout's exact `target/` | Private to that checkout; removed at checkout disposal |
| RIPR cache, reports, receipts, and retained evidence | `target/ripr/` | Preserved by Cargo-artifact cleanup and managed by RIPR contracts |
| CI-owned Cargo state | the job's explicit `CARGO_TARGET_DIR` | Owned, cached, and deleted by that job's existing lifecycle |

`.cargo/config.toml` disables incremental compilation by default. Agent worktrees are normally short-lived, so retaining another object graph in every checkout creates large low-reuse directories. A deliberate long-lived checkout can opt in for one command or session with `CARGO_INCREMENTAL=1`; that does not make the target shareable.

The forced `TEMP`, `TMP`, and `TMPDIR` redirect remains intentional. It keeps linker and test scratch off the Windows system drive and inside the checkout's disposal boundary.

## Why there is no global TTL collector

Cargo can build nested fixture workspaces and caller-selected manifests that are not registered Git worktrees. A pool-wide age rule cannot prove those trees are inactive. The repository therefore does not delete compiler state belonging to another manifest, checkout, or host-level target owner.

Cleanup is an explicit terminal operation for the current checkout. The caller stops Cargo work in that checkout, reviews the exact direct children selected under its resolved `target/`, and then applies deletion. This is an ownership decision, not a liveness guess.

## Build the cleanup utility

The utility is a dependency-free standalone Rust source set. Compile it into the retained `target/ripr` tree so it remains executable while `target/debug`, `target/release`, and leaked scratch are removed.

### Bash

```bash
mkdir -p target/ripr/tools
rustc --edition=2024 tools/cargo-artifacts/main.rs \
  -o target/ripr/tools/cargo-artifacts
```

### PowerShell

```powershell
New-Item -ItemType Directory -Force target/ripr/tools | Out-Null
rustc --edition=2024 tools/cargo-artifacts/main.rs `
  -o target/ripr/tools/cargo-artifacts.exe
```

The sources use only the Rust standard library. They can be compiled before the workspace itself is healthy and do not allocate another Cargo target.

## Commands

From any directory inside the repository:

```bash
target/ripr/tools/cargo-artifacts report
target/ripr/tools/cargo-artifacts clean-current
target/ripr/tools/cargo-artifacts clean-current --apply
```

On Windows, use `target/ripr/tools/cargo-artifacts.exe`.

Every cleanup is a dry run unless `--apply` is present. The default soft budget is 12 GB of removable current-checkout state; override the reporting threshold with `--max-size-gb <n>`. The budget does not authorize broader deletion.

The utility writes:

- `target/ripr/reports/cargo-artifacts.json`
- `target/ripr/reports/cargo-artifacts.md`

`--no-receipt` is available only for isolated dry-run diagnostics and tests. Destructive cleanup always retains a receipt.

### `report`

`report` resolves Cargo's current metadata offline and inventories the current checkout's exact `target/`. It reports removable bytes, retained `target/ripr` bytes, file counts, and soft-budget pressure. It does not delete.

### `clean-current`

`clean-current` selects every direct entry of the current checkout's exact `target/` except `target/ripr/`. This includes Cargo profiles, fingerprints, incremental state left by earlier configurations, and top-level test/linker scratch.

Before selecting or deleting, it refuses:

- non-empty `CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET_DIR`, or `CARGO_BUILD_BUILD_DIR` overrides;
- a Cargo `workspace_root` other than the current repository;
- a `target_directory` other than `<repository>/target`;
- a `build_directory` that is not exactly the same path as that target;
- symlinks, Windows reparse points, special files, unreadable paths, and non-direct deletion candidates.

The exact-equality rule matters: containment is not ownership. A nested Cargo build path never authorizes deletion of its ancestor hash tree or siblings.

Apply mode writes the initial receipt before mutation, rechecks each direct child immediately before removal, and rewrites the receipt after every successful deletion. A lock, race, or filesystem error stops cleanup, returns nonzero, and records the exact partial result and remaining failing path without a traceback.

## Agent lifecycle

1. **Start work normally.** Each checkout gets its own `target/`; never point sibling worktrees at one mutable `CARGO_TARGET_DIR`.
2. **Inspect pressure when it matters.** Compile the utility once and run `report` before a build-heavy campaign when disk state is unknown or constrained.
3. **Keep evidence separate.** `target/ripr` is not disposable compiler state. Use RIPR's own cache/report lifecycle for it.
4. **Dispose deliberately.** Stop Cargo processes in the checkout, run `clean-current`, inspect the direct-child selection, then rerun with `--apply` immediately before removing or retiring the checkout.
5. **Retain the receipt.** It records selected, deleted, preserved, failed, and over-budget state.

Do not substitute wildcard deletion, an unkeyed shared target, or undifferentiated `cargo clean`. `cargo clean` owns Cargo's target as a whole; this repository deliberately retains a different evidence class under `target/ripr`.

## CI ownership

A CI workflow that sets `CARGO_TARGET_DIR` continues to own all Cargo intermediates and final outputs under that path. Because the repository does not set a separate `build-dir`, existing cache and terminal-cleanup boundaries remain complete. The checkout cleanup utility refuses those overrides rather than deleting caller-owned CI or qualification state.

## Existing split-pool remnants

An earlier draft of this change used a sibling `.ripr-swarm-build` pool. The final design does not own or delete that path. On a host that exercised the draft, remove such a pool only after independently confirming that no Cargo process or checkout still references it.

## Verification

The standalone source set carries its own standard-library unit tests. Repository CI compiles and runs them directly with `rustc --test` through:

```bash
cargo test -p xtask --test cargo_artifact_lifecycle
```

The suite covers Windows JSON paths, exact target/build ownership, direct-child selection, budget reporting, `target/ripr` preservation, destructive receipt requirements, partial-failure receipts, and symlink non-traversal.
