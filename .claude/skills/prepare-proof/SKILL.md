---
name: prepare-proof
description: Design discriminating proof for one selected claim before or during implementation. Use when tests are absent, weak, self-confirming, or disconnected from the retained production path.
---

# Result

A bounded proof contract distinguishes the desired behavior from the strongest plausible wrong implementation and exercises the real authority and consumer.

# Workflow

1. Name the claim, producer, consumer, and authority boundary.
2. Inspect current tests, fixtures, goldens, schemas, reports, and CI routes before adding another oracle.
3. Define the cheapest positive case that reaches the retained behavior.
4. Define at least one discriminating negative or alternate case that would expose a fake, stale, proxy, nearby-owner, or no-op implementation.
5. Prove production-path reachability:
   - invoke the retained public or internal entry point;
   - establish that the intended branch and owner ran;
   - fail fixture setup at setup;
   - assert the intended parsed subject exists;
   - never treat skipped or unobserved work as pass.
6. Separate source failure, test-or-oracle failure, instrument failure, infrastructure failure, and not-established evidence.
7. Use a removal or deliberately wrong implementation experiment when it materially strengthens the oracle.
8. Bind evidence to the relevant repository, candidate, configuration, and artifact identities.
9. Challenge the proof with a fresh oracle review before continuing implementation.

# Proof laws

- Printed output is not proof of retained behavior.
- Goldens can encode incorrect expectations; add an independent invariant or corpus when promotion risk is material.
- Derive expected semantic values from the contract, with literal pins or an independent invariant; do not compute them with the production decision being checked. Shared setup helpers are fine. When a targeted wrong-result or removal control materially strengthens behavioral proof, verify that the unchanged expected values reject it; record unexecuted discrimination as not established.
- A failed prerequisite must not masquerade as a downstream analyzer failure.
- Status zero with missing or structurally unrecognized evidence is incomplete, not clean.
- Platform-specific branches require platform-capable proof.
- Bounded subprocess-cleanup claims need an actual owned-descendant control that distinguishes primary-only termination from containment cleanup, including resistance to graceful termination when escalation is claimed. Observe terminal status, primary reaping, and descendant cleanup before deleting fixtures; a timeout return alone is insufficient. Use the existing platform-specific process owner: Linux process-group evidence does not qualify Windows containment, and unknown third-party processes are not owned. Record unavailable execution as not established.
- Process success, static movement, semantic correctness, mutation adequacy, and merge readiness remain separate.

# Subagents

A test-oracle subagent may challenge fake oracles, token coincidence, platform/path assumptions, and missing negative controls. A separate source-mapping subagent may trace the production consumer. The lead Claude context reconciles one proof contract.

# Valid outcomes

- `PROOF_READY`
- `PROOF_NEEDS_REPAIR`
- `PRODUCTION_PATH_NOT_REACHED`
- `INSTRUMENT_FAILURE`
- `EXTERNAL_BLOCKER`
- `NOT_ESTABLISHED`
