# Sibling tools and bidirectional learning

`ripr` is one of a series of deterministic, fast, useful static PR tools that
share interfaces, are composed by the same CI gate, and deliberately learn from
each other. Each is cheap, runs on a diff, and emits trusted coverage artifacts
without executing repository code or issuing a verdict. This mirror makes the
relationship explicit for ripr contributors and points back to the canonical
ledger in [`unsafe-review-swarm`](https://github.com/EffortlessMetrics/unsafe-review-swarm/blob/main/docs/interop/sibling-tools.md).

## The family

| Tool | Repo | Role (coverage instrument) |
|---|---|---|
| `unsafe-review` | `EffortlessMetrics/unsafe-review-swarm` | unsafe-contract coverage: which unsafe seams are reviewable, what evidence exists/is missing |
| `ripr` | `EffortlessMetrics/ripr-swarm` | mutation / weak-oracle exposure coverage |
| `cargo-allow` | `EffortlessMetrics/cargo-allow` | owned exception ledger (unsafe/panic/lint/etc. allow entries) |
| `tokmd` | `EffortlessMetrics/tokmd-swarm` | token-aware repository receipts and PR context packets |
| `ub-review` | `EffortlessMetrics/ub-review` | CI gate — composes the family and LLM lanes; owns PR analysis, review, posting, and the blocking decision |

Each family member is a deterministic, fast, static PR tool — an instrument.
`ub-review` is the CI gate built on top: it composes configured sensors and
reviews their coverage artifacts. None of the family members is itself the gate
or the LLM reviewer. They are complementary, and a capability proven in one is
expected to flow to the others.

## Shared contracts

The family is aligning on these interfaces so `ub-review` can route sensors
uniformly instead of special-casing each one:

- **Sensor CLI shape:** `--root`, `--base`, `--diff`, `--head`, `--format`, and
  `--out` carry repository and PR context consistently.
- **Versioned gate manifest:** `<tool>-gate.json` provides `schema_version`,
  status, summary counts, artifact pointers, and a trust boundary. Consumers
  route by schema and dialect, not by scraping stdout or Markdown.
- **Ledger evidence taxonomy:** typed evidence prefixes, a policy dialect
  marker, ownership, classification, and lifecycle fields keep cross-repository
  references machine-checkable.
- **Coverage movement:** `new`, `worsened`, `resolved`, and `inherited` describe
  posture against a baseline; the orchestrator decides whether it blocks.
- **Trust-boundary discipline:** sensors remain advisory by default and state
  what their evidence does not prove. A receipt is not execution, mutation,
  runtime coverage, or a merge verdict.
- **Spec lifecycle:** machine-checked status and wording contracts prevent
  support and no-finding claims from drifting beyond their evidence.

## Ripr's contribution and adoption

| Direction | Contract | Evidence / owner | Status |
|---|---|---|---|
| ripr teaches the family | Canonical `new_unsuppressed` counter for threshold consumers | [ripr-swarm #1038](https://github.com/EffortlessMetrics/ripr-swarm/issues/1038) | landed |
| ripr adopts | Versioned gate manifest and baseline-debt-delta shape | [unsafe-review-swarm #1522](https://github.com/EffortlessMetrics/unsafe-review-swarm/issues/1522) | landed |
| ripr adopts | Machine-checked spec-status dashboard and wording verifier | [ripr-swarm #1040](https://github.com/EffortlessMetrics/ripr-swarm/issues/1040) | landed |
| ripr adopts | Diff-first downstream consumer contract | [ripr-swarm #1041](https://github.com/EffortlessMetrics/ripr-swarm/issues/1041) | open |
| ripr adopts | Pre-guard scratch garbage collection for shared CI runners | [unsafe-review-swarm #1519](https://github.com/EffortlessMetrics/unsafe-review-swarm/issues/1519) | landed |

These rows are the ripr-facing projection of the shared ledger. When a
cross-repository item lands, update this mirror and the canonical sibling table
in the same change or link the landing change from both sides.

## Bidirectional learning ledger

The canonical table lives in
[`unsafe-review-swarm/docs/interop/sibling-tools.md`](https://github.com/EffortlessMetrics/unsafe-review-swarm/blob/main/docs/interop/sibling-tools.md).
Its live rows currently cover:

- versioned gate manifests and baseline-debt deltas;
- multi-mode gates and the canonical `new_unsuppressed` counter;
- typed exception-ledger evidence and the `schema_version` convention;
- spec-status and no-finding wording verification;
- coverage-movement vocabulary;
- packet input-schema ownership and downstream consumption;
- limited-runtime vocabulary; and
- pre-guard scratch cleanup on shared runners.

The receiving repository owns the tracking issue. A shared contract is
co-designed across both repositories rather than copied unilaterally.

## Standing process

When work in one tool surfaces something a sibling should learn:

1. File the issue in the receiving `-swarm` repository with direction, concrete
   evidence, and a one-line proposal.
2. Cross-link the sibling issue when the contract is shared.
3. Add or update the ledger row in both mirrors when the item is live.
4. Do not duplicate an existing issue; add the concrete contract to its thread.

## Trust boundary

Cross-pollination changes interfaces and rigor, not claims. Every sibling tool
stays within its own trust boundary and advisory posture. Sharing a manifest or
ledger schema never lets one tool assert another tool's proof.
