# Native formal-result contract

A native tool process and a formal proposition are different things. The
result contract preserves process completion, provider-native outcomes,
registered claim satisfaction, source applicability, and trust separately.

The draft schema is
[`schema/native-proof-run-v1.schema.json`](schema/native-proof-run-v1.schema.json).

## Run identity

A native run binds at least:

```text
schema and run ID
repository and root identity
revision/tree or captured worktree identity
source-closure digest
claim-registry generation
provider, toolchain, solver, target, and feature identity
runner generation and command digest
preflight, whole-run, and per-subject deadlines
expected claim and subject inventory
native process completion
raw output artifact digest/reference
```

A self-reported hash proves byte equality, not trusted execution. Governed
producer custody and unattested imported observations remain distinct.

## Result identity

Each row binds:

```text
claim ID and generation
harness/model locator
exact source digest
provider-native verdict
required-cover result
assumption and bound digest
duration/resource telemetry
semantic claim status
currentness
rejection reasons
raw output block reference
limitations and nonclaims
```

One successful native block cannot satisfy two ambiguous source declarations.
Duplicate source identities, duplicate result blocks, missing expected rows,
and neighboring same-name results are not first-match-wins.

## Status law

### `ESTABLISHED`

All of the following hold:

- exact current claim generation;
- exact current source closure and provider/configuration identity;
- complete expected inventory;
- normal native process completion;
- provider reports the required successful outcome;
- all required covers are reachable;
- no ambiguity, duplicate, stale input, unsupported status, or result-parser
  mismatch.

### `FAILED`

The provider establishes a counterexample, invariant/assertion failure, or an
unreachable required cover for the exact current claim.

A compile failure, tool crash, timeout, or missing result is not a behavioral
counterexample.

### `NOT_ESTABLISHED`

The claim could not be established because evidence is missing, ambiguous,
incomplete, timed out, abnormal, zero-subject, malformed, or otherwise
insufficient.

### `STALE`

The evidence is parseable but belongs to incompatible source, claim
generation, model, assumptions, bounds, provider, configuration, or result
law.

### `UNSUPPORTED`

The proposition applies, but no admitted provider/route supports the subject.

### `NOT_PROVEN`

Available facts do not justify a stronger state, and no more specific
instrument failure applies.

The existing public three-state Kani command may represent `STALE` and
`UNSUPPORTED` as typed `NOT-ESTABLISHED` reasons until a versioned public
schema explicitly expands the vocabulary.

## Process law

- The version preflight has its own deadline.
- Every provider process has a whole-run deadline.
- Expensive subjects may have individual deadlines.
- Nonzero, absent, killed, or timed-out completion cannot yield a successful
  aggregate from favorable-looking text.
- Completed exact rows may remain visible after a later failure.
- Missing expected rows remain not established.
- `--allow-empty` is a local diagnostic affordance only; it cannot bypass a
  real process or result-integrity failure.
- Cleanup state and retained residue are reported separately.

PR #12581 owns the remaining Kani version-preflight deadline. The already
delivered ambiguous-attribution and abnormal-completion repairs must be
preserved.

## Inventory law

The run carries both:

- expected registered claims/subjects; and
- discovered native subjects/results.

A complete successful run requires an exact compatible mapping. The aggregate
refuses:

- missing expected claims;
- duplicate claims or native rows;
- unexpected rows where the provider contract requires a closed inventory;
- zero selected subjects for a required run;
- shard overlap or incomplete shard union;
- inconsistent source/result generation.

## Artifact law

The machine-readable artifact is authoritative. Human Markdown is rendered
from the same DTO.

Publication requirements:

1. write to a fresh run-owned temporary path;
2. validate complete DTO and inventory;
3. atomically publish the final machine artifact;
4. render Markdown from the validated DTO;
5. retain raw provider output or a content-bound artifact reference;
6. never select a mutable “latest” file as current evidence;
7. preserve interrupted/stale prior output as historical, not current.

## Static and native projection

User-facing output keeps these independent:

```text
static guard relation
native provider result
source/currentness applicability
specification-adequacy experiment
receipt/provenance class
```

Examples that must remain representable:

- static relation exact + native result not observed;
- static relation unknown + native result established for a harness;
- static relation exact + native counterexample;
- native success + weak or narrowed guard;
- current static fact + stale native result;
- external unattested observation + exact source bytes.

No axis overwrites another.

## Machine and human parity

The following derive from one normalized DTO:

- JSON result;
- Markdown report;
- CI summary;
- generated assurance scoreboard;
- CLI, MCP, editor, and PR projections that advertise support.

Each surface requires its own route test before parity is claimed. Fixture
success on one surface does not promote the others.
