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
