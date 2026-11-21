# Cost and Verification Policy

The engineering acceptance targets are **under 20 total raw runner-minutes
and under 20 minutes complete qualification elapsed for a normal accepted
submission or stack**. Qualification elapsed runs from its first qualification
allocation to the complete result; queue-delay measures are separate.
Both targets must be measured. A short critical path across many runners does not
establish the runner-work target, and a cheap individual G does not establish
the stack target. Cost classification does not decide whether a correctness
obligation remains required. See the
[#4841 acceptance clarification](https://github.com/EffortlessMetrics/ripr-swarm/issues/4841#issuecomment-6999978169).

[CI strategy](../CI.md#qualification-rebuild-architecture) and the
[detailed qualification design](QUALIFICATION_REBUILD.md) define the target.
[Current state](current-state.md) records the installed route and observed cost;
[merge flow](../MERGE_FLOW.md) owns the operational request and landing procedure.
This document does not activate a planner, budget guard or replacement workflow.

## Verification economics

We reduce repeated preparation, duplicated execution and obsolete work while
retaining the complete applicable merge-safety portfolio. The current full
workspace tests are not optional merely because their preparation is expensive.

`ripr` supplies static mutation-exposure evidence. It does not run mutants and
does not report `killed` or `survived` outcomes. It asks whether current tests
appear to contain a discriminator for the changed behavior. Its own CI must keep
that evidence claim separate from actual product qualification.

The target covers all merge-group attempts associated with the accepted
submission or stack: allocated prefix dispatches, retries, replacements and
their causally required preparation, transport and finalization. Exact-G
totals remain diagnostic; no G can pass the stack objective independently.
Cheap H admission and control work remain separately explicit lifecycle costs.

A local compiler optimization, a smaller archive and a shorter hosted job are
different observations. Claim the result that was measured. Neither raw
runner-minutes nor Local Evidence Minutes (LEM) establishes invoice savings.

## CI posture taxonomy

| Posture | Purpose | Qualification boundary |
| --- | --- | --- |
| Required | The complete applicable merge-safety portfolio, regardless of its current cost. | Conserved during optimization. The target executes product proof on exact combined G after cheap trusted H admission; the installed label/queue route remains until qualified cutover. |
| Advisory | Evidence with a separately declared nonblocking purpose. | Retains its own authorized trigger and meaning. It cannot replace a required test, contract or producer. |
| Release / deep proof | Release-bearing or separately selected proof beyond the required qualification portfolio. | Follows its existing explicit authority. It does not justify routine main replay or automatic development-event CI. |

Applicable extension proof remains required when selected. Adding extension
e2e to an unrelated Rust-only subject is unnecessary work; omitting it from a G
that includes a relevant extension change loses coverage. Whole-group scope,
not the last PR's filename suffix or a cost band, decides applicability.

## Measurement boundary

[CI actuals](ci-actuals.md) defines the proposed accounting requirements and the
limits of the existing audit command. Keep these views distinct:

- **Stack acceptance cost:** all allocated jobs across every associated
  merge-group attempt, including prefix dispatches that allocate a runner,
  failures, retries and replacements. Charge setup, preparation, workers,
  evidence, transport, verification, finalization and cleanup, including
  causally required work outside the main workflow or its timing window.
- **Exact-G diagnostics:** per-G and per-attempt breakdowns explain the stack
  total. They cannot replace it or exclude an expensive predecessor.
- **Lifecycle cost:** show H admission and control work separately, along with
  unsuccessful submissions and other work in the observation window. Report
  each stack total and its landed-PR count. A per-PR division cannot hide an
  expensive stack or establish acceptance.
- **Elapsed and resource limits:** measure first qualification allocation to
  complete result without restarting the clock on a new G or retry. Retain
  enqueue-to-result and enqueue-to-landing separately; the qualification target
  does not establish a queue-delay SLA. Also measure queue wait, CPU, attributed
  compiler peak, simultaneous process-tree memory, swap, disk and transfer.
  These are not interchangeable with raw runner-minutes.

List the included native run/attempt/job identities before comparing totals.
Report reused evidence with its original producer and cost provenance, rather
than counting the same job twice or presenting required preparation as free.
Unavailable records or measurements are unknown, not zero.

Compare equivalent selected obligations, configurations and declared cache
states. A warm filesystem, a dependency-cache hit and reuse of exact compiled
outputs are different conditions. A cache miss must preserve a complete,
bounded correctness route without relying on a recurring warmer.

## Acceptance of an efficiency change

1. Conserve applicability, selected cases, doctests, ignored-test behavior,
   assertions and required result meaning. Reject missing or wrong evidence.
2. Demonstrate that the actual consumer reaches the changed preparation or
   execution path. A merged interface that no required caller uses is not a
   runtime saving.
3. Measure the whole affected submission/stack boundary, with cold/warm
   conditions and failures disclosed. Sum unique job executions, not
   overlapping test durations; retain unfavorable measurements, qualification
   elapsed and both enqueue-based views.
4. State the result at its earned scope. An enabling change may satisfy its
   narrow acceptance without reaching the parent target. A complete-target
   claim requires the named representative accepted-submission/stack cohort and
   full accounting against both the runner-work and elapsed targets.

Choose comparison subjects and acceptance criteria before reviewing results.
Report individual stack totals and landed-PR counts, cold/warm strata, sample
counts, p50, p90 and maximum; a favorable median alone cannot establish the
targets. Do not choose only inexpensive successes afterward, infer a stable
percentile from a tiny sample, or create a fixed quota of full CI replays for
every PR.
An explicit memory or latency tradeoff remains a tradeoff even when total
runner work falls.

## Boundaries that cost pressure cannot change

- Draft, Ready, ordinary updates and review do not start automatic qualification,
  including cheap or skipped wrappers.
- An over-budget run still owes every selected required obligation. A label,
  forecast, cache hit or advisory budget report cannot waive that proof.
- Release-surface work is not added to every qualification without its existing
  applicability and authority.
- Correctness and consumer-parity evidence precede a routing change. Unknown
  scope broadens proof; missing inputs do not create an empty success.
- A numeric planning or budget-enforcement change requires its own accepted
  implementation and policy transition. Editing these documents does neither.

## Related policies

[LEM budgeting](lem-budgeting.md) retains approximate planning bands.
[Soft budget guard](budget-guard.md) records the unactivated advisory design;
legacy cost/posture fields do not demote required work.
[The RIPR soft gate](ripr-soft-gate.md) separately governs calibration and
acknowledgement of static exposure findings, not completion of required CI.
