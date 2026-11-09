# Cargo build-artifact lifecycle

RIPR separates compiler intermediates, final Cargo outputs, and product evidence because they have different reuse and retention rules.

| Class | Location | Lifecycle |
|---|---|---|
| Cargo intermediates (`deps`, incremental state, build-script output) | `${CARGO_HOME:-~/.cargo}/ripr-swarm-build/{workspace-path-hash}` | Private to one checkout; bounded by age and size |
| Final Cargo outputs (`ripr`, `xtask`, docs, timings) | the checkout's `target/` | Disposable with the checkout |
| RIPR cache, reports, receipts, and retained evidence | `target/ripr/` | Managed by RIPR's cache/report contracts, not Cargo cleanup |

`.cargo/config.toml` uses Cargo's stable `build.build-dir` split. `{workspace-path-hash}` is required: it keeps the large trees under one managed root without allowing two worktrees to mutate the same build tree. Final binaries remain at established paths such as `target/debug/ripr`.

The existing forced `TEMP`, `TMP`, and `TMPDIR` redirect remains intentional. It keeps build and test scratch off the Windows system drive. `clean-current` removes leaked top-level scratch together with the checkout's Cargo output while preserving `target/ripr`.

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

Every command is a dry run unless `--apply` is present. The command prints Markdown and writes both Markdown and JSON receipts to:

- `target/ripr/reports/cargo-artifacts.md`
- `target/ripr/reports/cargo-artifacts.json`

Use `--no-receipt` only for an isolated diagnostic or unit test.

### `report`

Inventories every recognized worktree-hash tree under the managed root. It identifies the current checkout, measures bytes and file counts, and shows what the default GC policy would select. It does not delete.

### `gc`

Selects inactive build trees using both controls:

- **TTL:** 14 days by default.
- **Budget:** 20 GB for the repo-scoped managed root by default.

Budget eviction considers oldest trees first but will not evict a tree newer than 24 hours. The current checkout and any tree containing a symlink, junction/reparse point, unreadable entry, or special file are ineligible. If protected or young trees keep the root above budget, the receipt reports the residual pressure instead of widening deletion.

Override the bounds explicitly when the host needs a different policy:

```bash
python3 tools/python/cargo-artifacts/run.py gc \
  --ttl-days 7 --max-size-gb 12 --min-age-hours 24
```

Review the receipt, then repeat the same command with `--apply`.

### `clean-current`

Use immediately before disposing of an agent worktree. It selects:

1. the exact `build_directory` returned by Cargo for the current checkout; and
2. every direct entry in the current checkout's `target/` except `target/ripr/`.

It refuses an external or overridden `CARGO_TARGET_DIR`, because deleting a caller-owned shared path is not a safe worktree cleanup. It also refuses any linked, reparse, unreadable, or special path rather than following it.

## Agent lifecycle

1. **Before a build-heavy campaign:** run `report` when disk state is unknown or constrained.
2. **During work:** build normally. Do not point multiple worktrees at one mutable `CARGO_TARGET_DIR` or one unkeyed `CARGO_BUILD_BUILD_DIR`.
3. **After ordinary work:** run `gc` when the managed root exceeds its budget or stale worktrees have accumulated.
4. **Before deleting a worktree:** run `clean-current`, inspect the exact selection, then rerun with `--apply`.
5. **Retain the receipt:** it states what was selected, what was deleted, what was preserved, and any unresolved pressure.

Do not substitute `cargo clean` for this lifecycle. With repository-owned evidence under `target/ripr`, an undifferentiated target deletion erases a different retention class.

## Migration from the old layout

Existing worktrees can still contain the former `target/debug/deps`, `target/debug/incremental`, build-script output, and leaked test scratch. After the configuration change has been read by the pinned toolchain:

1. run `clean-current` in each worktree;
2. inspect that `target/ripr` is the only retained target entry that matters;
3. apply the cleanup before deleting or retiring that worktree.

The next build writes large intermediates to the managed Cargo-home root and recreates only the final outputs needed in the checkout.

## Verification

The lifecycle tool uses only the Python standard library. Its unit suite is:

```bash
python3 tools/python/cargo-artifacts/test_run.py -v
```

The tests cover hash-leaf recognition, current-tree protection, TTL and budget selection, minimum-age refusal, `target/ripr` preservation, out-of-root deletion refusal, and symlink non-traversal.
