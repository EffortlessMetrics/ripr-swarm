# Current formal-verification delivery state

This is the executable delivery index for the architecture in this directory.
It records the current authoritative owners, dependency order, qualification
boundary, and remaining work. It is not a second claim registry and it does not
turn a passing verifier process into assurance.

Last reconciled against `main` on 2026-11-17.

## Status vocabulary

Use these states consistently:

- **LANDED** — source is on `main`; this says nothing beyond the landed change's
  documented scope.
- **QUALIFIED / UNLANDED** — exact source has focused positive and negative
  evidence, but it is not repository authority until it lands through the
  protected route.
- **DRAFT** — implementation exists but one or more source-review,
  qualification, composition, or admission requirements remain.
- **BLOCKED** — the next transition depends on another owner or an unresolved
  current-base conflict.
- **NOT ESTABLISHED** — the formal proposition is not currently established.
  This is the required default for missing, stale, ambiguous, supplied-only, or
  incompletely bound evidence.

## Landed foundation

| Owner | State | Landed result | Remaining boundary |
| --- | --- | --- | --- |
| PR #14471 | LANDED | Maintained claim/result/CI/maintenance contracts under `docs/formal-verification/` | Documentation is policy and design authority; it does not execute a verifier or establish a claim. |
| PR #14469 | LANDED | Existing Kani lane requires complete positive cover evidence and keeps missing or ambiguous cover evidence fail-closed | The source census is not a registered assurance denominator. |
| PR #14545 / #14472 | LANDED | Run-owned `ripr.kani-observation.v1` JSON/Markdown Journal with exact selected-source inventory and explicit non-promotion | The producer intentionally emits observations, not registered claim assurance. Its current output has no authority to mint `ESTABLISHED`. |

These three results are the current `main` authority. All downstream work must
extend them rather than reintroducing a second Kani executor, parser, registry,
or report owner.

## Active delivery chain

The chain below is dependency ordered. A downstream candidate may be locally
composed for review, but it must not be described as landed or authoritative
while an ancestor is open.

| Order | Owner | Current state | Required next transition |
| ---: | --- | --- | --- |
| 1 | #12579 / PR #12581 | BLOCKED: the bounded `cargo kani --version` probe was qualified at its historical head, but the PR is now conflict-dirty against current `main` | Reconcile only the unique typed-capture and bounded-probe behavior with the landed observation producer; preserve current process-owner and Journal changes; rerun focused mutation/restoration controls and current native qualification. |
| 2 | PR #14602 | DRAFT; depends on #12581 and the shared process owner | Land a private, non-deserializable live byte-capture capability that settles owned descendants and refuses incomplete capture. Passive files must not be able to impersonate this capability. |
| 3 | PR #14478 | DRAFT | Land the offline evidence/document auditor and its fail-closed admission predicates. An auditor success means the documents are internally consistent, not that the verifier ran. |
| 4 | PR #14542 | DRAFT atop #14478 | Land the critical-claim planner and its refusal for plans with no critical claim or incomplete dependencies. |
| 5 | PR #14556 / issue #14473 | DRAFT | Land the governed claim registry, generation transitions, index, and scoreboard. Keep every missing/stale/unestablished claim visible and keep the Kani source census separate. |
| 6 | PR #14549 | DRAFT | Land the versioned native-execution declaration and exact provider/profile/tool/configuration contract. |
| 7 | PR #14551 and PR #14575 | DRAFT stack | Land one native Kani result interpreter with unambiguous result-block attribution, exact cover admission, and schema-valid machine output. Do not retain two parser authorities. |
| 8 | PR #14577 | DRAFT | Land passive source-input closure with exact file-set equality, duplicate rejection, supported include discovery, and fail-closed handling of unsupported nested input forms. |
| 9 | PR #14588 and PR #14597 | DRAFT | Land command-error and selector-domain repairs so enum/domain drift is a compile-time or explicit coverage failure rather than a silent reduction of the proof domain. |
| 10 | #14473 integration | NOT YET AUTHORITATIVE | Join the landed observation producer, live capture, declaration, source/configuration commitments, registry identity, parser result, required covers, and negative-control state into a local result. Replayed or supplied evidence alone must leave claims `NOT_ESTABLISHED`. |
| 11 | #6452, #6456, #5224 | NOT IMPLEMENTED | Compare base/head verification authority and preserve applicable base protection until a typed replacement or retirement is accepted. |
| 12 | #14474 / #12904 | DEFERRED | Add conservative affected-critical selection and advisory CI; required-check admission is a separate measured decision after authority-transition protection. |

## Qualified composition checkpoint

The contribution branch
`contrib/14473-native-producer-prerequisites` at
`cd0531ae2838e8a17779bb2f8cde434ffac8fa2d` is the current integration
checkpoint for the unlanded prerequisite stack. Retained qualification records
show focused canonical-library and process/Journal test graphs plus mutation
controls.

