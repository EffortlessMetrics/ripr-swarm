# Python wheel exact-subject leaf (#4987)

## Scope and staging

Source basis: e231d468c79b6391395efb31b6e2f2819f738663, refreshed from main
on 2026-10-15. A pre-edit refresh observed main
94a42072e4b348f7a10aaa69ba4e19b5a7106a58: both owned source blobs remained
identical to the accepted basis; unrelated movement did not require restacking.
Admission ownership refreshed at
`codex/4987-windows-subset-admission` 0cf965fcdfca893e7ce9b89767ee7f88937f1822
and evidence companion 6902a132eb0366a31bdd1b52a30b189847c1f82f.
The shared routed-rust caller and its tests are untouched.

The existing wheel workflow is now callable. Its single build-wheel / pip / uv
implementation serves the callable path and the retained legacy PR/manual path.
No caller is added here. Production PR path filters and the independent
`workflow_dispatch(mcp_evidence_pair=true)` experiment remain. The latter still
has its own pinned objects, 60-minute budget and advisory semantics; it is not
part of the admitted wheel qualification budget. No main-push replay or native
merge queue is introduced. Ordinary push/review/admission lifecycle policy
remains the parent owner's responsibility.

## Callable contract

The caller selects wheel qualification conservatively before allocation and
supplies the existing admission vocabulary:

| Input | Meaning |
| --- | --- |
| proof_profile | `qualification` or separately authorized `diagnostic` |
| request_id | native run-attempt, digits-digits |
| head_sha | immutable H |
| base_sha | immutable B, required for qualification |
| subject_sha | actual execution commit T (H for diagnostic) |
| subject_tree | exact T tree, required for qualification |
| workflow_generation | caller workflow generation, matched to GITHUB_WORKFLOW_SHA |

`workflow_call` inherits the caller event. Input-key presence distinguishes it
from legacy PR/dispatch, including calls whose required values are empty. The
early shell guard rejects malformed identities, wrong native run-attempt,
wrong repository, wrong
generation, stale checkout, wrong tree or parents other than B H. Diagnostic
requires subject=H. Qualification always executes the already-selected wheel
obligation; it does not run the legacy event-diff selector on a synthetic merge.
Legacy policy/applicability still uses immutable event base/head and its existing
conservative failure behavior. A failed legacy selector remains red and retains
the fallback build/consumers; a failed callable guard/policy blocks expensive work
and leaves the callable workflow red. Missing/failed/cancelled producers cannot
be accepted as selected evidence by the future caller.

The callable critical path has job execution limits of 3 + max(8, 12) + 5
minutes. The source-contract job runs the unchanged package-source commands
alongside the release wheel job; each pip/uv consumer requires both producers
to succeed. The source contract checks the same exact subject and retains
failure propagation. Queue/setup overhead and actual hosted performance
are unqualified; these limits alone do not prove <=20-minute wall-clock admission.
The parent must budget its entire admission graph and reject timeout/cancellation.
No successful-skip receipt or candidate-authored success document is introduced.

Permissions remain `contents: read`; checkout credentials are not persisted.
There are no secrets, check-write, OIDC, publication or release permissions.
Artifacts retain the exact subject name and the original wheel/payload hashes.
The metadata/RECORD/payload corruption negatives, Linux native build, pip/uv
consumer Python journeys and uninstall checks retain their production commands.

The callable declares no workflow outputs. The parent consumes its reusable
job result (`needs.<job>.result`) and must require success when selected. Build
outputs `native_version`, `python_version`, `payload_sha256` and `wheel_sha256` stay
internal to the two consumers. The artifact is
`ripr-rs-python-wheel-<subject_sha>` in the calling run; it contains the original
wheelhouse, metadata, RECORD/payload negatives and tool/evidence files.

## Explicit activation delta (not applied)

1. Parent integrates this leaf with the reviewed admission owner and other
   selected platform/package leaves as one source candidate. Invoke
   `./.github/workflows/python-wheel-qualification.yml` as a reusable job,
   passing the seven inputs above from the trusted admission output. Use the
   admitted workflow generation, actual B/H/T/tree and native request identity.
   Do not pass `secrets: inherit` or expand permissions.
