# Hosted dependency-cache boundary investigation

Bounded #4987 repair, related to #7685; #7827 retains cross-subject writer/reader
continuity. Base: `d4c952532ad29ac005fcf231748357ae41f6519b`. Candidate branch:
`fix/4987-hosted-cache-partition`. Admission, evidence reuse, native Windows
qualification and scratch/sccache remain with their existing owners. No workflow
trigger, required command, save authority, protection or release setting changes.

## What the completed runs actually establish

Read the complete logs of [job 115553677750](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38472100072/job/115553677750)
and [job 116078910676](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38629886041/job/116078910676).
They used Swatinem/rust-cache commit
`6323deb102c322ba6fcbdcafc7e3dddab59af2b6` (v2.9.2).

| Observed dimension | Older job | Newer job |
| --- | --- | --- |
| Checkout | `ce237e6c3087b95a33133e9bfefb80548a9c6a75` | `4fc71a48b55b83640c997d46f67bec3181716efe` |
| Restored full key suffix | `eb531e7b-340b2ac8` | `eb531e7b-b6999c15` |
| Namespace | `v0-rust-ripr-swarm-rust-small-Linux-Linux-x64` | same |
| Exact cache-key match | true | true |
| Compressed archive bytes | 489352849 | 587026894 |
| Initial xtask dev build | 1m33s | 1m25s |
| Outer test-profile build | 3m02s | 2m54s |

Both compiled with Rust 1.95.0 (`59807616e`), CARGO_BUILD_JOBS=8 and
CARGO_INCREMENTAL=0. Cache key construction also considered the installed Rust
1.99.0 toolchain; this is broader cache invalidation, not evidence that the
product ran on 1.99. The action logged CARGO_BUILD_JOBS, CARGO_HOME,
CARGO_INCREMENTAL and CARGO_TERM_COLOR as keyed environment variables, plus
Cargo manifests, lockfile, toolchain files and .cargo/config.toml. Both runs had
save-if=false on their PR refs. An exact restore did not save their new output.

The cache paths included the checkout's complete `target/`. No alternate
CARGO_TARGET_DIR or RUSTFLAGS was logged in these jobs; the actual driver
commands ran `target/debug/xtask`. The scratch route's private per-run target
and shared sccache are a distinct route, skipped in these reference jobs.

The first recompilations name workspace crates (`repo-policy`, `ripr`, `xtask`),
consistent with the logged cache-workspace-crates=false. This is expected
dependency-only cache behavior. Dev/check, all-features check, Clippy and the
test harness are different compile units; nextest's `--profile ci` selects its
runner configuration, not Cargo's release profile. Features, build-script inputs
and compile modes still belong to Cargo's fingerprint checks. Distinct Windows,
minimal-feature, coverage and release artifacts are not interchangeable.

No incompatible restore is demonstrated here. The jobs did not retain verbose
Cargo fingerprint rejection reasons or the original archive manifests, so a
complete per-artifact explanation of every rebuild remains unknown. Restored
bytes and restore duration do not measure compilation avoided. Workspace source
changes, pruning, feature/mode variation and immutable dependency-only keys must
not be relabeled as a proven toolchain/target mismatch or a measured saving.

## Selected repair and contract

The actionable boundary defect is that the default archive includes arbitrary
target siblings. #7685 records foreign fixture paths restored from an older
fallback archive. That contamination is independent of expected workspace
recompilation. This repair:

- Uses `ripr-swarm-rust-deps-v1-${{ runner.os }}` so neither exact nor fallback
  lookup can select the old whole-target namespace. First use can be cold.
- Retains `. -> target` for the action's existing dependency pruning and
  manifest discovery, disables whole-target archiving, and explicitly archives
  `target/debug` and `target/release` only.
- Keeps dependency-only workspace policy explicit and excludes cargo/bin.
  Registry/git caching, compatibility hashes and main-only save-if remain.

The upstream [configuration](https://github.com/Swatinem/rust-cache/blob/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/src/config.ts)
adds cache-directories independently of cache-targets. Its
[save path](https://github.com/Swatinem/rust-cache/blob/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/src/save.ts)
still prunes configured workspaces even when target archiving is disabled.
Therefore mapping each profile as a workspace would be incorrect. Cleanup still
recursively traverses target siblings; this change excludes them from the archive,
and does not claim to preserve those siblings against upstream cleanup.

This is acceleration configuration only. All current checks/tests still run.
No workspace artifact reuse, shared mutable target, controller cache promotion,
cache deletion, credential change or new cache writer is introduced.

## Regression evidence

On the exact base, worktree doctor passed and check-fast passed with zero changed
paths independently confirmed by Git. `cargo check -p repo-policy --all-targets`
passed. The driver was built with installed Rust 1.95.0 on Linux x86_64.

`cargo test -p repo-policy --test hosted_cache` first ran against the unchanged
workflow: two intended failures, including selection of a foreign fixture path;
three other contract cases passed. After the production repair, all five passed.
The tests retain debug/release dependency paths and refuse fixture/report/test
siblings, broad or absent target opt-out, a restored old namespace, incorrect
cleanup mapping, expanded save authority and workspace/tool-binary caching.
These are workflow contract tests, not a reimplementation of cache persistence.

An additional local probe parsed actual workflow inputs and action defaults,
then invoked the upstream bundled `CacheConfig.new()` with real local Cargo
metadata. Before: registry/git, cargo/bin metadata, entire checkout target.
After: registry/git plus only target/debug and target/release. The cleanup
workspace remained the original target. Local key environment hash `04a9bf31`
differs from hosted `eb531e7b`; these are local configuration observations,
not claims that the local fixture restored the historical hosted cache.

An independent reader separately executed the upstream bundled configuration
against controlled compiler/metadata commands: five assertions confirmed paths,
cleanup root, old-namespace fallback separation and RUSTFLAGS invalidation.
Exact committed-candidate review and subsequent gate results belong in the
GitHub handoff bound to that candidate SHA.

## Limits and next transition

No new hosted qualification was launched. Cross-ref persistence, save denial,
eviction, native hosted cold/warm behavior and end-to-end savings remain unmeasured.
The namespace requires a new main-scope writer before warm hosted reuse; #7827
must qualify the accepted writer lifecycle before the old producer is retired.
This repair does not complete #7827 or #4987. Cargo continues to reject stale
fingerprints and rebuild safely.

Future explicit target triples or custom Cargo profiles need reviewed additional
archive roots. The action remains a floating v2 reference, as before; upstream
behavior changes require contract review. A rollback reverts this one boundary
change but restores exposure to the old archive namespace; do not claim that
rollback also purges existing contaminated caches.

The initial all-state cache PR query via gh GraphQL was denied with `Forbidden`;
no alternate search route was used to bypass that denial. Source, relevant issue
comments and remote branch names were inspected, but exhaustive all-state PR
ownership remains a visibility gap. A direct attempt to list the root account's
toolchain directory also returned Permission denied; the workspace's installed
toolchain was discovered through the authorized workspace filesystem.
