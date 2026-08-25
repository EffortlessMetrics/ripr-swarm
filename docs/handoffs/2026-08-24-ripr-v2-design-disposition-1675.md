# RIPR v2 walking-skeleton design disposition

Work item: `EffortlessMetrics/ripr-swarm#1675`

This is a bounded root receipt for the first proposed RIPR-SPEC v2 walking
skeleton. It records the current disposition without creating a requirement,
implementation slice, or source-behavior claim.

## Receipt identity

| Field | Value |
| --- | --- |
| Repository | `EffortlessMetrics/ripr-swarm` |
| Observed base/head | `d46e352007c1963dc33931e864a8b09e9ba918bd` |
| Issue | `#1675` |
| Subject | `#1672` self-hosted runtime-promotion walking skeleton |
| Authority map | `docs/specs/RIPR-SPEC-0130-ripr-authority-map.md` |
| v2 runtime boundary | `docs/specs/RIPR-SPEC-0132-spec-v2-runtime-promotion.md` |
| Existing slice model | `.allow/spec-system/slices/` / `ImplementationSliceV1` |
| Result | `AlreadySatisfied` / `DuplicateOrSuperseded` |

The subject is no longer a safe implementation root for a fresh v2
materialization: the repository already contains the v2 authority map,
runtime-promotion boundary, implementation-slice model, and downstream
receipt-producing surfaces. The remaining open issues describe follow-on
work, not an unmaterialized copy of the original walking skeleton.

## Pass 1 — issue intent and evidence

The requested outcome was a portable, challenged design receipt before
materializing a v2 requirement or implementation slice. The repository now
has those durable boundaries in place. Evidence is limited to the current
checkout and tracked source-of-truth files; no claim is made about external
adapter generations, private orchestration logs, or live GitHub state beyond
the selected issue snapshot.

Success for this receipt is an explicit stop disposition that prevents a
duplicate requirement or scaffolding-only slice. It is not a claim that the
broader v2 program is complete.

## Pass 2 — RIPR vision and authority alignment

RIPR's product boundary remains producer-owned static evidence with explicit
limitations. `RIPR-SPEC-0130` identifies the authority map and separates
authored specifications, implementation slices, generated views, and live
work evidence. `RIPR-SPEC-0132` records the v2 runtime-promotion boundary and
keeps proposal, implementation, and support promotion distinct.

The canonical owner for a future requirement is the spec/requirement system,
not this handoff. A new requirement here would compete with the existing
authority map and slice registry.

## Pass 3 — ownership and seam discovery

The relevant seams are already explicit:

- specification and authority: `docs/specs/`;
- PR-local scope and claim boundary: `.allow/spec-system/slices/`;
- generated and checked projections: repository `xtask` commands and policy
  files;
- durable workflow explanation: `docs/swarm-development.md` and
  `docs/source-of-truth/README.md`.

The receipt is documentation-only. It does not write to the spec registry,
slice registry, generated projections, issue metadata, or runtime state.

## Pass 4 — state, failure, and abuse review

The stop boundary is required because forcing a proceed result would create
the following false promotions:

| Failure case | Disposition |
| --- | --- |
| Existing v2 authority treated as absent | reject as duplicate |
| Spec-only material treated as implemented behavior | reject; no source claim |
| Existing slice model treated as a new implementation slice | reject; no slice write |
| Stale or unknown external adapter generation | retain as an explicit limitation |
| Reordered or concurrently edited source inputs | invalidate this receipt and re-run |
| Generated view mistaken for authored authority | resolve through the authority map |
| Rollback requiring deletion of durable evidence | prohibited; preserve this receipt |

The receipt therefore chooses a non-proceed disposition and creates no
accepted requirement or implementation-slice file.

## Pass 5 — verification design

The positive case is not an implementation test. It is the disposition
invariant: a current checkout containing the v2 authority map and slice model
must not receive a second equivalent root artifact.

The discriminating negative cases are:

1. A weak evidence packet that only cites a plan or issue body is insufficient
   because it cannot establish the current owner or implementation boundary.
2. An irrelevant assertion that a v2 document exists is insufficient because
   it cannot prove source behavior or support promotion.
3. A strengthened assertion that the authority map, runtime boundary, and
   slice model all exist supports the duplicate/superseded disposition, but
   does not support a product-completeness claim.
4. A zero-subject or missing-current-head input must stop without creating a
   requirement or slice.
5. A changed head invalidates the receipt identity and requires a fresh
   disposition.

The receipt denominator is one root disposition. No implementation, test
adequacy, runtime execution, mutation, or release denominator is claimed.

## Pass 6 — PR slice and economy

The smallest coherent slice is this handoff receipt. Adding a parser,
validator, adapter, workflow runner, schema, or migration would expand the
issue beyond its current authority state and recreate infrastructure already
represented by the repository.

Deferred work remains with the open follow-on issues that own it. This PR adds
no package, workflow, schema, generated artifact, or source behavior.

## Pass 7 — independent challenge

Challenge result: `none_found` for the stop disposition.

The strongest contrary reading is that the issue remains open and therefore
must produce a proceed artifact. That reading fails the issue's own
fail-closed rule: when the original root is stale, already satisfied, or
conflicts with current authority, the correct output is a stop/disposition
receipt rather than forced materialization.

The challenge does not establish that every dependent issue is complete. It
only confirms that this root must not create a duplicate requirement or slice.

## Root disposition

`AlreadySatisfied` for the original walking-skeleton materialization premise;
`DuplicateOrSuperseded` for any attempt to create a second equivalent
requirement or implementation slice from this root.

Exact next action: route future work through the current authority map and an
explicit, narrower child issue with fresh evidence. Do not materialize from
this receipt alone.

## Claim boundary

This receipt proves a bounded current-state disposition for #1675. It does not
claim that RIPR-SPEC v2 is complete, that all dependent issues are closed, or
that any static evidence has become runtime behavior or release authority.
