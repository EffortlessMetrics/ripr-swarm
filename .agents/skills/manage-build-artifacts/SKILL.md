---
name: manage-build-artifacts
description: Inspect and reclaim RIPR Cargo compiler artifacts before or after build-heavy agent work, during disk pressure, and before disposing of a worktree.
---

# Useful result

Compiler intermediates remain private to each checkout but live under one bounded, repo-scoped Cargo-home root. The current worktree, `target/ripr` evidence, external targets, and ambiguous filesystem objects are preserved. Cleanup produces a receipt rather than an unobservable `cargo clean`.

# Read first

Read [the build-artifact lifecycle](../../../docs/BUILD_ARTIFACTS.md), `.cargo/config.toml`, and the current worktree's actual `cargo metadata`. Treat prior disk summaries as leads; inventory the live root before claiming pressure or reclaimed bytes.

# Operating procedure

1. Run `python3 tools/python/cargo-artifacts/run.py report` (or `py -3` on Windows) when entering a build-heavy campaign with unknown disk state.
2. Build normally. Keep the configured `{workspace-path-hash}` isolation. Do not share one mutable `CARGO_TARGET_DIR` or an unkeyed `CARGO_BUILD_BUILD_DIR` across worktrees.
3. For host-wide stale state, run `gc` without `--apply`, review the selected hash trees, limits, protected current tree, unsafe paths, and residual pressure, then repeat the exact bounded command with `--apply` when the plan is correct.
4. Before disposing of the current worktree, run `clean-current` without `--apply`. Confirm that it names the current Cargo build tree and direct `target/` entries while preserving `target/ripr`; then repeat with `--apply`.
5. Retain `target/ripr/reports/cargo-artifacts.{json,md}` with the handoff when build-disk state matters.

# Safety boundary

- Deletion is never implicit; `--apply` is mandatory.
- The current worktree is ineligible for stale GC.
- Budget GC will not select a tree newer than the minimum age.
- Symlinks, Windows reparse points, unreadable entries, and special files fail closed.
- `clean-current` refuses external or overridden target directories.
- `target/ripr` is a separate evidence/cache retention class and is always preserved by current-worktree cleanup.
- The forced repository temp redirect protects the Windows system drive and remains in force.

Do not use `cargo clean`, wildcard deletion, a shared global target tree, or manual traversal of Cargo's hash paths as substitutes. If the receipt remains over budget because protected or young trees dominate, report that pressure; do not weaken the bounds silently.

# Verification

Run:

```bash
python3 -m py_compile tools/python/cargo-artifacts/run.py tools/python/cargo-artifacts/test_run.py
python3 tools/python/cargo-artifacts/test_run.py -v
```

For a config-bearing candidate, hosted Rust qualification must also establish that normal builds still produce final binaries at the established `target/debug/...` paths while intermediates resolve below `ripr-swarm-build/{workspace-path-hash}`.
