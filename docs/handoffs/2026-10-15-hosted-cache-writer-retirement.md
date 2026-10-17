# Hosted cache writer retirement contract (#7827)

Scope: source-level retirement evidence only. Issue #7827 remains open; #7929
owns the selected Contabo consumer and its native cache qualification.

At candidate `3edc7e7de8e43a0affaf1323a099b531d6c11f1e`, the archive
boundary changes to dependency profiles but saves remain guarded by
`github.ref == 'refs/heads/main'`. `routed-rust.yml` still admits main pushes.
A Ready PR uses a PR ref, so its hosted run cannot refresh this main-only
producer. Retiring the push trigger removes its automatic writer opportunity;
manual dispatch remains available but does not establish an automatic lifecycle.
Other rust-cache users have separate keys and are not evidence that this
producer persists its cache.

`tools/repo-policy/tests/hosted_cache_continuity.rs` binds the real caller,
reusable producer, caller dependencies, caller/producer job admission, sole
Swatinem cache action, scratch opt-out and main-only save guard. Removal, missing
producer, scratch rebinding and broad-save controls reject plausible accidental
cutovers. It is a deliberately strict source-shape contract, not a YAML engine,
a scheduler simulation, a save receipt, or a cache hit measurement. Source
refactoring requires review of the changed contract.

Run the focused integration target with:

```text
cargo test --locked -p repo-policy --test hosted_cache_continuity -j 2
```

The std-only target can also run without building the workspace:

```text
rustc --test --edition 2024 tools/repo-policy/tests/hosted_cache_continuity.rs -o <task-owned-output>
<task-owned-output>
```

Before #4838 retires main-push qualification, replace this temporary contract
with the reviewed selected-route writer/readers/ref/trust/storage lifecycle
and #7164/#4840 native observations. Do not retain full main qualification just
to warm a cache or broaden PR save authority to satisfy this test. A replacement
must preserve #7685 archive isolation and #7827's two distinct integration
subjects, compatibility misses, unavailable/denied/corrupt/evicted/cold behavior,
controller separation and total-work accounting. A passing test here closes
none of those native acceptance dimensions. No live workflows, cache contents,
credentials or protections are changed by this contribution.

## Qualification and continuation

[PR #8070](https://github.com/EffortlessMetrics/ripr-swarm/pull/8070) retains
exact candidate identities, native command receipts, independent review,
negative-control results and task-resource custody. Execution-specific paths,
capacity observations and temporary harness failures belong in that PR's
records rather than this portable repository handoff.

The eight-test source contract covers main-trigger retirement, filtered pushes,
disabled caller/producer admission, missing caller dependencies, a rebound or
missing producer, broad saves, an extra cache action and PR-only prerequisite
admission hidden behind restored trigger declarations. Independent review
also exercises deliberately wrong source configurations. Native Cargo and
source-export results are distinct evidence epochs; none is a real save/hit
or cross-ref reuse observation.

Before merge, follow the repository's normal local shaping, exact-head review,
remote merge-subject verification and protected Ready qualification procedure.
The parent cache issue remains open after this source-only slice lands:
owner7827 retains actual route lifecycle acceptance, and #7929 retains the
Contabo consumer/native qualification work. Preserve the selected-route and
retirement obligations above; a passing source contract closes none of them.


## Staging repair after native qualification

At [#8082 head `1915dfb6`](https://github.com/EffortlessMetrics/ripr-swarm/commit/1915dfb6a09d6c473e2220ccb3ddab609e78fd28),
native qualification exposed the intended retirement dependency. Exported exact
base `eb98c802c7db33f378c71d0cad5aed81989773d9` passes the original seven
source tests; that head passes four and fails three. The positive contract
rejects lost main push; two negative-control setups also require that trigger.
Changing the PR request from Ready to labeled does not itself retire the writer.
The retirement control now compares the unchanged actual PR event block instead
of requiring the old request spelling.

The agreed staging repair preserves main/master-push and manual **full qualification**
while production trigger retirement remains held. This preserves the existing
qualification obligation during staging; it is not a suite retained only as a
cache warmer. Restoring trigger declarations alone is insufficient: the former
PR-only admission still prevents route/detector execution. The source invariant
therefore binds the reviewed main/manual prerequisite branch, its explicit
`!cancelled()` status override across skipped PR admission, and the successful
route requirement in the hosted job. PR admission, selected self-hosted branches,
PR-only leaves and final main/manual result reduction remain the caller owner's
separate executable obligations. No new cache writer or save authority is added.
The staged profile must pass joined exact-head review and proof before recovery;
this test patch alone cannot make the failed candidate qualified.

## Observed legacy lineage and its limits

Retained native evidence narrows the earlier absence of observed reuse:

| Subject | Actual observation |
| --- | --- |
| [Main run 38537661704](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38537661704/job/115766193707), checkout `6e654eb1fdf193e03b14cf520af1cb633800e779` | `save-if: true`; post-job upload sent 587,026,894 bytes at 2026-10-14 09:39:50.588Z. |
| [PR #8070 run 38691306506](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38691306506/job/116277700689), checkout `d4504d0ca561ed66337ff7e71cd21803ba84d988` | Full-match restore of the legacy key; `save-if: false`. |
| [PR #8082 run 38705560148](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38705560148/job/116324145759), checkout `3a4c52d48f97b88b8db20ebb0f63073c153f102b` | Full-match restore of the same key and byte count; `save-if: false`. |

The exact legacy key is
`v0-rust-ripr-swarm-rust-small-Linux-Linux-x64-eb531e7b-b6999c15`.
A read-only cache API observation at 2026-10-15 08:52:53Z returned one matching
entry, ID `8930285189`, ref `refs/heads/main`, size 587,026,894 bytes, created
2026-10-14 09:39:50.868Z. The matching timestamp, key and size corroborate the
historical upload and two distinct integration-subject reads. The upload log
does not emit a cache ID or explicit save acknowledgement. This is legacy cache
lineage, not a new writer receipt or a measured compilation saving.

The historical writer predates archive-boundary candidate `3edc7e7d` and logs
fixture-path `ENOENT` errors during cache cleanup. Neither archive contents nor
#7685 isolation is established by those restores. Preserve that independent
repair; do not label this historical cache clean. Effective storage/tool/key
configuration, two-subject reuse on the selected replacement route, compatibility
misses, denied/unavailable/corrupt/evicted/cold behavior, trust separation and total
work remain #7827/#7929/#4988/#7164/#4840/#4841 acceptance. Production cutover,
credentials, security settings and cache contents remain untouched here.
