## Summary

Closes #5280 by making the workspace-relative linker-temp redirect
discoverable instead of removing it.

`ripr doctor --profile source-build` gains an advisory preflight
(`linker_temp_redirect`): when the selected workspace's own
`.cargo/config.toml` (or the legacy extensionless `.cargo/config`) redirects
linker temp variables (`TEMP`, `TMP`, `TMPDIR`) into a workspace-relative
directory (`relative = true`) that does not exist, doctor warns with the
failure mode and the one-line repair, for example:

    ~ .cargo/config.toml redirects linker temp variables (TEMP, TMP, TMPDIR) into
    workspace-relative target/, which does not exist; building with an isolated
    CARGO_TARGET_DIR fails MSVC linking (LNK1104 naming <root>/target/lnk*.tmp)
    until it exists. Create the missing directory before building from source
    (for example `mkdir target`); exported TEMP/TMP cannot override it (force = true)

The constraint is also documented in `docs/agent-context/validation.md`
(new "Workspace linker temp redirect" section).

## Why option (b): the `force = true` redirect is documented and load-bearing

The issue offered (a) drop `force = true` or (b) keep the config and make the
constraint discoverable. Investigation per the issue's instruction:

- `git log --follow .cargo/config.toml` attributes the redirect to 71b655552,
  shipped in PR #397 ("xtask: route check-pr temp through target"). Its body
  states the deliberate purpose: force Cargo temp variables to resolve under
  the workspace `target` directory "so `cargo xtask ...` does not hand MSVC
  linker temp files to a full system temp drive" (a full C: drive on the
  author's Windows checkout). No later commit changed the `[env]` block;
  PR #397 review comments carry no contrary ruling.
- Dropping `force` would not just "let an exported TEMP win" in the isolated
  worktree case: on Windows, `TEMP`/`TMP` are always exported by the user
  profile, so `force = false` would disable the redirect on Windows entirely
  and revert PR #397's documented scenario (linker temps back onto a full
  system drive).
- Removing the redirect would also move linker temp placement on the required
  CI path: the scratch lanes in `.github/workflows/rust-gates.yml` build with
  an isolated `CARGO_TARGET_DIR` and their own `TMPDIR` (explicitly
  `mkdir -p`ed at lines 89/139/168), and currently work with the forced
  redirect. Changing that behavior for the required merge gate is a risk with
  no offsetting need, which is exactly the case the issue says to prefer (b)
  for.

So the redirect stays; this PR removes the trap's invisibility.

## Issue -> fix -> discriminating test -> proof

| Issue | Fix | Discriminating test | Proof (all run locally at 8849379bc) |
| --- | --- | --- | --- |
| #5280 isolated-CARGO_TARGET_DIR builds fail LNK1104 until workspace `target/` is hand-created | `linker_temp_redirect` advisory in `ripr doctor --profile source-build` (human + JSON), plus `docs/agent-context/validation.md` section and CHANGELOG entry | `cargo test -p ripr --lib linker_temp_redirect` (parses only `relative = true` table entries; advisory fires exactly on redirect + absent dir, stays silent on present dir, no redirect, legacy-config fallback, oversized config) and `cargo test -p ripr --test cli_smoke doctor_source_build_preflight_warns_on_linker_temp_redirect` (end-to-end binary: source-build JSON carries the advisory check; analysis profile stays quiet; human output names `LNK1104` and `mkdir target`; a present `target/` resolves it) | `cargo fmt --check`; `cargo clippy -p ripr --all-targets --locked -- -D warnings`; `cargo test -p ripr --lib cli::commands::doctor` (43 passed); `cargo test -p ripr --test cli_smoke doctor` (25 passed); `cargo xtask check-fast` (selector stable, base origin/main, 4 files selected and independently verified with `git diff --name-only origin/main...HEAD`); policy cascade `check-no-panic-family`, `check-process-policy`, `check-allow-attributes`, `check-static-language`, `check-executable-files`, `check-network-policy`, `check-lint-policy` all pass; `check-local-context`, `markdown-links`, `check-doc-index` pass |

## Reproduction (witnessed at fe9239231, the issue's exact steps)

In a detached worktree of `fe9239231` with `CARGO_TARGET_DIR` pointed outside
the worktree and no workspace-local `target/`:

- `cargo check -p ripr` failed exit 101: `LNK1104: cannot open file
  'F:\...\ripr-fix-build-config\target\lnk{GUID}.tmp'` while linking build
  scripts (rustversion, thiserror, crossbeam-utils, icu_segmenter_data) —
  objects and `/OUT` correctly landed in the isolated target.
- `mkdir target` (confirmed git-ignored: `.gitignore:1:/target`) made the
  identical command pass (`Finished dev profile ... in 1m 04s`).

With this PR's binary, the same setup (fresh worktree, real repo config,
`target/` absent) makes `ripr doctor --profile source-build` print the
advisory naming `LNK1104` and `mkdir target`; after `mkdir target` the
advisory is gone. Doctor stays exit 0: the check is advisory, scoped to the
source-build profile, and never blocks analysis use.

## Non-goals

- The `.cargo/config.toml` redirect is not changed (documented above).
- Doctor `analysis` profile output is unchanged; the advisory only appears
  with `--profile source-build`.
- No workflow/CI changes; the scratch lanes keep their explicit `mkdir`s.

## Rollback

Revert the single commit; the doctor preflight and doc section disappear and
behavior returns to the inherited (undocumented-constraint) state.
