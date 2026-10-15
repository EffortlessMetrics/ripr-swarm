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

The seven-test source contract covers main-trigger retirement, filtered pushes,
disabled caller/producer admission, missing caller dependencies, a rebound or
missing producer, broad saves and an extra cache action. Independent review
also exercises deliberately wrong source configurations. Native Cargo and
source-export results are distinct evidence epochs; none is a real save/hit
or cross-ref reuse observation.

Before merge, follow the repository's normal local shaping, exact-head review,
remote merge-subject verification and protected Ready qualification procedure.
The parent cache issue remains open after this source-only slice lands:
owner7827 retains actual route lifecycle acceptance, and #7929 retains the
Contabo consumer/native qualification work. Preserve the selected-route and
retirement obligations above; a passing source contract closes none of them.
