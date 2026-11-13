# Formal verification in ripr

This directory defines how ripr uses formal methods without overstating what
they establish. It is the maintained entry point for both:

1. **ripr self-assurance** — bounded claims about ripr's own fail-closed
   decision cores and state machines; and
2. **formal-guard integration** — change-local analysis of Kani, Verus,
   property, model, and other specialized guards in repositories ripr
   analyzes.

These products share identity, currentness, result, and transition concepts.
They do not satisfy one another automatically.

## Current state

The repository already has:

- a pinned Kani workflow and `cargo xtask kani`;
- conservative handling for missing tools/results, timeouts, ambiguous result
  attribution and abnormal process completion; complete positive-cover admission
  remains with #14469 and bounded tool preflight remains with #12581;
- bounded Kani proofs over production decision cores;
- a generated source census of ordinary Kani harnesses;
- a governed claim-registry candidate with five initial critical claims,
  deterministic planner projection, generation transitions, and a registry-only
  scoreboard that keeps every claim non-established until an exact result join;
- scheduled property and metamorphic fuzzing;
- bounded static Kani and Verus source facts for user-facing analysis;
- an unattested Kani result importer;
- a TLA+ repair-lifecycle pilot with retained finite native model checks;
  production correspondence and Rust adapter qualification remain separate.

The missing durable layer is a maintained chain from an exact proposition to
its production owner, proof/model, native result, base/head transition, and CI
policy:

```text
production behavior
→ registered formal claim
→ exact owner/domain/observable
→ harness or model and trust boundary
→ current native result
→ base/head authority transition
→ product and CI projection
```

The governed registry in `policy/formal-claims.json` is the candidate
membership and generation authority for #14473. Its registry-only scoreboard
does not promote any existing result. The #14472 native-result producer,
admitted lifecycle join, and proof-authority transitions remain separate
runtime obligations.

## Non-cross-credit law

Keep these states distinct:

```text
source recognition
≠ semantic owner relation
≠ formal proposition
≠ native tool execution
≠ specification adequacy
≠ implementation conformance
≠ CI admission
```

Keep proof families distinct:

```text
Kani bounded proof
≠ Verus deductive proof
≠ TLA+ model checking
≠ Loom/Shuttle concurrency exploration
≠ Miri execution
≠ fuzzing
≠ mutation testing
≠ ordinary test execution
```

A result contributes only to the registered proposition whose source,
configuration, model, provider, denominator, and result law match.

## Maintained authorities

| Concern | Current or intended authority |
| --- | --- |
| Kani lane semantics | `RIPR-SPEC-0371` |
| Kani runner | `xtask/src/kani_lane.rs` |
| Kani workflow | `.github/workflows/kani.yml` |
| Self-proof claim boundaries | #12109 |
| Governed claim registry and scoreboard | #14473 / `policy/formal-claims.json` |
| Required-check and scoreboard decision | #12904 |
| User-facing FV integration | #10745 |
| Kani source/context inventory | #10686 / draft PR #11517 |
| Kani owner/domain/observable relation | #12596 |
| Guard-only and proof-only transitions | #12512 / #10749 |
| Static guard weakness findings | #10687 |
| Source-bound native result correlation | #10689 / #12597 |
| Formal adversarial corpus | #10746 |
| Verification-authority transition | #5224 / #6452 / #6456 |
| Specialized proof families and applicability | #6381 / #6385 / #6390 |

Do not open parallel owners for these claims. Extend or split the existing
owner where scope has become too large.

## Documentation map

- [CLAIMS.md](CLAIMS.md) — stable claim identity, required fields, lifecycle,
  negative controls, and review.
- [RESULTS.md](RESULTS.md) — native result identity, status law, currentness,
  artifact handling, and projection.
- [CI.md](CI.md) — critical/full/canary topology and required-check admission.
- [MAINTENANCE.md](MAINTENANCE.md) — operator and maintainer runbook.
- [ADMISSION_AUDIT.md](ADMISSION_AUDIT.md) — isolated document-consistency
  checks, adversarial controls and the boundary to native assurance.
- [REGISTRY.md](REGISTRY.md) — governed membership, decision and registry
  generations, planner projection, registry-only scoreboard, transitions, and
  the boundary to admitted native results.
- [CLAIM_PLAN.md](CLAIM_PLAN.md) — passive affected-critical-claim selection,
  fail-closed fallback, queue-subject identity, and the boundary to execution.
- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) — dependency-ordered
  implementation programme using the current issue and PR owners.
- [WORK_ITEMS.md](WORK_ITEMS.md) — existing executable implementation owners
  and remaining acceptance.
- [`schema/`](schema/) — draft machine contracts.
- [`examples/`](examples/) — non-authoritative examples conforming to those
  draft contracts.

## Claim language

Acceptable:

- “Kani established claim X for source Y within assumptions A and bound B.”
- “TLC found no violation of invariants I in model/configuration M;
  implementation conformance is separately tested.”
- “The Verus result is current for claim X; specification adequacy remains a
  separate question.”
- “No admitted provider currently exists for this applicable obligation.”

Not acceptable:

- “ripr is formally verified.”
- “The complete program is correct.”
- “A passing verifier proves the specification matches intent.”
- “TLA+ proves the Rust implementation.”
- “A syntax census is formal coverage.”
- “A successful tool process establishes the proposition.”
- “The head proof passed, therefore removing the base proof is safe.”

## Self-assurance completion boundary

The self-assurance milestone under #12109 and #12904 is complete only when:

1. RIPR-SPEC-0371 matches the implemented lane.
2. Tool preflight, execution, and result collection are bounded.
3. Critical self-claims have stable IDs, exact domains, bounds, covers,
   nonclaims, negative controls, and current machine-readable results.
4. Changes to proofs, assumptions, trust, bounds, covers and effective
   configuration invalidate affected evidence and retain applicable base
   protection until a replacement or retirement is accepted.
5. A generated assurance scoreboard joins registered claims to current
   results.
6. A critical Kani route runs advisably on the admitted qualification subject
   with complete selection/fallback evidence; it adds no development-event
   subscription.
7. Model results have explicit production correspondence evidence before
   contributing to an implementation claim.
8. Every unsupported, stale or not-established obligation remains visible.

Required-check admission is a separate decision under #12904 after measured
advisory operation and transition protection. A maintainer must be able to
reproduce a positive result and the named negative controls from retained
source, configuration, commands and artifacts.

## Formal-guard product completion boundary

The separate #10745 product programme additionally requires an installed
Kani journey joining changed behavior, owner, domain, observable, static guard
state and exact native result; guard-only changes must remain affected and
the adversarial corpus must qualify the user-facing claims. That product
journey does not establish RIPR's own correctness, and implementing it is not
a prerequisite for the self-assurance milestone above.
