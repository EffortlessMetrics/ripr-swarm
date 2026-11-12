# Formal-verification implementation plan

This plan composes existing issue and PR owners. It does not create a new
programme or reserve work already owned elsewhere.

## Live non-overlap constraints

- **PR #12581 / issue #12579** owns the Kani version-preflight deadline.
- **Draft PR #14469 / issue #13768** owns the current positive-cover admission
  tightening for critical async-bridge proofs.
- **Draft PR #11517 / issue #10686** owns declared Kani context, source identity,
  cache identity, and public inventory. It explicitly does not own
  owner/domain/observable semantics.
- **#12109** owns the self-verification claim boundary and deferred claims.
- **#12904** owns required-check admission and the assurance scoreboard.
- **#10745** owns the user-facing formal-guard integration programme.
- **#5224/#6452/#6456** own verification-authority transitions.
- **#6381/#6385/#6390** own generalized proof-family, applicability, and provider
  architecture.

New implementation should extend these owners, split them when reviewability
requires it, and avoid replacement branches for active candidates.

## Phase 0 — make the current Kani lane maintainable

### 0A. Finish preflight integrity

Owner: #12579 / PR #12581.

Acceptance:

- version discovery has an explicit deadline;
- timeout writes a typed not-established result;
- verification is never launched after failed discovery;
- child cleanup and retained report are bounded;
- current-head native qualification completes.

### 0B. Reconcile RIPR-SPEC-0371

Owners: #12109, #12904; coordinate with PR #14469.

Update the existing spec to describe:

- current fail-closed runner behavior;
- source census versus registered assurance;
- stable claim identity;
- machine result and raw artifact;
- required covers and negative controls;
- critical/full/canary CI phases;
- base/head authority transitions;
- required-check admission;
- exact nonclaims.

Do not mark the spec active until implementation and policy checks agree.

### 0C. Add machine result DTO

Owners: #12109/#12904.

Likely implementation seams:

- `xtask/src/kani_lane.rs`;
- a small typed Kani report module if needed to keep the runner reviewable;
- `target/ripr/reports/kani.json`;
- Markdown rendered from the same DTO;
- workflow artifact upload of JSON, Markdown, and bounded raw evidence;
- focused parser, currentness, and parity tests.

Acceptance:

- complete expected claim/subject inventory;
- exact process and cleanup state;
- duplicate/missing/stale rows reject;
- JSON/Markdown parity;
- atomic publication;
- source-order independent normalized result.

### 0D. Add stable claim registry

Owners: #6381/#5198/#12109.

Start with critical self-claims only. Do not attempt every proof family in the
first slice.

Acceptance:

- unique ID and generation;
- proposition, owner, domain, observable, trust, bounds, covers, negative
  control, phase, and nonclaims;
- generated source and assurance inventories;
- unregistered harnesses cannot increase registered coverage;
- passive validation executes no provider.

## Phase 1 — complete one end-to-end Kani product path

### 1A. Qualify or split PR #11517

Keep that candidate bounded to context/source/cache/public inventory. Carry
unaffected work rather than recreating it.

### 1B. Implement #12596

Produce an exact Kani proof relationship:

```text
harness/claim
→ production owner
→ symbolic domain
→ asserted return/effect/observable
→ trust and assumption boundary
```

Required controls:

- wrong owner and same-name sibling;
- local shadow;
- trusted/stubbed implementation;
- unrelated observable;
- missing or ambiguous domain;
- inactive configuration;
- multiple candidate owners.

### 1C. Implement #12512 and #10749

Preserve affected behavior when only the proof, property, assumption, contract,
selection, model, bound, or trust changes.

Required transitions:

- assertion deletion;
- assumption narrowing or contradiction;
- cover deletion;
- lower bound;
- changed stub/trust;
- changed owner;
- changed provider/configuration;
- legitimate strengthening and intended partial domain;
- unsupported comparison.

### 1D. Implement #10687

Initial bounded findings:

- unreachable/contradictory domain;
- no supported discriminator for changed observable;
- owner mismatch;
- trusted result path;
- assertion independent of symbolic input;
- guard weakened or scope reduced;
- comparison unknown.

