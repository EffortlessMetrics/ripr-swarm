---
name: manage-build-artifacts
description: Inspect and reclaim RIPR Cargo compiler artifacts before or after build-heavy agent work, during disk pressure, and before disposing of a checkout.
---

# Useful result

Compiler intermediates remain private per manifest but live in one bounded, same-volume pool beside sibling checkouts. Current and live sibling root builds, `target/ripr` evidence, external targets, and ambiguous filesystem objects are preserved. Cleanup produces a receipt instead of an unobservable `cargo clean`.

# Read first

Read [the build-artifact lifecycle](../../../docs/BUILD_ARTIFACTS.md), `.cargo/config.toml`, and the current checkout's actual `cargo metadata`. Treat prior disk summaries as leads; inventory the live pool before claiming pressure or reclaimed bytes.

# Operating procedure

1. Run `python3 tools/python/cargo-artifacts/run.py report` (or `py -3` on Windows) when entering a build-heavy campaign with unknown disk state.
2. Build normally. Preserve `{workspace-path-hash}` isolation. Do not share one mutable `CARGO_TARGET_DIR` or an unkeyed `CARGO_BUILD_BUILD_DIR` across worktrees.
3. For stale state, run `gc` without `--apply`. Review the selected hash trees, current/live protections, age and size limits, pool anomalies, and residual pressure. Repeat the same bounded command with `--apply` only when the plan is correct.
4. Before disposing of the current checkout, run `clean-current` without `--apply`. Confirm that it names the current root build tree and direct `target/` entries while preserving `target/ripr`; then repeat with `--apply`.
5. Retain `target/ripr/reports/cargo-artifacts.{json,md}` with the handoff when build-disk state matters.

# Safety boundary

- Deletion is never implicit; `--apply` is mandatory.
- The current workspace and live sibling root-workspace trees are ineligible for stale GC.
- Budget GC will not select a tree newer than the minimum age.
- Apply mode rechecks live worktrees and refuses unexpected pool entries.
- Symlinks, Windows reparse points, unreadable entries, special files, active Cargo artifact overrides, and override pools fail closed.
- `clean-current` refuses external target directories.
- `target/ripr` is a separate evidence/cache retention class and is preserved by current-checkout cleanup.
- The forced repository temp redirect protects the Windows system drive and remains in force.
- CI may override `CARGO_BUILD_BUILD_DIR` only when the job already owns and cleans the resulting private target/scratch path.

Do not use `cargo clean`, wildcard deletion, a shared global mutable tree, or manual traversal of Cargo's hash paths as substitutes. If the receipt remains over budget because protected or young trees dominate, report that pressure; do not weaken the bounds silently.

# Verification

Run:

```bash
python3 -m py_compile tools/python/cargo-artifacts/run.py tools/python/cargo-artifacts/test_run.py
python3 tools/python/cargo-artifacts/test_run.py -v
```

For a config-bearing candidate, hosted Rust qualification must also establish that ordinary final binaries remain at `target/debug/...`, local intermediates resolve below the same-volume sibling pool, and CI-owned overrides retain their existing cache and cleanup boundary.
