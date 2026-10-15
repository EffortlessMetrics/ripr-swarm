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

The callable critical path has job execution limits of 3 + 12 + 5 minutes
(consumers run in parallel). Queue/setup overhead and actual hosted performance
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
