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

Local evidence: Windows PowerShell, pinned Rust 1.95.0, five standalone tests
passed with native exit 0. Heavy concurrent Rust builds were observed before
execution; no Cargo build was admitted. Cargo baseline/check-fast/worktree-doctor,
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
