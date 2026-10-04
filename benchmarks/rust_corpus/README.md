# Rust repository corpus

`benchmarks/rust_corpus/manifest.json` pins the real Rust repositories that
ripr evaluation work runs against: scoreboards, verdict checks, CPU, memory
and size measurements, mutation spot-checks, and first-run tests. Using one
corpus keeps results comparable across those lanes and stops them from
measuring only serde, ripgrep, regex, and ripr itself.

Each repository is pinned to one upstream commit (`sha`) and that commit's
first parent (`base_sha`). A subject therefore has a whole-repository state
and a real upstream diff, the change a reviewer actually saw:

```bash
cargo xtask rust-corpus fetch --allow-network --tier fast
cargo xtask rust-corpus list --tier fast    # id, tier, path, base_sha, sha
ripr check --root target/ripr/corpus/tokio --base <base_sha> --format json
```

`fetch` writes `target/ripr/corpus/<id>` and `target/ripr/corpus/index.json`
(paths, pins, stresses, probe counts). It fetches only the two pinned commits
per repository (`git fetch --depth 2 <sha>`), verifies HEAD and its first
parent against the manifest, and reuses a matching checkout on the next run.
`--repo <id>` selects individual repositories. Fetching is opt-in network and
never runs on the default CI path; `cargo xtask rust-corpus check` is the
offline manifest validator.

## Tiers

| Tier | Selects | Repos | Checked out | Fetch (measured 2026-10-04, cloud runner) |
|------|---------|-------|-------------|-------------------------------------------|
| fast | `tier = fast` | 12 | ~60 MB | 14 s cold |
| full | every repo | 19 | ~296 MB | 31 s cold |

Use `fast` for pull-request and quick local runs. Use `full` for nightly and
release measurement; it adds the large workspaces and monorepos.

## What each repository is for

| Repo | Tier | Kind | Rust lines | Stresses |
|------|------|------|-----------:|----------|
| serde | fast | library | 43k | proc_macro, compile_fail_ui_tests, build_rs, no_std |
| regex | fast | library | 161k | generated_code, large_library, no_std, workspace |
| ripgrep | fast | binary | 56k | binary_crate, custom_test_macro (`rgtest!`), workspace |
| anyhow | fast | library | 6k | build_rs, compile_fail_ui_tests, no_std, small_crate |
| thiserror | fast | library | 5k | proc_macro, compile_fail_ui_tests, small_crate |
| heapless | fast | library | 21k | no_std, embedded |
| rstest | fast | library | 21k | proc_macro, test_macro_rstest, workspace |
| proptest | fast | library | 38k | test_macro_proptest, proc_macro, custom_test_harness, workspace |
| insta | fast | library | 26k | snapshot_tests, workspace |
| prost | fast | library | 19k | generated_code, build_rs, proc_macro, custom_test_harness, workspace |
| axum | fast | library | 47k | async_tokio, workspace, proc_macro |
| rusqlite | fast | library | 51k | ffi, build_rs, bundled_c |
| tokio | full | library | 185k | async_tokio, workspace, custom_test_harness, large_library, loom_model_tests |
| clap | full | library | 85k | proc_macro, snapshot_tests, custom_test_harness, workspace |
| cargo | full | binary | 345k | binary_crate, custom_test_macro (`#[cargo_test]`), snapshot_tests, large_workspace, large_monorepo |
| rust-analyzer | full | binary | 587k | binary_crate, large_monorepo, snapshot_tests (expect-test), generated_code, large_workspace |
| wasm-bindgen | full | library | 371k | wasm_target, proc_macro, ffi, large_workspace, generated_code |
| embassy | full | library | 591k | no_std, embedded, async_no_std, build_rs, large_monorepo, ffi |
| bevy | full | library | 664k | large_monorepo, proc_macro, large_workspace, custom_test_harness |

The manifest's `why` field gives the reasoning for each entry, and its
`probe` block records the lexical counts behind the stress tags (Rust files
and lines, Cargo manifests, build.rs scripts, proc-macro crates, no_std
files, `harness = false` targets, checkout size). Probe counts describe the
subject at the pin; they are not ripr results.

`check` enforces that every declared stress category is covered by at least
one repository, and by a fast-tier repository unless the category is marked
`full_only` (large_monorepo, large_workspace, wasm_target, async_no_std,
loom_model_tests).

## Known gaps

The manifest's `known_gaps` lists what the corpus does not yet represent:
an application-style tokio service binary with a database layer, a
nextest-only test configuration, a cfg(windows)-dominant crate, and tests
that live in another repository. Python subjects stay in
`fixtures/python-eval-sweep/manifest.json`.

## Changing the corpus

Pins are exact. Refreshing a pin, adding a repository, or changing a tier is
a new `corpus_version`, so results recorded against an earlier version stay
attributable. Apply the manifest's `selection_rule` when choosing a new pin:
a recent first-parent commit whose diff changes 1 to 12 non-test `.rs` files
and 6 to 400 Rust lines, preferring behavior fixes over chores. Run
`cargo xtask rust-corpus check` and then a `fetch` of the changed entries,
which fails if the commit or its first parent does not match the pins.
