# Independent late-admission vectors

These 32 literal vectors inspect the inactive #4987 owner candidate
`e22a2e5e716806e8b84db5a418a28473a1f1be41`, tree
`29ad2515461fb9882745e31f01dc72063624f886`.
They are review inputs, not an evaluator, production selector, or controller.
The subject files and every active workflow are unchanged by this evidence branch.

## Coverage and use

`vectors.json` includes 10 event contexts, 12 live-state snapshots, 6 final
result environments, and 4 concurrency cases. Expressions were extracted from
the exact subject. Expected values are literal contract/docs-derived pins;
they were not calculated by a replacement expression interpreter.

Use the owner's existing proof route to evaluate the exact source expressions
with the supplied GitHub contexts, and the extracted real preflight/final shell
and jq predicates with the supplied API snapshots. Keep the native trigger
filter separate from evaluation of a hypothetical out-of-trigger context.
Terminal environments bind the selected Windows and GitHub lanes, not a
hypothetical aggregate success. The `request_selected_evidence_skipped` vector
must fail even though the Rust lane succeeded.

Check these wrong controls against unchanged expected values:

- Replace the dynamic final name with the static required name: the unrelated
  label case must reject the resulting required skipped/success check.
- Admit by the PR label collection instead of the event's added label: the
  unrelated label with merge-ready already present must reject it.
- Use output truthiness rather than equality with the string 'true':
  execute='false' is a nonempty string and must not allocate product work.
- Collapse metadata and qualification groups: the unrelated-label sequence
  must reject pending-qualification displacement.
- Omit final draft/head/base/subject/label checks: the corresponding live-drift
  cases must reject the false success.

No mutation, native expression, shell, jq, Cargo, or Actions run occurred in
this session. The execution environment failed during container creation with
`container_service_failed`. Tool inventory exposed no usable shell. The only
executed checks were 59 JavaScript structural assertions over the JSON:
nonempty partitions, 32 unique IDs, context/identity fields, required-name
fixture consistency, and exact extraction of the source expressions.
These establish fixture integrity, not GitHub semantics or workflow correctness.

## Findings for the production owner

1. Final readiness can drift after admission. The final PR predicate checks
   open/draft/head/base/subject/label but does not reload reviews or require
   mergeable=true. With initial current-head approval, then current-head
   CHANGES_REQUESTED, unchanged H/B/T and successful selected jobs, the source
   can still publish the required success. The review-revocation, dismissed
   review and false/unknown mergeability snapshots distinguish this from
   passing unchanged readiness. The old-head approval case is a synthetic final-state negative, not
   demonstrated reachable drift after successful preflight with unchanged H.
   Terminal vectors distinguish candidate_required_success from
   contract_required_success for the known review-revocation gap.
   Candidate acceptance fields are source-derived
   expectations, not observed job results. The final result has no final review
   read, so the review payloads are intentionally unconsumed counterexamples.
   The owner's handoff separately delegates final post-green review to the
   protected merge owner; this counterexample concerns the required result's
   currentness, not proof of automatic unsafe merging.
   Preserve the existing readiness owner and refresh it at the result boundary;
   a new service or generic controller is unnecessary. Separate PR/API reads
   and check publication remain non-atomic.

2. Default concurrency can displace pending explicit qualification. Three
   merge-ready runs for PR8008 have the same qualification group. With run301
   running and run302 pending, run304 replaces run302 despite
   cancel-in-progress=false. Run302 never reaches the duplicate preflight.
   Unrelated label metadata uses a distinct run-ID group and does preserve
   pending qualification. Distinguish these two propositions. Native queue:max
   is a documented possible alternative, with a 100-pending bound and no
   combination with cancel-in-progress=true; source/runtime compatibility is
   unexercised. A supported unique request grouping is another alternative,
   subject to the owner's duplicate/admission requirements. This branch picks
   no production repair.

3. Current tests assert source fragments and reject textual replacement; they
   do not execute the expressions, live predicates, or scheduler. Dynamic
   naming is correctly event/action/label-bound in the current fixture;
   case-insensitive MERGE-READY is consistently accepted by native expressions
   and the shell's lowercase normalization. The group checks only the event
   label. An unlabeled merge-ready context aliases qualification, but the
   current [labeled] filter excludes it: this is a trigger-expansion regression
   vector, not a currently reachable event defect.

## Evidence limits and integration

The source evidence and official references support the counterexamples.
Official docs explain that a skipped job reports success even when required,
and that default concurrency replaces an existing pending run regardless of
cancel-in-progress. They are not receipts from this candidate.

- [Expressions](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions)
- [Skipped required jobs](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-jobs-with-conditions)
- [Pending concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)

The parent and #4987 worker retain production wiring and coordinated activation.
Next: replay these vectors on an available executor/native expression route;
repair accepted findings in the owner candidate; refresh exact-candidate proof
and independent review before any trigger-safe qualification. No native scheduler
behavior, queue support, Windows execution, final race elimination, savings,
merge readiness, or CI acceptance is claimed.

Publication uses a new evidence-only branch based on main
`2af3f38a1de75ca48d5d84555a839d50710159b6`, tree
`840e2c3d897778d49a0a1abdf63d43588f5c00d1`.
All 30 repository workflows were read at that main: push triggers are restricted
to main/master or absent, so this new branch has no matching source push trigger.
No PR, Ready, label, dispatch, rerun, settings, credentials, or deployment action
was requested. Organization-required workflows/provider launch settings and local
hook configuration cannot be established from the unavailable executor.
Repository root AGENTS.md and prepare-proof/build-candidate/review-pr procedures
were read. .githooks and .pre-commit-config.yaml reads returned exact GitHub API
404 Not Found; recursive Git-tree read returned Transport closed. These errors
do not establish absence of all hooks or identify a cause.

Changelog disposition: no fragment; independent inactive review fixtures make no
product/API/output/workflow or public-doc behavior change, under the internal
no-behavior-impact exemption in docs/CHANGELOG_POLICY.md.
