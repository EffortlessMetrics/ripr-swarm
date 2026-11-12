# Formal-verification CI and admission policy

Formal verification should be useful enough to run and honest enough to block.
Those goals require separate critical, complete, and canary routes.

The target development lifecycle is #14500's hosted, queue-only path. Draft,
Ready, synchronize, label and routine push events must start no automatic
product qualification. This document describes the replacement topology;
it does not change the installed workflow or retire a protection. Explicit
warm-owner proof requests, scheduled deep proof and release/incident work
retain their separately selected authority.

## Layer 1: `kani-critical`

Purpose: bounded qualification of registered critical claims affected by the
actual admitted merge group G, including every member of its admitted stack.
The claim phase is `admitted_merge_group`, not an ordinary-PR trigger.

Selection inputs:

- changed production owner or source closure;
- changed claim, harness, model, bounds, cover, assumptions, or trust;
- changed runner, provider, parser, configuration, or result law;
- changed verification-authority policy;
- incomplete semantic classification.

Incomplete classification falls back to all registered critical claims. It
never falls back to no work.

Bind selection to the admitted base, actual G, complete ordered constituent
heads, accepted claim registry and controller generation. The executor consumes
the exact plan identity; it must not infer a smaller selection from paths,
one stack member or a prior H result. Claims whose complete source/configuration
closure is not proven unchanged cannot borrow earlier proof. Unsupported or
manual obligations remain visible. A fallback that exceeds the budget is an
honest incomplete qualification, not permission to omit claims or fabricate
success. #14474 owns the conservative selector and its completeness controls.

Required behavior:

- no `--allow-empty`;
- exact expected inventory;
- machine-readable result;
- missing, duplicate, stale, abnormal, or unsupported required evidence is
  non-success;
- reason recorded for each selected claim and for all-critical fallback;
- no credit from source census counts.

Initial evaluation target: p95 at or below 15 minutes after measurement and
optimization. This is a design target, not a current performance claim.

## Layer 2: full Kani

Purpose: complete registered Kani inventory.

Runs:

- nightly;
- explicit manual dispatch;
- an explicit full-proof request when the accepted qualification plan requires it.

The existing scheduled/manual full lane remains a separate inventory owner.
The replacement does not automatically add the full lane to every G, and a
critical selector cannot silently retire an applicable full-proof obligation.
Legacy merge-ready triggers are retired only through #14500's reviewed event
perimeter cutover, not by relabeling claims in this document.

The route may shard by package and measured cost class. Shards share one exact
run identity and expected inventory. Final aggregation rejects missing,
duplicate, overlapping, stale, or incompatible rows.

Expensive claims remain visible as `NOT_ESTABLISHED` or assigned later-phase
claims; they are not silently removed to make the route green.

Initial evaluation target: p95 at or below 60 minutes after sharding and
measurement. The current workflow budget is not evidence that this target has
been met.

## Layer 3: assurance canaries

Canaries test the proof apparatus, not ordinary product behavior.

Required canaries include:

- deliberately wrong implementation fails the named claim;
- required cover made unreachable fails;
- verified-looking output with abnormal process completion does not pass;
- duplicate source/native identity does not pass;
- stale or wrong-head result does not pass;
- missing expected result does not pass;
- changed domain, assumption, trust, or bound invalidates prior result;
- incomplete shard union does not pass.

Canaries may run scheduled, on explicit request, or in the admitted-G portfolio
when the runner, registry, parser, provider or claim satisfaction law changes.
They do not create an ordinary-PR or label-trigger exception.

## Metrics

Record separately:

- setup and version preflight;
- dependency/build preparation;
- Kani translation;
- solver duration;
- report parse/render;
- per-claim duration;
- peak memory and disk;
- output bytes;
- cache identity and hit/miss;
- timeout, tool, and infrastructure states;
- selected/expected/completed claim counts.

A cache hit is not a proof result. A fast refusal is not a completed proof.

## Required-check admission

#12904 owns the decision. A bounded critical context may become required only
after all of the following are true:

1. RIPR-SPEC-0371 reflects the current implementation.
2. Preflight, run, and per-subject execution are bounded.
3. Critical claims have stable IDs and machine-readable current results.
4. Affected-claim selection has an all-critical fallback.
5. Every required critical claim has explicit bounds, nonclaims, meaningful
   covers, and a negative control.
6. Missing, duplicate, stale, malformed, abnormal, or zero-subject evidence
   fails closed.
7. Base/head verification-authority transition protection is active.
8. An advisory observation window demonstrates acceptable p95 duration,
   resource use, and infrastructure reliability.
9. The outage policy yields `NOT_ESTABLISHED`, not bypass or fabricated pass.
10. Rollback removes required status without deleting claims or evidence.

Harness count alone is not an admission criterion.

## Provider and infrastructure outages

An outage record includes:

- provider/tool version;
- exact failed phase;
- source/claim inventory intended;
- process and cleanup status;
- retry count;
- retained diagnostic artifact;
- whether any exact rows completed;
- terminal `NOT_ESTABLISHED` reason.

Do not retry indefinitely or increase proof bounds/timeouts merely to obtain
green status. A policy decision to move a claim to another phase is a
verification-authority transition and cannot govern its own PR.

## Rollback

Rollback of CI admission:

- preserves the claim registry and result history;
- removes only the required status or selected workflow path;
- records the reason and remaining owner;
- keeps applicable obligations visible as advisory, scheduled, manual, or
  unsupported;
- does not call the claims not applicable merely because the provider route
  is unavailable.
