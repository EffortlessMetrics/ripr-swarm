# Native merge-ready production-wiring candidate — cutover held

Owner: #4987 with #4986/#4836. Fresh main `2af3f38a1de75ca48d5d84555a839d50710159b6`
was merged cleanly into this lane before wiring. No equivalent subset admission
writer was found in refreshed #4987 comments/open-PR search. The newer main-health
candidate is separate. #8002 and other owner branches are untouched.

## What is now wired, and what remains historical

The `.github/workflows` files on this **candidate branch** are executable workflow
wiring, not examples: `routed-rust.yml` has explicit labeled admission, the existing
Rust/docs producers and final aggregate; `windows-subset-qualification.yml` is
the extracted callable Windows producer; `windows-advisory.yml` calls that same
producer only on schedule/manual diagnosis. Its full twice-run job is unchanged.
`pr-staleness-watchdog.yml` remains API-only and observes requested rather than
merely Ready heads, excluding unrelated metadata runs from in-flight qualification.

The two YAML files under `fixtures/convergence/windows-subset` are frozen inactive
prototype/oracle inputs. Do not deploy them: the production caller additionally
refreshes final review/mergeability/thread state; the callee has explicit
qualification/diagnostic profiles. Production payload parity tests retain all
native commands, oracles, status/log/receipt checks and cleanup after the source
identity delta. No new controller, queue, database or scheduler was added.

The first controller/custody prototype at 9b8c8eb3b was superseded by e22a2e5e7,
and its Rust admission model remains removed. Historical Ready-only/static-name
policy is migrated in the same candidate. Compatibility function names in
repo-policy predate the new event law; they now enforce labeled requests, a
request-bound protected name, metadata-isolated concurrency, and an always-running
aggregate depending on admission and selected Windows evidence. Source/docs/skills
now separate source review before admission from final REVIEW_READY after green.

## Scope and safety

Only adding `merge-ready` to an open, reviewed, non-Draft PR is a qualification
request. Opening, pushing and Ready alone do not invoke routed qualification.
Other labels emit `Merge-ready Nonrequest`, not a skipped required success.
No main-push routed full run or branch dispatch can impersonate PR qualification.
Existing unrelated feature, wheel, security, full-Windows and editor workflows
remain separately owned: this bounded candidate does not claim graph-wide zero
launches. Retiring routed's old main cache writer may cause cold/stale acceleration;
proof still executes. No warm-cache or <=20-minute saving is assumed.

Admission is read-only native API bookkeeping, with no candidate checkout:
actor permission, current repository/target/open/Draft/label, H/B/T and parents,
mergeability, current review disposition and paginated thread state are checked.
`SOURCE_REVIEW_READY` on a current COMMENT supports a source-complete independent
review with only admitted hosted proof outstanding. GitHub identity alone does
not establish reviewer independence; the trusted requester verifies the substantive
review-of-record. Current approvals are observed without adding an approval quota.
Quota notices, skipped sensors or empty findings are not review. Product execution
retains existing trust/permissions; the source/attempt receipt separates H/B from T.

Sequential duplicate requests reconcile native runs by workflow, H/B/T title and
completed required result, without repeating producers. A prior failed result
remains failed; an unreadable or incomplete lookup refuses. Unrelated label
concurrency uses per-run metadata groups. **Native default concurrency still
replaces an existing pending request when another identical request arrives, even
with cancel-in-progress:false.** Running work is preserved, not exactly-once
submission. Read back queued/running exact-subject work before labeling/relabeling;
do not submit routine duplicates, retoggle Ready, or cancel unchanged slow work.
No optional queue setting or new controller is needed to describe this limitation.

The final result repeats live PR mergeability/readiness/reviews/thread checks,
including when reusing a prior green result. Selected evidence missing/skipped/
failed cannot green the aggregate. The final merge owner still makes the
vision/current-state decision and current REVIEW_READY judgment after green.
Separate API reads/publication/expected-head merge are non-atomic base guards;
changed integration inputs need affected-proof disposition and landed-tree readback.

The parent verified required `Ripr Rust Small Result` from app15368 and an active
ruleset with **no native queue**. This candidate keeps that identity and changes
no protection/settings. Complete classic strictness remains unreadable; it is not
assumed absent and does not invent a new settings prerequisite for this repair.
Main-health (>=1 hour AND >=10 merges) is a separate proposal, not a prerequisite.

## Evidence and remaining limits

The exact integration baseline passed `cargo check --locked -p repo-policy
--all-targets`. Focused workflow policy tests, source contracts, actionlint and
mocked native Bash/expression replays bind to this candidate; terminal manifests
in `/workspace/windows-subset-proof` identify final counts/source hashes.

Independent vectors from commit 88a82f942db1bad25e4ee0e2b2f333a6d7287ac7 are imported
unchanged at `fixtures/convergence/native-late-admission`. The worker's 59 structural
assertions were not runtime proof. This executor replayed all 32 vectors: 18 actual
Bash readiness/terminal cases, 10 expression cases and four group relations whose
scheduler consequence remains documented rather than natively executed. The old
review-revocation acceptance was reproduced on e22a2e5e7; corrected final refresh
rejects it. Permanent `repo-policy --test native_admission` executes the 18 actual
Bash cases with fixed mocked read-only API replies under bounded GNU timeout.