Static findings never claim a native result or general specification adequacy.

### 1E. Implement #12597 / complete #10689

Correlate an imported observation only when source closure, claim generation,
owner/domain/observable, provider/configuration, and artifact identity match.

Retain external-unattested provenance where applicable.

### 1F. Complete #12598 Stages B and C

- native correct/wrong/weak/restored Kani controls;
- exact denominators and raw evidence;
- installed public command journey;
- wrong source/owner/configuration/currentness controls;
- separate static, native, and adequacy states.

### 1G. Complete #10746 adversarial corpus

Include false reassurance, false actionable, missed evidence, honest
abstention, and unsupported cases. Do not derive truth from ripr output.

## Phase 2 — close self-verification claim families

Refactor #12109 into:

1. classification/actionability;
2. gate/exit/result aggregation;
3. currentness/identity/causal comparison;
4. resource/budget safety;
5. edit/repair final composition.

Each critical claim requires current native evidence, covers, negative control,
and explicit nonclaim.

Where Kani is not economical, register the alternative evidence owner rather
than keeping a chronic timeout or copied core.

## Phase 3 — assurance scoreboard and CI

### 3A. Generate assurance scoreboard

Join:

```text
registered claim
+ source mapping
+ current exact native result
+ required covers
+ negative control
+ owning spec
+ alternative/manual evidence owner
```

Keep the existing source census separate.

### 3B. Add affected critical selection

Select by changed owner/source closure, claim/harness/model, runner/provider,
configuration/result law, or verification policy. Unknown impact selects all
critical claims.

### 3C. Add advisory `kani-critical`

Measure before required admission. Record why each claim was selected and
whether all-critical fallback occurred.

### 3D. Shard full lane by measured cost

Preserve exact inventory, one run identity, and fail-closed aggregate.

### 3E. Decide required status

#12904 applies the admission criteria in [CI.md](CI.md). Harness count or one
successful run is insufficient.

## Phase 4 — qualify the repair-lifecycle TLA+ pilot

The source-only pilot on
`codex/repair-lifecycle-model-20261106` needs:

1. Rust adapter build/tests;
2. native positive TLC run;
3. all three named negative configurations and counterexamples;
4. exact model/config/tool/source commitments;
5. timeout/malformed/stale controls;
6. state/action-to-Rust mapping;
7. six-case bounded conformance comparison;
8. current-main port and protected integration.

Model safety, implementation conformance, and Rust implementation correctness
remain separate claims.

## Phase 5 — verification-authority transition

Implement #6452 then #6456/#5224:

- compare base/head claim, role, owner, domain, trust, bound, cover, provider,
  result law, and CI phase;
- retain old applicable protection during the changing PR;
- require typed replacement or retirement;
- expose ambiguity and lost coverage;
- feed the transition into critical claim selection.

Complete this before required Kani admission.

## Phase 6 — Verus

1. Finish body-level `assume`/`admit` and trust facts.
2. Complete static output/corpus parity.
3. Implement #10748 native adapter.
4. Add wrong-owner, trusted-stub, zero-obligation, stale, and incomplete
   controls.
5. Run #10753 verus-spec-check experiment.
6. Exercise one same-behavior Kani/Verus journey without claiming equivalence.

## Phase 7 — focused specialized providers

Implement #6381 → #6385 → #6390 before adding provider lanes.

A provider admission packet contains:

- exact proposition/family;
- semantic selection trigger;
- exact subject/model/corpus and denominator;
- provider/version/result contract;
- source and process policy;
- positive and negative corpus;
- cost/resource bounds;
- support and nonclaim wording;
- rollback.

Candidates include Miri, Loom/Shuttle, deterministic fuzz replay, targeted
mutation, and exact branch/condition coverage. There is no run-every-tool lane.

## Overall acceptance

The programme reaches the first maintained milestone when:

- lane/spec/result/claim contracts agree;
- critical self-claims have current falsifiable evidence;
- one installed user-facing Kani journey is complete;
- guard-only changes remain affected;
- the assurance scoreboard is generated;
- critical selection is conservative and measurable;
- unsupported and not-established states remain visible;
- required status, if any, is separately admitted.