This branch is **not** repository authority and must not be merged wholesale
onto a newer `main`. It contains a useful exact composition, but current-base
adoption must:

1. preserve all later `main` changes;
2. carry only unique source from each owning PR;
3. resolve duplicate or superseded implementations at their original owner;
4. re-run qualification on the exact composed head;
5. use the protected landing route.

Historical proof results for a different claim ID, generation, source closure,
profile, toolchain, or configuration are evidence about that historical
subject only.

## Native join contract

The first maintained native join must be one execution path, not a collection
of mutually trusting reports:

```text
landed Kani request
+ private live-capture capability
+ exact program/argv/cwd/environment commitment
+ tool and provider profile
+ exact selected source closure
+ claim ID, generation and registry digest
+ unambiguous native result and all required covers
+ negative-control identity
+ bounded process, output, time, memory and artifact state
= local claim result
```

The result remains non-canonical unless the admitted execution environment and
protected provenance contract authorize promotion. The following are never
sufficient by themselves:

- a successful exit status;
- schema-valid JSON;
- a replayed transcript;
- matching hashes supplied by the same untrusted input;
- a source census;
- a passing unit test;
- a verifier success for a different profile or generation.

## Required discriminating controls for the join

Before the join can leave Draft, retain controls that demonstrate all of the
following failures are detected:

1. supplied/replayed evidence cannot mint live origin;
2. a stale claim generation or registry digest is rejected;
3. wrong source closure, subject, provider, profile, tool, configuration, cwd,
   argv, or environment is rejected;
4. duplicate, missing, ambiguous, or unmatched result blocks are rejected;
5. a required cover that is absent, unreachable, undetermined, or merely
   claimed in a forged transcript is rejected;
6. abnormal exit, timeout, incomplete pipe drain, unsettled descendant, or
   missing exit status remains `NOT_ESTABLISHED`;
7. artifact-count, byte, invocation, time, memory, and disk caps include early
   failure paths;
8. configuration values and dynamic names are committed without leaking their
   raw values into retained reports;
9. JSON and Markdown are projections of one DTO and agree on identity and
   status;
10. removing each material guard makes the intended control fail, and exact
    restoration passes;
11. one real bounded native run succeeds on the exact reviewed source; and
12. the same source cannot be promoted by passive replay outside the admitted
    execution path.

## Schema and report maintenance

Formal schemas, examples, rejection fixtures, registry documents, replay
reports, and emitted native reports need one maintained repository command.
That command must:

- use the repository's existing Rust schema evaluator;
- reject duplicate JSON keys, unknown local references, unsupported keywords,
  invalid recursion/depth, and schema/runtime drift;
- validate real emitted reports in addition to hand-written examples;
- remain separate from evidence authenticity and claim admission;
- have mutation/restoration controls for its material guards.

Schema validity means only that bytes satisfy the declared shape.

## Documentation required at each landed transition

Every integration slice updates, in the same protected change:

- the owning issue and PR with exact source and qualification identity;
- `RIPR-SPEC-0371` when lane behavior changes;
- claim generation and registry/index/scoreboard outputs when decision-bearing
  semantics change;
- machine schemas, examples, rejection fixtures, and runbook commands;
- this work index, moving the owner to LANDED and naming the next successor;
- explicit nonclaims, trust assumptions, bounds, rollback, and retained failed
  or superseded evidence.

Do not delete historical failed or stale evidence merely because a successor
passes. Preserve enough identity to explain why it no longer applies.

## Separate user-facing product track

The #10745 product programme is not cross-credited as ripr self-assurance. Its
first Kani path remains:

1. qualify or split #11517 for context/source/cache/public inventory;
2. implement #12596 exact owner/domain/observable relation;
3. preserve proof-only and guard-only transitions through #12512/#10749;
4. implement #10687 bounded static weakness findings;
5. correlate exact native observations through #12597/#10689;
6. complete #12598 native and installed-command stages; and
7. qualify user-facing claims through #10746's adversarial corpus.

Static guard facts, native execution, specification adequacy, and trusted
execution remain separate axes.

## Completion boundary

The first maintained self-assurance milestone is complete only when:

- the executable chain above is landed on one current source lineage;
- critical claim families have stable IDs, complete source closure, explicit
  domains/nonclaims/bounds/covers, current native results, and retained
  behavioral negative controls;
- live origin and passive replay are structurally distinct;
- every missing, stale, failed, unsupported, or not-proven obligation remains
  visible;
- base/head verification authority cannot be weakened by the changing PR;
- the assurance scoreboard is generated from the governed denominator;
- advisory selection is conservative and measured; and
- any required status is separately admitted under #12904.

Until then, the accurate repository-level statement is: ripr has bounded,
falsifiable formal claims and qualified infrastructure in progress; ripr is
not "formally verified" as a whole.
