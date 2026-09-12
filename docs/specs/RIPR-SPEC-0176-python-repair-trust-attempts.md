# RIPR-SPEC-0176: Python repair-trust selection and attempt semantics

Status: proposed

Owner:

Created: 2026-09-10

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #3569 (bind the two-phase repair driver to durable attempt identity)
- #3568 (define immutable repair-trust selection and attempt semantics)
- #3557 (parent capability)
- #3555 / #3556 (the judged-panel case authority and the eval-sweep subject
  authority these selections draw from)
- #2927 (attempt convergence)

Linked PRs:

Support-tier impact:

- No tier change. This spec defines metadata validation for a governed
  Python repair corpus; it produces no analyzer behavior, no runtime
  evidence, no gate, no badge, and no support claim.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- New xtask command `python-repair-trust check` registered in the command
  mutability catalog as a non-mutating check writing only
  `target/ripr/reports/python-repair-trust-check.{json,md}`.
- The product (`ripr` crate) gains the driver binding surface only:
  `agent repair` trust/authorization flags, the staged binding artifact, and
  the apply record. No new public API items; the binding module is
  crate-private.
- New `python-repair-trust check-driver` subcommand registered in the command
  mutability catalog as a non-mutating check writing only
  `target/ripr/reports/python-repair-driver-check.{json,md}`.

## Problem

External Python repair trust needs a denominator fixed before outcomes are
known and one typed lifecycle tying selected cases to later edits, execution,
movement, review, and rollback. Without it, difficult attempts can disappear,
partial work can look complete, and totals can be hand-entered.

## Behavior

