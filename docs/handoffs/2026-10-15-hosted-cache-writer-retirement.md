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
reusable producer, scratch opt-out, job eligibility and main-only save guard. Removal, missing
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

Initial standalone evidence: Windows PowerShell, pinned Rust 1.95.0, five standalone tests
passed with native exit 0. Concurrent Rust builds were observed during the initial epoch; that observation alone did not establish unavailable capacity. Cargo baseline/check-fast/worktree-doctor,
workspace compilation and required hosted checks remain unrun, not passed.

Resources: isolated clone `H:/Temp/ripr-task16-cache-continuity`; standalone
executables/logs under task-owned H:/Temp paths. Retain while draft review and
owner7827 handoff are pending. No shared build artifacts were mutated.

Independent review found disabled-job and path-filter oracle gaps in the first
version. Both now have discriminating negative controls. The same final test
logic against exported caller/producer bytes from candidate `3edc7e7de8e43a0affaf1323a099b531d6c11f1e`
also passes all five tests. Removing only its main push trigger makes the
positive source contract fail with native exit 101 and the #7827 diagnostic.
Task-owned logs are `H:/Temp/ripr-task16-evidence/candidate-tests.txt` and
`H:/Temp/ripr-task16-evidence/retired-tests.txt`. Initial temporary-harness
compilation failed due a raw-string delimiter collision; the corrected harness
uses exported files and that compiler failure is not behavioral evidence.

## Follow-up native qualification

The initial build-presence-only deferral was too conservative. A fresh measurement
at 2026-10-15T05:20:06Z found 98.19 GiB free physical memory, 62.23 GiB commit
headroom (207.38/269.61 GiB committed), and 2246.77 GiB free on H:. No actual
admission refusal was observed. Task16 admitted two jobs with a conservative
5 GiB additional disk / 8 GiB memory budget, using only its own
`H:/Temp/ripr-task16-cargo-target` and configured clone-local `target` temp.
No floor was lowered or other active resource modified.

At `ba5edc435c51341119a02d1f6798980ffd6d6cf5`, the real focused Cargo target
passed 5/5 tests, native exit 0, in 35.4236 seconds. Focused strict Clippy and
`cargo policy preflight` also passed. The target measured 48.95 MiB during
compilation; commit headroom remained 60.9 GiB. These are bounded local proof,
not full workspace qualification or cross-ref hosted cache observations.

Fresh published-head review then identified missing caller dependency and
producer-job/cache-set binding. Independent controls reproduced those gaps;
the caller-dependency assertion failed before repair (native101). The repair
binds the exact caller dependency declaration, requires the invoked producer
job to remain unconditional, scopes the named cache step to that job, and
requires exactly one Swatinem cache action there. New tests cover both missing
caller dependencies, disabled producer admission and a second broad-save cache.
No workflow bytes change. Native proof/review for this repair is recorded in
the PR with its exact committed head; earlier 5/5 and 7/7 epochs remain separate.

The repaired native focused Cargo target passes all seven submitted tests;
focused strict Clippy and policy preflight pass. Its first positive run exposed
an unanchored source-helper match (`jobs` matched the suffix of
`cargo-build-jobs`); mapping keys are now matched as complete lines. This was a
local parser defect and was repaired before publication. Final target size was
142.63 MiB, with 90.57 GiB physical free and 55.23 GiB commit headroom at
2026-10-15T05:25:21Z. No admission gate refused this work. Existing task-owned
Cargo state and witnesses remain retained for draft review, within the 5 GiB
additional-storage budget; other resources were neither cleaned nor modified.