2. Select with canonical conservative package applicability. Require success
   for every selected leaf in the protected aggregate; missing, skipped, failed,
   cancelled or wrong-subject execution cannot satisfy a selected obligation.
   Preserve final live freshness/cancellation checks owned by admission.
3. In that same reviewed cutover, remove this workflow's `pull_request` trigger
   and retire its legacy applicability/fallback branch only once the corresponding
   admitted caller is present. Preserve workflow_dispatch and both manual modes.
   Independently authorized scheduled callers must pass an exact diagnostic or
   qualification subject; this leaf adds no schedule and removes none.
4. Execute combined-graph event/payload negatives and separately admitted hosted
   rehearsal: ordinary open/push/Ready must allocate no qualification; selected
   actual T must produce both consumers; test command failure, selector failure,
   missing evidence, stale H/B/T, cancellation and MCP-only dispatch. Verify real
   artifacts, required-result association and whole-graph timing before live
   activation. Settings/protections, release and deployment remain outside scope.

Rollback before activation is reverting this additive callable interface.
After activation, caller and leaf trigger restoration must be one coherent
rollback; do not remove enforcement while restoring only half the graph.

## Evidence and limitations

Focused test commands and exact candidate review are recorded with the delivered
commit. Local WSL Linux tests exercise real workflow shell, immutable Git histories
and job predicates. They do not build a Linux wheel or run the hosted pip/uv jobs.
Source qualification is not hosted proof, activation or measured savings.
Changelog disposition: Internal / CI fragment
`changelog.d/4987-wheel-callable-subject.md`, using the canonical fragment format.

Unchanged-base fixtures timed out in Git setup on DrvFs. Re-running serially
with a native Linux temporary root passed all 12 baseline subjects. Configure
that root through Cargo's `env.TMPDIR.value` and `env.TMPDIR.force=true` CLI
overrides when necessary; the repository forces its default temporary location.
This changes execution storage, not assertions or the 20-second child limit.
Local resource locations, capacity and retention belong to the task's proof
return, rather than portable repository documentation.

Observed source checks on the changed workflow/test bytes:

- `cargo test -p repo-policy --test python_wheel_workflow -- --test-threads=1`:
  14 passed, zero failed/ignored, including callable payload/event controls and
  tree-check, applicability-guard and failed-Git binding removal witnesses.
- `cargo clippy -p repo-policy --test python_wheel_workflow -- -D warnings`,
  `cargo policy preflight`, Actions syntax validation and scoped whitespace:
  passed. The preflight covers workflow and agent policy.
- Exact-base `cargo check -p repo-policy --all-targets` and the original 12
  workflow tests passed with the native temporary-root override described above.

The cold broader `xtask` baseline linked successfully, but its broad Git status
scan was stopped for filesystem latency; worktree-doctor/check-fast/precommit
baseline completion is not established by that attempt. The trailing broad WSL
whitespace scan was likewise stopped after the focused candidate gates passed;
native Windows Git supplied the scoped whitespace observation. These local
tooling observations are separate from source behavior and hosted qualification.


## Repair after admitted-run timeout

Joined run [38705560148](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38705560148)
on integration head `1915dfb6a09d6c473e2220ccb3ddab609e78fd28` admitted
its exact subject but cancelled the wheel job at its 12-minute execution cap.
The package-source step consumed 253 seconds before release compilation began:
170 seconds building the dev xtask, 82 seconds building its test executable,
and 20 distribution tests passed. This left approximately 448 seconds for the
release build; metadata validation and both consumers never ran. The protected
caller correctly rejected the cancelled leaf.

The repair moves those same two source-contract commands to a parallel job.
It preserves all build, metadata/RECORD/payload negatives and pip/uv commands,
release optimization/LTO/features, tool pins, event semantics and permissions.
Both producers depend on the original applicability authority; consumers require
both successes. A failed or skipped source contract cannot satisfy qualification.
The wheel timeout remains 12 minutes; no cache hit or weaker release profile is
assumed. Pinned package setup consumed only 13 seconds in the failed hosted run,
so it is not the primary serial cost. No Rust restore step existed in that job;
this repair removes a measured prefix without depending on a new warm cache.