A wider repo-policy run had 62 passes and one unavailable matching llvm-profdata
prerequisite. That unrelated diagnostic test is not reported as green; matching
LLVM tooling is not a new admission-infrastructure dependency. No product suite,
PowerShell or native Windows execution, live labeled workflow, savings or merge
readiness is claimed by the local replay. Full xtask compilation/precommit remains
unrun; focused source-test compilation is distinguished from that broad driver.

## Minimum rehearsal and review before cutover

1. Independent exact-candidate source/oracle/security review of this combined
   production wiring and its native rehearsal path, with any repairs re-reviewed.
2. At a separately admitted existing same-repo PR and reviewed H/B/T/workflow
   generation, observe unrelated-label non-required naming/metadata isolation,
   Ready alone no routed run, then **one** explicit merge-ready request. Use the
   available qualified route plus native Windows; no Contabo/fleet deployment,
   native merge queue, controller, new settings or main-health implementation is
   a prerequisite. Do not open a PR while the early-broad-CI hold remains.
3. Read actual checkout/run/attempt, complete Rust/docs and Windows evidence
   denominator, source/status/log/receipt agreement and both cleanup outcomes.
   Confirm missing/stale/final-review-revoked results block; reconcile a duplicate
   only after reading existing custody, with no extra product execution. Native
   pending replacement remains a limitation, not a promised scheduler fix.
4. Only after combined candidate + rehearsal-path review and explicit cutover
   admission, integrate through the existing protected squash process. Obtain a
   fresh post-green final decision and actual required check readback; do not arm
   merge early or change protections. If current server rejects a base condition,
   respect it and refresh affected proof rather than guessing strictness.

Source landing is held in this task: no PR, merge, live labeled request, settings
change or CI dispatch. Branch publication is permitted only after rechecking its
push filters. Rollback restores the matching caller/callee/policy/instruction
source generation and retains old attempts/receipts; no synthetic green fallback.

## Resource custody

Own branch/worktree: `codex/4987-windows-subset-admission`,
`/workspace/windows-subset-admission`. Proof/toolchain/target:
`/workspace/windows-subset-proof`. The original checkout and all other writers'
branches remain unchanged. Local target is bounded/product-independent; compact
patch/bundle/logs are retained for parent review. No broad cleanup is authorized.

## Independent source review repair

Exact candidate `789a615` review found two source defects, both repaired on the
same owner branch: embedded Windows storage tests now read the extracted callee,
and ordinary COMMENTED reviews cannot erase a native CHANGES_REQUESTED
verdict. Admission and final refresh retain the latest decisive native review
(APPROVED / CHANGES_REQUESTED / DISMISSED) separately from source attestation
comments. A later native approval or dismissal resolves that reviewer's verdict;
ordinary comments, including source-ready markers, do not. Focused regressions
exercise both rejection cases and native resolution. All 30 embedded storage
Python tests run portably; this is consumer/source proof, not Windows execution.

## Older-review and cancellation repairs after published-source review

