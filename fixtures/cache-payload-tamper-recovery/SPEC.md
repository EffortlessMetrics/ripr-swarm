# Cache payload tamper and recovery corpus (#4515)

Spec anchors: RIPR-SPEC-0123 stored payload integrity; governing issues #4515
(parent qualification leaf #4507).

## Given

PR #4466 landed checksum admission for persisted evidence envelopes on `main`.
Library integration tests already exercise cold generation, same-key semantic
corruption, shard-set rejection, inventory exclusion, fingerprint handling,
recovery to an independent cold authoritative output, and a subsequent warm hit
for several families.

This manifest-only corpus commits the required family denominator, tamper
operation catalog, removal-control inventory, and traceability anchors to those
library tests. `cargo xtask check-fixture-contracts` loads `corpus.json` and
refuses silent omission, stale anchors, or a false exact-candidate pass.

## When

An offline validator run recomputes digest bindings where present, checks every
named persisted-evidence family records
`included_and_exercised`, `not_persisted_or_not_applicable`, or
`explicitly_excluded_with_non_claim`, and verifies that each
`included_and_exercised` row names producer and consumer test anchors whose
source files exist in the workspace.

## Then

- Every issue #4515 family id appears exactly once with a closed disposition.
- Every required tamper operation and removal control id is catalogued.
- Included families name nonempty library producer and consumer anchors.
- Every row's `exact_candidate_execution` remains `not_run` until the #4510
  harness runner and packet emission land in a follow-up slice.

## Must Not

- Do not claim exact-candidate qualification, #2769 consumption, or issue close
  from this manifest alone.
- Do not authenticate cache writers or claim hostile-writer resistance.
- Do not compete with #4382/#4466 production ownership.
