# Cargo build-artifact lifecycle

RIPR separates compiler intermediates, final Cargo outputs, and product evidence because they have different reuse and retention rules.

| Class | Location | Lifecycle |
|---|---|---|
| Cargo intermediates (`deps`, incremental state, build-script output) | `<checkout-parent>/.ripr-swarm-build/{workspace-path-hash}` | Private to one manifest; bounded across sibling checkouts |
| Final Cargo outputs (`ripr`, `xtask`, docs, timings) | the active workspace's `target/` | Small, familiar output surface; disposable with the checkout |
| RIPR cache, reports, receipts, and retained evidence | `target/ripr/` | Managed by RIPR's cache/report contracts, not Cargo cleanup |

`.cargo/config.toml` uses Cargo's stable `build.build-dir` split. Its relative path is resolved from the repository config, not from whichever nested manifest Cargo is building. Root builds and fixture workspaces therefore converge on one pool beside the checkout instead of scattering hidden build trees through `fixtures/`. `{workspace-path-hash}` keeps every root, fixture, and sibling-worktree manifest in a private mutable tree.

The pool stays on the checkout volume. It does not redirect large builds into `CARGO_HOME` or the Windows system drive. Final binaries remain at established paths such as `target/debug/ripr`.

The existing forced `TEMP`, `TMP`, and `TMPDIR` redirect remains intentional. It keeps linker and test scratch off the Windows system drive. `clean-current` removes leaked top-level scratch together with the checkout's final Cargo output while preserving `target/ripr`.

## Commands

Run from any directory inside the repository.

```bash
python3 tools/python/cargo-artifacts/run.py report
python3 tools/python/cargo-artifacts/run.py gc
python3 tools/python/cargo-artifacts/run.py gc --apply
python3 tools/python/cargo-artifacts/run.py clean-current
python3 tools/python/cargo-artifacts/run.py clean-current --apply
```

On Windows, use the same arguments with `py -3`:

```powershell
py -3 tools/python/cargo-artifacts/run.py report
py -3 tools/python/cargo-artifacts/run.py clean-current --apply
```

Every command is a dry run unless `--apply` is present. The command prints Markdown and writes JSON and Markdown receipts to:

- `target/ripr/reports/cargo-artifacts.json`
- `target/ripr/reports/cargo-artifacts.md`

Use `--no-receipt` only for an isolated diagnostic or unit test.

The repo-owned lifecycle refuses active `CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET_DIR`, or `CARGO_BUILD_BUILD_DIR` overrides. An override changes ownership and retention, so the tool will not infer that the resulting path is safe to delete.

### `report`

Inventories every recognized Cargo workspace-hash tree under the pool, measures bytes and file counts, resolves the current workspace through `cargo metadata`, and shows what the default GC policy would select. It also reports unexpected files, links, or directory shapes without traversing them.

For stale GC, the tool protects:

- the current workspace build tree;
- root build trees for registered Git worktrees beside the current checkout; and
- root build trees for other sibling checkouts whose Cargo config names the same pool.

If a registered sibling is missing or cannot produce trustworthy Cargo metadata, inspection fails closed. Repair or prune that worktree before applying GC.

### `gc`

Selects inactive build trees using both controls:

- **TTL:** 7 days by default.
- **Pool budget:** 12 GB by default.

Budget eviction considers oldest trees first but will not evict a tree newer than 24 hours. Current/live trees and trees containing a symlink, junction/reparse point, unreadable entry, or special file are ineligible. Apply mode also refuses any unrecognized pool entry and re-resolves live worktrees immediately before deletion.

If protected or young trees keep the pool above budget, the receipt reports the residual pressure instead of widening deletion.

Override the bounds explicitly when a host needs a different policy:

```bash
python3 tools/python/cargo-artifacts/run.py gc \
  --ttl-days 3 --max-size-gb 8 --min-age-hours 24
```

Review the receipt, then repeat the same command with `--apply`.

`--root` exists for read-only recovery and tests. Apply mode refuses an override pool.

### `clean-current`

Use immediately before disposing of an agent checkout. It selects:

1. the exact root-workspace `build_directory` returned by Cargo; and
2. every direct entry in the current checkout's `target/` except `target/ripr/`.

It refuses external target directories, active Cargo path overrides, links/reparse points, unreadable paths, special files, and a build directory outside the configured sibling pool. After deletion, the cleanup receipt is written back under the preserved `target/ripr` tree.

Nested fixture build trees are reclaimed by the pool TTL/budget policy. Cargo hashes their manifest paths but does not expose a reversible manifest-to-hash index, so `clean-current` does not guess ownership from a hash.

## Agent lifecycle

1. **Before a build-heavy campaign:** run `report` when disk state is unknown or constrained.
2. **During work:** build normally. Do not point multiple worktrees at one mutable `CARGO_TARGET_DIR` or one unkeyed `CARGO_BUILD_BUILD_DIR`.
3. **After ordinary work:** run `gc` when the pool exceeds its budget or stale manifests have accumulated.
4. **Before deleting a checkout:** run `clean-current`, inspect the exact selection, then rerun with `--apply`.
5. **Retain the receipt:** it states what was selected, deleted, preserved, refused, and left over budget.

Do not substitute `cargo clean` for this lifecycle. Cargo owns both the final target and split build directory, while RIPR owns retained evidence under `target/ripr`; undifferentiated cleanup crosses those retention classes.

## CI ownership

CI jobs that already allocate and clean a private target or scratch directory should set `CARGO_BUILD_BUILD_DIR` to that same owned path. This keeps compiler intermediates inside the job's existing cache and terminal cleanup boundary rather than using the developer sibling pool. Hosted jobs using `Swatinem/rust-cache` must cache the effective build directory, not only final target artifacts.

The developer pool is the fallback for ordinary local and agent Cargo commands. CI-specific overrides are valid because the CI job, not this tool, owns their deletion lifecycle.

## Migration from the old layout

Existing checkouts can still contain the former `target/debug/deps`, `target/debug/incremental`, build-script output, and leaked test scratch. After the configuration change is read by the pinned toolchain:

1. run `clean-current` in each checkout;
2. confirm that `target/ripr` is the only retained target entry that matters;
3. apply cleanup before deleting or retiring that checkout; and
4. run bounded pool GC after old fixture/build hashes age out.

The next build writes large intermediates to the same-volume pool and recreates only final outputs and active scratch inside the checkout.

## Verification

The lifecycle tool uses only the Python standard library. Its unit suite is:

```bash
python3 -m py_compile tools/python/cargo-artifacts/run.py tools/python/cargo-artifacts/test_run.py
python3 tools/python/cargo-artifacts/test_run.py -v
```

The tests cover config-root pool placement, NUL-safe worktree parsing, current/live tree protection, TTL and budget selection, minimum-age refusal, `target/ripr` preservation, override and out-of-root refusal, unexpected pool entries, safe receipt paths, and symlink non-traversal.

A config-bearing candidate also needs hosted Rust qualification showing that:

- `cargo metadata` resolves intermediates below `.ripr-swarm-build/{workspace-path-hash}` outside CI-owned overrides;
- final binaries remain at `target/debug/...`; and
- CI-owned target/scratch paths still contain and clean their own intermediates.