The [follow-up review on #4987](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6182909371)
found two defects in published `851195415bd2226f6cba2569cf94a4c787645251`.
That source acceptance is superseded for these dimensions; it was not native
qualification or merge readiness.

- **Outstanding verdict survives a push.** The reducer already kept decisive
  APPROVED / CHANGES_REQUESTED / DISMISSED states separate from comments, but its
  negative predicate ignored a CHANGES_REQUESTED attached to an older commit.
  Both admission and final refresh now block any reviewer's latest decisive
  CHANGES_REQUESTED, irrespective of commit. Actual later approval or dismissal
  clears that reviewer's negative disposition. Positive approval or source-ready
  attestation still requires the current head: resolving a verdict is not fresh
  positive proof. Ordinary comments/markers cannot clear a requested change.
- **Workflow cancellation survives green producers.** The result job's
  `always()` condition intentionally survives workflow cancellation; completed
  dependency results therefore do not establish that the workflow is uncancelled.
  Its last step now uses native `if: always() && cancelled()` and exits 1 when
  selected. It runs after readiness and aggregation, including the early-success
  reuse branch, and remains reachable if earlier steps fail or are interrupted.
  A skipped cancellation step cannot erase an earlier failure. `cancelled()` is
  evaluated in the supported step-condition context, not an environment field.
  See GitHub's [cancellation reference](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-cancellation).

Two independent agents reproduced the old behavior using immutable Git source:
24 actual Bash executions at both review boundaries exposed eight wrong successes
across older-verdict/comment/marker cases. Seven cancellation cases exposed three
wrong successes: cancellation with green dependencies, after aggregation, and
prior-success reuse. Controls included native later approval/dismissal, stale
positive approval, failed and cancelled producers. These are controlled read-only
API fixtures and literal expression cases, not live API/Actions observations.

The permanent `tools/repo-policy/tests/native_admission.rs` suite now has five
subjects, retaining all prior independent drift/terminal vectors and adding both
repairs. Admission tests execute the complete actual preflight body. Each actual
run-step body has its own shell scope; failures accumulate while the terminal
check remains reachable. This avoids putting a `set -e` body on the left of an
AND-list, which would suppress Bash errexit and manufacture a false drift pass.
Cancellation tests pin the native terminal condition, execute the rejection body,
cover normal and reused green proof, and retain failed-producer refusal. Independent
expression replay evaluates the extracted condition with controlled boolean status
functions. Actionlint validates the supported condition context.

This repair reuses the existing owner branch, unchanged Windows producer and
required-check identity; no #8002, platform-lane, main-health or protection work is
included. It does not activate workflows or open a PR. Fresh main was observed at
`8b7a9ba4a7ca2f1212fc3b456d0a2bee876a5e55`; its changes since the candidate's
`16190da1b` integration basis affect product analysis and fixture-home work, not
these workflow/test owners. No restack is needed for unrelated main movement.

**Remaining native rehearsal:** exercise real API/token shapes, exact check
name/head/integration association, older outstanding review state after a push,
controlled cancellation after successful dependencies and during finalization,
sequential duplicate readback, and one actual reusable Windows integration-subject
run with PowerShell, nonempty status/log/receipt agreement and cleanup. The native
scheduler and cancellation delivery are not simulated proof. Cancellation arriving
after the last condition evaluation/publication remains a non-atomic race requiring
fresh run/check readback; this adds no atomic-publication claim. Native pending
replacement, final freshness races and the unmeasured <=20-minute target remain.

Resource admission/retention: the existing owner worktree and Git common directory
remain in `/workspace`; toolchain and mutable Cargo target remain under
`/workspace/windows-subset-proof`. This repair admits <=128 MiB additional logs,
focused test output and compact patch/bundle, with >=8 GiB reserve (29 GiB observed
free). Original 851 source and independent negative witnesses are retained in Git
and `older-review-independent` / `cancel-independent` proof directories for the
parent's review/rehearsal. No frozen executable copies, product build or unrelated
cleanup are needed; source and proof stay retained with that consumer.

## Companion repair: subset evidence integrity

The independent review of `4eebbc6e6c3c078170790d52bca5877a0bda6d54`
identified a concrete evidence gap, recorded on
[#4987](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6183220409).
This companion starts from that exact candidate on
`fix/4987-reject-empty-windows-evidence`; the original owner branch and the
separate documentation integration remain unchanged.

The four-pair guard checked only file existence. Empty status files skipped
the `status == 0` executed-test check, so four empty status/log pairs could
produce a successful reusable job. The four Bash producers also ignored a
failed status write under `set +e` before their advisory `exit 0`.

Native Windows reproduction used Git-for-Windows Bash and standalone std-only
Rust tests, with Cargo mocked rather than executed. Against the unchanged input,
six tests yielded two passes and four intended failures: all empty pairs,
one empty component among valid pairs, malformed status, and hidden status-write
failure. The positive advisory and zero-test controls passed.

The guard now requires nonempty regular status/log files and one canonical
decimal native Bash exit status in `0..255`. It validates the file before command
substitution, rejecting embedded NULs and extra blank lines too. Nonzero native
test/lint verdicts remain advisory. Successful test subsets still require an
executed-test denominator; Clippy does not acquire a test-count requirement.
Target creation and status/log capture errors fail explicitly. The selector's
PowerShell capture uses explicit `try/catch` plus terminating cmdlet errors:
the focused native control showed that adding `-ErrorAction Stop` alone did
not prevent the isolated script's final `exit 0` after a failed status write.
The selector receipt validator, native commands, source admission, cleanup,
workflow events, permissions and required-result identity remain unchanged.

Permanent tests in `xtask/tests/windows_subset_evidence.rs` extract and execute
the actual production bodies. Seven tests cover 99 controlled replays: valid
success and advisory failure, every missing/empty/nonfile component, malformed
and out-of-range statuses, zero executed tests, Bash capture failures, and native
PowerShell capture failures. The PowerShell test mocks the consumer, not its
capture cmdlets. The seven existing admission/parity tests also pass. Their
comparison recognizes only the explicit evidence-hardening delta; the historical
prototype remains frozen and every native payload/selector oracle is compared.

Focused execution uses `rustc --edition=2024 --test` on those two test files,
without compiling the product or xtask dependency graph. `actionlint` passes
with external ShellCheck/Pyflakes disabled. The production policy checker is
run separately as `cargo run --offline --locked -p repo-policy -- check-workflows`;
only that small utility is built. The PowerShell repair fits the existing inline
line budget; the policy allowance is unchanged. Logs and the original failing
test executable are retained by the desktop task for independent review.

This establishes the controlled evidence guard/capture behavior on Windows,
not a hosted producer run. Real product test/Clippy execution, selector consumer
build/replay/receipt/cleanup, API/token/check association, cancellation delivery,
sequential duplicate readback and freshness races remain unrehearsed as described
above. No workflow is activated, PR opened, protection changed or runtime/release
qualification claimed by this companion.