One crate-private selection/attempt model and semantic validator is exposed
as `cargo xtask python-repair-trust check [--manifest <path>]
[--attempts <dir-or-file>]`. Ordinary CI validates accepted metadata offline:
no repository materialization, no RIPR execution, no external commands, and
no filesystem lookups beyond the supplied artifact files themselves (an
external authority's bytes are digested as recorded, never re-read).

### Selection manifest (the accepted denominator)

The selection manifest (`python_repair_trust_manifest`, schema `0.1`,
spec `RIPR-SPEC-0176`) fixes the denominator before outcomes are known. Every
selection row carries, with a deny-unknown schema:

- identity: `attempt_id` (unique across the manifest), `case_id`,
  `subject_id`;
- repository pins: `repository` (https), `base`/`head` (40-char git SHA),
  optional `tree` (sha256), optional `source_currentness` (the RIPR-SPEC-0151
  vocabulary, reused — never a private one);
- selection intent: `selection_reason`, `diversity_stratum`;
- native Python behavior identity: `family`, `owner`, `discriminator`,
  `relation`, `oracle`, optional `limitation`. No Rust-side conversion
  vocabulary (`SeamKind`) may appear anywhere in a corpus value;
- expectation: `expected_direction` (the RIPR-SPEC-0092 direction
  vocabulary), `claim_boundary`;
- target: `target_path` (portable repo-relative) and `target_state`
  (`existing`/`proposed`/`ambiguous`/`unavailable`/`unsafe`);
- provenance: `selected_at` (ISO-like date prefix), `selector`,
  `authority_snapshot_digest` (sha256 over the exact authority-snapshot
  bytes the row was selected from, recorded at selection time —
  well-formed here; binding to external bytes is a selection-time concern);
- immutability: `selection_digest` (the row-content digest defined below),
  which must equal the checker's recomputation. Any replacement or edit of
  a selected row moves the digest and fails.

A target path under a denied production/generated/vendor/environment surface
prefix (`target/`, `dist/`, `build/`, `vendor/`, `vendored/`,
`node_modules/`, `generated/`, `__pycache__/`, `site-packages/`, `.venv/`,
`venv/`, `env/`, `.tox/`, `.eggs/`, or any component carrying
`.generated.`) must be declared `target_state: unsafe`. Ambiguity,
unavailability, and unsafety are states, not errors.

Optional identities (`tree`, `source_currentness`, `limitation`) are either
absent (typed `incomplete`, disclosed, never invented) or well-formed: an
explicit null or a malformed value fails.

### Digest definitions (canonical, each defined once)

Two digest names carry the immutability bindings; each is defined exactly
once, here, and every other reference uses the name as defined.

- serialization contract: the `selection_digest` preimage is the row JSON with
  only `selection_digest` removed — every other field (including
  `authority_snapshot_digest`) remains — serialized as compact UTF-8 JSON with
  sorted object keys and serde_json default string escaping. Producers hash
  exactly these bytes; pretty-printed or differently escaped serializations of
  the same logical row do not validate.
- `manifest_digest` is sha256 over the EXACT manifest file bytes — no
  canonicalization, no reserialization. This one definition covers both
  envelope-level binding, where the envelope's recorded value must equal
  the checker's recomputation over the presented selection-manifest bytes
  (the stale-digest check). Each selection row's recorded
  `authority_snapshot_digest` is the same digest over the exact
  authority-snapshot bytes the row was selected from, recorded at selection
  time (its bytes are digested as recorded, never re-read by the offline
  check); `manifest_digest` is reserved for the envelope binding alone.
- `selection_digest` is distinct: sha256 of the row's canonical content,
  defined as the JSON serialization the checker recomputes (the row
  without its digest field, re-serialized with sorted object keys). Any
  replacement or edit of a selected row moves this digest and fails.

### Attempt envelopes (the typed lifecycle)

Attempt envelopes (`python_repair_trust_attempts`, schema `0.1`) bind to the
selection manifest by `manifest_digest` (the canonical manifest-level digest
defined below; a changed manifest fails the stale-digest check). `--attempts`
accepts one envelope file or a directory of
`*.json` envelopes (sorted); attempt identities are unique across the loaded
corpus.

Each attempt row carries:

- `attempt_id`, which must name an existing selection row: a missing
  reference is a deleted or replaced selection, never a new freedom;
- `states`, the ordered lifecycle progression over `selected`/`eligible`/
  `started`/`edited`/`verified`/`reviewed`/`accepted`/`stale`/`rejected`/
  `abandoned`: it starts at `selected`, passes through each state once,
  keeps the progress states ordered (each of `edited`/`verified`/`reviewed`/
  `accepted` requires its predecessor), and ends at a discard terminal if
  one is present;
- optional `supersedes`: a refresh appends a new record under a new attempt
  identity linked to a retained record that ended `stale`. Superseding a
  non-stale or unknown record mutates history instead of appending, and two
  records superseding the same stale record leave two current successors —
  both fail;
- `movement` (optional): terminal static movement
  `closed`/`improved`/`unchanged`/`regressed`/`limited`/`stale`/`uncertain`;
- `execution` (optional): verification execution
  `passed`/`failed`/`timed_out`/`cancelled`/`unavailable`/`not_run`/
  `invalid`;
- identity blocks: `analyzer` (`source_sha`, `binary_digest`), `config`
  (`profile`), `input` (`digest`), `patch` (`digest`), `command`
  (`verification_command`), `after_state` (`tree_digest`), `packet`
  (`reference`). Source and target identities live on the selection row.

### Validation rules (fail closed; each pinned by a test)

1. Selected rows cannot be deleted or replaced after outcome: unknown
   attempt references fail, replaced rows fail their recomputed digest, and
   a manifest changed under a bound envelope fails the stale-digest check.
2. Native Python behavior identity remains authoritative: `SeamKind`
   appears nowhere in a corpus value.
3. Source/analyzer/config/input/packet/target/command/patch/after_state
   identities are required as lifecycle advances: `started` requires the
   analyzer/config/input identities, `edited` the patch digest, `verified`
   the command and after_state identities, `reviewed`/`accepted` the packet
   reference. Missing identities at their transition fail closed — they are
   never invented and never excused. A supplied identity block is
   shape-validated (deny-unknown fields, well-formed value types) at any
   lifecycle position: only requiredness waits for the transition.
4. Static movement and command execution cannot imply one another: the axes
   are separate fields with no cross-axis derivation. A recorded movement
   requires an edited lifecycle (movement is a before/after comparison of an
   edit, never a consequence of a run); a verdict execution (`passed`/
   `failed`) requires the `verified` state; a non-verdict execution
   contradicts a claimed `verified` state. An improved movement never
   requires a passed run, and a passed run never forces improved — both
   pairings stay representable.
5. Partial/stale/abandoned attempts cannot appear completed: a
   `stale`/`rejected`/`abandoned` terminal forbids the completed lifecycle
   states (`verified`/`reviewed`/`accepted`) and the completed movements
   (`closed`/`improved`).
6. Production/generated/vendor/environment edit surfaces are forbidden: an
   attempt that edits a declared-unsafe target fails.
7. Aggregates are derived from rows: with rows present the full owned
   aggregate set is required — an omitted field (the selected denominator
   or a diversity aggregate included) would silently disable its
   row-agreement check. `attempts_total`, `lifecycle_counts`,
   `movement_counts`, `execution_counts`, and `achieved_strata` must equal
   the row-derived values exactly (both directions; hand-edited totals and
   fabricated keys fail), `selected_denominator` must equal the manifest's
   selection count, and with zero attempt rows a recorded nonzero aggregate
   is a fabricated claim. `stratum_floor` with `stratum_floor_met` must
   equal the derived comparison: achieved diversity is reported without
   pretending the target floor was met.
8. Historical attempts are immutable: duplicate attempt identities fail;
   a refresh appends a new record under a new identity through `supersedes`.

### Verdict and outputs

The verdict vocabulary is `valid`/`incomplete`/`not_run` — selection and
lifecycle structural validation only, never a support-tier,
repair-correctness, gate, badge, or promotion claim. `not_run` (no attempts
supplied, a zero-row corpus, or no accepted manifest at the default path)
exits 0 and is never a vacuous pass. `incomplete` discloses absent optional
selection identities in full while exiting 0. Any fail-closed violation
exits nonzero with a diagnostic naming subject/field/reason plus the
deterministic rerun command. The check writes a versioned report
(`schema_version` `0.1`, kind `python_repair_trust_check_report`) to
`target/ripr/reports/python-repair-trust-check.{json,md}` and nothing else.

## Threat model

The checker enforces structural consistency at the accepted manifest state;
it does not attempt provenance against a fully rewritten corpus.

- The accepted selection manifest's digest must be recorded externally at
  selection time — in the governing issue or receipt. For the first
  cohort, that external record lives in the #3557 governed-cohort issue.
- Validation binds each envelope to the manifest bytes presented to the
  checker (`manifest_digest`, defined above): any rewrite of the manifest
  — or of a selected row — changes the canonical digest, so a rewritten
  corpus no longer equals the externally recorded accepted digest and is
  detectable against that record.
- An editor who rewrites the manifest and recomputes every internal digest
  produces a different, internally consistent corpus; detecting that
  against the external record requires a signed external anchor, which is
  out of scope for offline structural validation.

## Required Evidence

- Data-driven acceptance: alternate valid temporary corpora (different
  identities, strata, directions, and lifecycle shapes) validate — the
  validator is not pinned to any retained cohort.
- Each fail-closed rule above is pinned by a named test that mutates a valid
  corpus and asserts the failure names the expected reason plus the rerun
  command.
- Wrong-target (ambiguous), invalid-command (`invalid` execution), stale,
  unsafe, regressed, limited, uncertain, and abandoned states remain
  representable inside a valid corpus.
- Zero golden drift: the check writes only new `target/ripr/reports/`
  artifacts and changes no `ripr` command output.
- Offline proof: the command performs no process spawns and no network
  access (pinned by the process/network policy gates).

## Non-Goals

- No edit application, verification execution, cohort run, analyzer fix,
  gate, badge, tier change, release, or publication action.
- No claim that any repair is completed or correct: completion establishes
  the immutable denominator and state machine for governed Python repair
  attempts only.
- No producer yet: no command writes selection manifests or attempt
  envelopes in this slice; the validator accepts the metadata contract ahead
  of producers.
- No unification with the Rust repair-trust corpus format; the Rust
  predecessor keeps its own report shape.

## Driver Binding (issue #3569)

The two-phase external-edit driver (`ripr agent repair`, #2443) binds one
durable repair attempt (#2927) to one accepted selection row by digests, never
by names. The bridge extends the driver; it does not create a Python-only
lifecycle and does not add a second attempt ledger: the durable attempt
manifest remains the sole ledger, and the binding rides in it as a staged,
digest-pinned artifact (role `python_repair_trust_binding`).

### Prepare phase (binding before editing)

`ripr agent repair --phase before` accepts
`--python-repair-trust-manifest <path>`, `--python-repair-trust-attempt <id>`,
and the authorization pair `--edit-authorized` + `--edit-authority <identity>`
(both flags or neither; an authorization without a binding, or a binding
without an authorization, is refused). Before the durable attempt is
published, the driver verifies, failing closed:

- the manifest envelope (`schema_version` `0.1`, `kind`
  `python_repair_trust_manifest`, spec `RIPR-SPEC-0176`) parses as strict JSON
  (duplicate keys fail) and its exact bytes digest to the recorded
  `selection_manifest_sha256`;
- the requested row exists in the selection denominator and its canonical
  `selection_digest` recomputes to the recorded value (a replaced or edited
  row requires a new selection);
- the row pins `head` equal to the repository's current HEAD (a stale
  selection fails before editing);
- `target_state` is `existing` (proposed/ambiguous/unavailable/unsafe targets
  require a new or re-authorized selection) and the target path is portable
  and test-only: a target under a production/generated/vendor/environment
  surface prefix, or any component carrying `.generated.`, is refused;
- the row target agrees exactly with the repair packet's selected edit target
  (identity alignment, never name similarity), and resolves to exactly one
  file in the repository inventory (zero matches and multiple case-insensitive
  matches both fail before editing);
- the authorization is explicit (`granted` under the
  `explicit-operator-flags` method with a named authority).

The verified record is staged into the durable attempt as a digest-pinned
artifact and mirrored at
`target/ripr/workflow/python-repair-trust-binding.json`. It retains the
trust identity (attempt, case, subject, repository, base, head, optional tree
and source-currentness, family, owner, discriminator, relation, oracle,
optional limitation), the digest anchors, the analyzer identity (running
binary digest and producer version), the config profile (real producer: the
analyzed root's `ripr.toml` presence), the input digests (packet, before
snapshot), the declared edit surface (allowed and forbidden paths), the
authorization, and the standing non-claims. The record carries no timestamps:
equivalent preparation is byte-identical, and the manifest location is the
declared telemetry.

### Apply phase (recording the applied edit)

`ripr agent repair --phase after` re-verifies the retained binding by digest
immediately before the applied edit is recorded: the selection manifest must
still digest to the pinned value, the row must still digest to its recorded
`selection_digest`, the target must still agree with the packet, the retained
packet must still digest to the pinned value, and the invocation must
re-affirm the retained authorization with the same authority. Any drift fails
before the durable attempt advances. Repository drift (HEAD movement) is
deliberately NOT re-refused here: it is owned by the durable attempt
authority, whose finish records the typed `stale` state.

After the durable finish, the apply record
(`target/ripr/workflow/python-repair-driver-after.json`) is published last
(the receipt re-evaluates the edit cage over the exact delta finish measured,
so no artifact write may land between finish and the receipt binding; a
receipt refusal still publishes the record). It carries the durable attempt
identity, the prepare-record digest chain, the patch digest, the actual
changed-file set, the edit-cage decision, the resulting repository head, and
the same non-claims. The apply record is a compatibility projection; the
durable `after` block in the attempt manifest remains the authority.

### State discipline and claim boundary

The driver keeps the durable states distinct and maps them onto the #3568
lifecycle vocabulary without claiming any of its completed states:
`awaiting_edit` is prepared-but-not-applied (the corpus `started` identity
inputs ride in the binding record), `ready_to_finish` is
applied-but-unverified, `failed` with a violated cage verdict is rejected and
retained, and `stale` is drifted. The driver claims no verification result,
no static movement, and no closure; lifecycle, movement, and execution fields
never appear in a driver record (the `check-driver` validator denies them),
and #3570 owns the verification phase. No verification result, static
movement, closure, support, gate, or badge is inferred from any driver
record.

The driver executes no arbitrary command: its edits are performed by the
human or external agent between the phases, and the driver itself only
writes its own artifacts.

## Acceptance Examples

- A corpus with six selections across four strata and six attempt rows —
  including a completed accepted attempt, a failed verification with an
  unchanged movement, an abandoned edit with an uncertain movement, a stale
  selection with a not_run execution, and a stale historical record refreshed
  under a new identity — validates with verdict `valid` when every optional
  identity is recorded and every aggregate is row-derived.
- Deleting a selection row that an attempt references fails with
  "outside the accepted selection denominator".
- Hand-editing `attempts_total` (or any count-map entry) fails with
  "hand-edited aggregate".
- Recording `movement: improved` on an attempt whose lifecycle never edited
  fails: movement cannot be implied by a verification run.
- An attempt that edits a target declared `unsafe` fails.

## Test Mapping

- `xtask/src/reports/python_repair_trust.rs::python_repair_trust` — the
  validator test module (data-driven acceptance, every fail-closed rule,
  verdict precedence, deterministic report rendering). Listed in
  `.ripr/traceability.toml` under this spec.
- `xtask/src/reports/python_repair_driver.rs::python_repair_driver_binding` —
  the driver binding validator test module (digest anchors, target identity
  agreement, denied surfaces, authorization, non-claims, apply-phase shapes,
  end-to-end file and directory inputs). Listed in
  `.ripr/traceability.toml` under this spec.
- `crates/ripr/tests/python_repair_attempt.rs` — the end-to-end driver
  binding case matrix (clean test-only positive, stale packet, wrong target,
  zero and ambiguous target matches, denied surfaces, outside-root manifest,
  missing or mismatched authorization, tampered retained binding, cage escape
  and production/generated edits, deterministic preparation, state
  distinctness). Listed in `.ripr/traceability.toml` under this spec.

## Implementation Mapping

- `xtask/src/reports/python_repair_trust.rs` — the crate-private model and
  semantic validator (selection manifest, attempt envelopes, aggregates,
  check entry point, report rendering).
- `xtask/src/command.rs` — `python-repair-trust` parse arm, command-catalog
  entry, and help listing.
- `xtask/src/dispatch.rs` — dispatch to the reports adapter.
- `crates/ripr/src/app/python_repair_binding.rs` — the crate-side binding
  authority: selection-manifest verification, prepare-record rendering,
  apply-phase re-verification and apply-record publication.
- `crates/ripr/src/cli/agent.rs`, `crates/ripr/src/cli/mod.rs`,
  `crates/ripr/src/cli/commands/agent.rs` — the driver CLI surface (trust and
  authorization flags, before-phase publication hook, after-phase
  re-verification and record publication).

## Metrics

- `python_repair_trust_check_verdict` — the disclosed verdict
  (`valid`/`incomplete`/`not_run`) written to the check report; descriptive
  only, never a gate input.
- No rates are derived: no denominator-based rate is meaningful for
  metadata-only validation, and none is emitted.