The new parallel-contract oracle failed against the serial workflow and passed
after the repair. The complete focused suite now passes 16 tests; focused Clippy,
policy preflight, Actions syntax and scoped whitespace checks also passed. New
controls replay actual shell and reject failed Git inspection, failed contract
commands and missing/failed/cancelled producer results; removing the contract
result guard demonstrates false consumer admission. The previous broader-policy
completion limitations above remain; source checks do not establish hosted time.

## Measured wheel codegen repair

The subsequent standalone producer on integration head
`c4c67acbf08e905e5e5dc1a84ac5abab0a0e3620` completed successfully but took
12m49.685s, including 12m07s of release compilation, exceeding the callable
producer's unchanged 12-minute cap. Its
[run](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38715099088)
is counterevidence to treating the parallel-contract repair alone as sufficient.

The canonical `[tool.maturin]` Cargo configuration now selects four release
codegen units for wheels. Keeping this in the package manifest gives manual and
CI wheel producers the same recipe. Optimization level 3, full/fat LTO, stripping,
language features, locked dependencies, tooling pins, validation negatives and
all consumer commands remain. The root Cargo release profile and separate manual
MCP evidence route retain their existing single-unit recipe. Job timeouts,
permissions, triggers, applicability and caller/result authority are unchanged.

A local Linux x86_64 experiment used Rust 1.95, Maturin 1.14.1, four CPU affinity,
and independent cold Cargo targets on immutable c4 above. Only the wheel codegen
environment override varied. The same baseline was retained for the four-unit
follow-up; registry/OS/source pages could be warm, so this is a directional local
comparison, not a replicated or hosted speed guarantee.

| Recipe | Complete wheel-build command | Native payload bytes | Disposition |
| --- | ---: | ---: | --- |
| Existing one unit | 610.390s | 32,776,256 | Baseline |
| Sixteen units | 402.904s | 35,705,336 | Rejected: 8.94% payload growth |
| Four units | 353.610s | 34,084,976 | 42.1% less time; 3.99% payload growth |

Acceptance limits were set before measurements: at least 20% compile saving,
at most 5% native-payload growth, and at most 5% median CPU regression on the
bounded workloads. The baseline and four-unit wheels passed metadata, RECORD and payload
negatives plus independent pip and uv install, fresh-shell follow-up, disabled
adapter, reinstall and uninstall controls. Sixteen units remains rejected even
though its compressed wheel grew less than its native payload.

The accepted four-unit payload and baseline also passed eight interleaved pairs
per workload with exclusive, initially empty analyzer caches: Python128,
Rust32 and mixed Rust/TypeScript. Tests required complete analysis, intended
changed-file/probe counts, actual preview-language findings, equal exit status
and every JSON field equal after normalizing only the exclusive cache directory.
Median CPU ratios were 1.012, 0.960 and 0.906 respectively; wall ratios were
1.005, 0.963 and 0.925. OS/source pages could remain warm. These are small bounded
workloads, not universal runtime equivalence. A passive compiler observation
reached 7,429,136 KiB of per-process high-water RSS; full hosted memory headroom
remains unqualified.

The full experimental wheels used immutable c4 plus an environment override,
not this later source commit. Separately, a dependency-free pinned-Maturin probe
with canonical pyproject config and no profile environment override emitted
actual rustc `opt-level=3`, `lto`, `codegen-units=4` and `strip=symbols` arguments.
That establishes the config route; it does not turn the experimental artifacts
into exact-commit hosted proof. The manifest setting applies to every Maturin
wheel target, while measurements cover Linux x86_64 only; Windows/macOS runtime,
size and timing are not established here.

Early local namespace, jobserver and resumed-environment failures were harness
failures. The first concatenated Rust runtime patch was malformed and is not
runtime-quality evidence. Corrected difflib-generated workloads apply real new
source; the mixed workload explicitly enables both languages. Rejected and
failed attempts remain in the task proof return, without a clean-pass claim.

Activation delta remains the admitted-caller wiring and coordinated legacy
trigger cutover described above. This repair does not activate either. Review
the new exact integration subject and measure its separately authorized hosted
producer and complete admitted graph before claiming the 12/20-minute budgets.
No timeout increase, release, deployment or protection change is included.
Changelog: `changelog.d/4987-wheel-codegen.md` (Internal / CI).
