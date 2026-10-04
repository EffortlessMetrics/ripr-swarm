# RIPR-SPEC-0236: Seam grip classification order: opaque and activation_unknown

Status: proposed

Owner: product / analysis

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #5411 (pilot ranks unresolved reach as the top gap)
- #5295 (mutation spot check)
- #5497 (withhold unresolved-reach limitations from the pilot top list;
  not adopted here, see Decision 6)

Linked PRs:

- #5569 (unresolved reach reads `opaque`, RIPR-SPEC-0230)
- #5946 (grade weak grip only when activation is established; open)

Amends RIPR-SPEC-0005 (classification order) and RIPR-SPEC-0230 (opaque
consequences and reach gaps).

Support-tier impact:

- No tier change. No grip class, `static_limit_kind` value or exposure
  class is added. Every class move in this spec goes from a gap class
  (`weakly_gripped`, `ungripped`) to an unknown class
  (`activation_unknown`, `opaque`). No seam gains credit and no seam
  moves to `strongly_gripped`. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- The classified-seam cache generations move once more after #5946 (full
  `1.33`, sharded and compact `0.39` if #5946 lands first at `1.32` /
  `0.38`).
- No schema or JSON field change. Pilot ordering is RIPR-SPEC-0237's.

## Problem

`classify_seam` (`analysis/seam_classification.rs`) maps five stage states
and the missing-discriminator list to one grip class. Its ten-rule order
exists only in the module doc comment. RIPR-SPEC-0005 lists the classes
and the headline table but no order. RIPR-SPEC-0230 refers to "rule 1" and
"rule 2" of 0005, which 0005 does not number. #5946 adds a 0005 section for
one boundary of the order only.

Measured on main `bcb0be576` with
`ripr check --root . --format repo-exposure-json` on one-file probe crates
(`m36/c1` to `m36/c10` in the session scratchpad), plus `ripr pilot` and
`--format repo-sarif` where shown:

| Probe | Stages R/A/P/O/D, missing | Class today | What the evidence supports |
| --- | --- | --- | --- |
| c1 `suffix(v)` boundary, run only through `impl Display for Plural`; test `assert_eq!(Plural(3).to_string(), "items")` | yes/unknown/yes/yes/yes, 1 | `weakly_gripped`, pilot #1, SARIF `warning`; evidence record `static_limitation` | activation not established; split verdict |
| c2 same `suffix`, test `assert_eq!(suffix(3), "items")` | yes/yes/yes/yes/yes, 1 | `weakly_gripped`; record `actionable_related_test_extension` | weak grip (correct) |
| c3 `suffix` called through `label(v)`; test `let _ = label(3);` | yes/unknown/unknown/no/no, 1 | `reachable_unrevealed`, pilot #1; record `static_limitation` | unrevealed: the test asserts nothing; record split from propagate and activate |
| c3 `label` in the same crate | yes/yes/unknown/no/no, 0 | `reachable_unrevealed`; record `static_limitation` | unrevealed; record split from propagate |
| c4 private `tier` behind `pub fn checkout`; `tests/checkout.rs` asserts `checkout(150)` | opaque/no/no/no/no, 1 | `opaque`; pilot lists it ("Actionable seams: 4 total", 3 of them opaque) | unknown; not a pilot gap |
| c5 `pub fn tier` with no test | no/no/no/no/no, 1 | `ungripped` | established negative (correct) |
| c6 = c4 with the test renamed `snapshot_checkout` | opaque/no/no/no/no, 1 | `opaque`; reach limitation category `snapshot_field_unknown` | category should be `opaque_static_evidence` |
| c10 = c4 plus an unreferenced `src/other.rs::tier` | opaque for both `tier` owners | `opaque` for both | the second owner shares the first one's witness by name |

c1 shows a split verdict: the grip class is a gap class (pilot rank 0,
LSP and SARIF warning), while the evidence record for the same seam says
`actionability.class = static_limitation` and
`recommendation.action = inspect_static_limitation`, because the activate
stage is `unknown`. Weak grip is a claim about which inputs drive the seam,
and that claim needs activation. #5946 and rule 5 remove this split. c3
also splits, but its class is right: `discriminate == no` (the test
asserts nothing) is an established negative whatever activation is. Its
`static_limitation` comes from the propagate stage too, and that split
stays (see Further rules). c6 shows that the evidence-record
category of an unresolved-reach limitation comes from substring matches on
its summary, so a test name decides it. c4 shows `opaque` seams ranked and
counted in pilot after every other ranked class, which matches #5569 and
RIPR-SPEC-0230 and stays.

## Behavior

### One authority

`classify_seam` stays the single classifier from `TestGripEvidence` to
`SeamGripClass`. Renderers, pilot, LSP, SARIF, badges and the evidence
record read the class; none of them reclassifies. `intentional` and
`suppressed` are reserved: `classify_seam` never returns them, and a later
governance pass may replace the natural class with one of them.

"Established activation" means `activate == yes`. `unknown`, `no`, `weak`
and `not_applicable` are not established.

### Classification order

The rules run in this order; the first match wins.

1. `reach == no` gives `ungripped`. Reach is `no` only when ripr
   established the negative (rule R below).
2. Any stage `opaque` gives `opaque`.
3. All five stages `yes` and `missing_discriminators` empty gives
   `strongly_gripped`.
4. `discriminate == no` gives `reachable_unrevealed`, whatever the
   activate stage reads.
5. `activate == yes` and (`discriminate == weak` or
   `missing_discriminators` non-empty) gives `weakly_gripped`. Today the
   rule does not read activation; #5946 adds `activate != unknown`, which
   equals `== yes` on every state the producer emits once rules 1 to 4
   have run.
6. `activate == unknown` gives `activation_unknown`. The seam keeps its
   `missing_discriminators` and its stage evidence.
7. `propagate == unknown` gives `propagation_unknown`.
8. `observe == unknown` gives `observation_unknown`.
9. `discriminate == unknown` gives `discrimination_unknown`.
10. Otherwise `opaque`.

Rules 1 to 4 and 6 to 10 are today's behavior and are normative as
written. Rule 5 is the change. In production the activate stage is `no`
only when there is no related test, and reach is then `no` or `opaque`,
so rule 5 sees only `yes` or `unknown`. A latent `activate` of `no`,
`weak` or `not_applicable` with reach not `no` and discriminate not `no`
skips rules 5 and 6 and reaches rules 7 to 10, which never yields a gap
class.

### Activate stage producer

The class depends on the activate producer
(`test_grip_evidence.rs`, `activate_evidence` and
`compact_activate_evidence`). Its current behavior is normative:

- No related test gives `no`.
- An ambiguous constructor-field owner, or an unresolved boundary constant
  or local, gives `unknown`.
- Observed activation values, or a direct or one-hop value-insensitive
  owner call, gives `yes`.
- Otherwise `unknown` (a helper hides the activation, or only a same-file
  or integration relation exists).

Missing discriminators (the #4214 boundary hint) are produced
independently of the activate state.

### Reach without related tests (rule R)

This section completes RIPR-SPEC-0230 for a seam with no related test.

- R1. The owner must resolve. When `owner_function(file, line)` finds no
  function, or the function's name is empty, reach is `opaque` (confidence
  low) with the summary "Reach unresolved (owner unresolved): ripr could
  not resolve the function that owns this seam, so no test path was
  searched; no gap is reported". Today reach is `no` and the seam reads
  `ungripped`. No probe produced this case (c9: seams in a `const`,
  `static` closure or associated const were not inventoried), so it is
  latent.
- R2. Witness order is RIPR-SPEC-0230's: transitive, macro, direct trait
  dispatch, dispatch root. The first witness found names the summary.
- R3. Limit kind. A transitive witness whose test file is a test file
  (`rust_index::is_test_file`: under `tests/` or `/tests/`) names
  `rust_integration_public_api_path_unresolved`; any other test file names
  `rust_transitive_reach_unresolved`. A macro witness whose macro is
  invoked in the test body names
  `rust_macro_wrapped_test_call_unresolved`; a macro further along the path
  names `rust_macro_reach_unresolved`. Trait dispatch reads
  `(trait dispatch)`. Owner unresolved reads `(owner unresolved)`.
- R4. Witness cache. The per-run cache key for the transitive and macro
  answers is the owner's display path plus its name
  (`name:{file}::{owner}`), not the name alone. The witness search itself
  stays name-based (RIPR-SPEC-0114), so the answer for two same-named
  owners is still the same today (c10); the key only stops a later
  path-aware search from leaking one owner's witness to another. Trait
  dispatch answers stay keyed by owner id.
- R5. When no witness fires and the owner resolved, reach is `no` with the
  summary "No static test path found for seam owner `X`".

### Evidence-record category of a reach limitation

The reach producer tags each unresolved-reach `StageEvidence` with a typed
source: `transitive_witness`, `macro_witness`, `trait_dispatch` or
`owner_unresolved`. `push_stage_limitation` maps a tagged stage straight
to category `opaque_static_evidence` and route
`analysis/static-limitation-taxonomy`. The summary-substring classifier
(`static_limitation_category`) runs only for untagged stages. A test,
entry or type name in the summary never decides the category. The tag is
crate-private evidence: it is cached with the seam and not added to any
public JSON. The `classification` / `opaque` entry that every `opaque`
seam carries is unchanged.

### Opaque in pilot

This spec owns class membership only; RIPR-SPEC-0237 owns pilot ordering.
`opaque` stays in the ranked set, as today: it is admitted to
`top_actionable_seams` and counted in `actionable_total`, at RIPR-SPEC-0237
rank 4, after every gap class and every unknown class. An opaque seam is
the Top Recommendation only when no other ranked class is present. The
agent brief also keeps admitting `opaque` at its lowest actionable
priority, because its route is `inspect_static_limitation`.

### Per-class consequences

Defaults come from `SeamSeverityConfig::default` (`config/model.rs`). Each
LSP severity is configurable under `[severity.seams]` with the key named
in the table (`off`, `info`, `note` or `warning`). SARIF maps `info` and
`note` to level `note`, `warning` to `warning`, and `off` to no result.
LSP maps `info` and `note` to `INFORMATION`. The LSP code is
`ripr-seam-<class with dashes>`; the SARIF rule is `ripr.seam.<class>`.

| Class | Rule | Headline | Pilot rank (RIPR-SPEC-0237) | Config key, default | Exposure counterpart | Repair route ceiling |
| --- | --- | --- | --- | --- | --- | --- |
| `strongly_gripped` | 3 | no | not listed | `severity.seams.strongly_gripped`, off | `exposed` | already gripped |
| `weakly_gripped` | 5 | yes | 0 | `severity.seams.weakly_gripped`, warning | `weakly_exposed` | none |
| `ungripped` | 1 | yes | 1 | `severity.seams.ungripped`, warning | `no_static_path` | none |
| `reachable_unrevealed` | 4 | yes | 2 | `severity.seams.reachable_unrevealed`, warning | `reachable_unrevealed` | none |
| `activation_unknown` | 6 | yes | 3 | `severity.seams.activation_unknown`, info | none | static limitation, "incomplete evidence stage: activation" |
| `propagation_unknown` | 7 | yes | 3 | `severity.seams.propagation_unknown`, info | `propagation_unknown` | static limitation, "... propagation" |
| `observation_unknown` | 8 | yes | 3 | `severity.seams.observation_unknown`, info | none | static limitation, "... observation" |
| `discrimination_unknown` | 9 | yes | 3 | `severity.seams.discrimination_unknown`, info | none | static limitation, "... discrimination" |
| `opaque` | 2, 10 | no (`ripr`: `unknowns`; `ripr+`: `unknowns_test_efficiency`) | 4 | `severity.seams.opaque`, info | none | static limitation, "incomplete evidence stage: opaque" |
| `intentional` | reserved | no | not listed | `severity.seams.intentional`, off | none | policy excluded |
| `suppressed` | reserved | no | not listed | `severity.seams.suppressed`, off | none | policy excluded |

Further rules:

- Pilot ranking does not read `[severity.seams]`. `for_seam` is called
  only from `lsp/diagnostics.rs`, `output/sarif.rs`,
  `output/badge/summaries.rs`, `app/agent_brief.rs`,
  `output/agent_brief.rs` and `output/review_comments.rs`; neither
  `output/pilot/` nor the seam inventory calls it. Setting a class to
  `off` removes its LSP diagnostic, SARIF result and badge count, not its
  pilot rank. The four unknown classes tie at rank 3; the existing tie-breakers
  (missing discriminators, related tests, suggested assertion, path, line,
  kind, id) order them.
- The grip class and the evidence-record actionability are separate axes.
  The class decides headline, pilot rank and severity. The evidence record
  decides repair guidance. After rule 5, a `weakly_gripped` seam always
  has established activation, so the activate stage alone never puts
  `weakly_gripped` beside `static_limitation`. A seam that rule 5 moves
  reads `activation_unknown` (rule 6) or, when an earlier rule matches,
  `opaque`.
- Known remaining split. Other `unknown` stages still put a gap class
  beside `static_limitation`. c3 `suffix` and `label`
  (`reachable_unrevealed`) and c4 `checkout` `call_presence`
  (`weakly_gripped`, pilot #1) all carry a `propagation_static_unknown`
  limitation, and c3 `suffix` also an activate one. Rule 4 keeps c3 by
  design. Resolving the propagate and observe splits is out of scope.
- An `activation_unknown` seam keeps its missing-discriminator guidance in
  the evidence `missing_discriminators` and in the evidence record's
  `recommendation.candidate_values`. It gets no suggested assertion,
  because its repair route is not repair-ready.

### Decisions

Steven delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Weak grip guard.** Adopted: `activate == yes`, stricter than #5946's
   `activate != unknown`, so "established" has one meaning. Production
   output is the same as #5946's today; only latent states differ.
   Rejected: `!= unknown`, which lets `no`, `weak` and `not_applicable`
   activation grade a weak grip.
2. **Activation guard scope.** Adopted: only `weakly_gripped` needs
   established activation. "Weak" claims that the tests' inputs miss a
   discriminator, which needs to know what inputs drive the seam.
   `discriminate == no` (a test that asserts nothing) is an established
   negative whatever activation is, so rule 4 stays unguarded, matching
   main and #5946. Measured by review on the 178 `fixtures/*/input` crates
   (1018 seams; `target/debug/ripr`, build revision not established):
   rule 5 moves 30 of 436 `weakly_gripped` seams to an unknown class. A
   rule 4 guard would have moved 2 of 9 `reachable_unrevealed` seams, the
   canonical no-op fixtures `property_macro_noop_named_test` and
   `property_macro_noop_property_assertion`, each a single seam with no
   outer owner left to surface the repair. No mutation spot check exists
   for either rule; #5946 reports that moving seams into unknown classes
   lowered pilot top-ten precision in a related trial. Rejected: guard
   rule 4 too.
3. **Unresolved owner.** Adopted: reach `opaque` (rule R1), because
   RIPR-SPEC-0230 requires an established negative for `ungripped` and no
   path was searched. Rejected: keep reach `no`.
4. **Witness cache key.** Adopted: owner file plus name (rule R4).
   Rejected: name only, which is correct today only because the witness
   search is name-based. A path-aware witness search is a separate
   precision change (ISSUES FOR ROOT).
5. **Reach limitation category.** Adopted: a typed source tag mapped to
   `opaque_static_evidence`, as RIPR-SPEC-0230 already states. Rejected:
   parse the `(kind)` prefix of the summary, which is still text coupling;
   and map a macro witness to `macro_generated_value`, whose route is about
   macro-generated values, not unresolved reach.
6. **Opaque in pilot.** Adopted (coordinator ruling, 2026-10-04): keep
   `opaque` in the ranked set at RIPR-SPEC-0237 rank 4, after every gap
   and unknown class, with no new JSON field. Merged #5569 ranks it there;
   RIPR-SPEC-0230 says only "after every gap class", and its amendment
   adds the unknown classes. Rejected: withhold it from the top N with a disclosed
   count (#5497), which changes the pilot JSON contract; #5497 stays the
   place to revisit it.
7. **Agent brief.** Adopted: keep admitting `opaque` at its lowest
   priority, because the brief routes it to inspection. Rejected: drop it
   there too, which hides the limitation from the inspection queue.
8. **Headline and `static_limitation`.** Adopted: the class axis owns the
   headline; `activation_unknown` stays headline-eligible as RIPR-SPEC-0005
   lists. Rejected: drop every `static_limitation` seam from the headline,
   which changes the badge contract and belongs to #5497.

## Required Evidence

- Each Problem-table probe reads the class this spec gives, in
  `repo-exposure-json` `seams[].grip_class`.
- c1 `suffix` reads `activation_unknown` and keeps one missing
  discriminator; c2 `suffix` stays `weakly_gripped`; c3 `suffix` and
  `label` stay `reachable_unrevealed`; the fixtures
  `property_macro_noop_named_test` and
  `property_macro_noop_property_assertion` stay `reachable_unrevealed`.
- The evidence-record category of every unresolved-reach limitation is
  `opaque_static_evidence`, including c6 and a macro witness.
- `ripr pilot` on c4 lists `checkout` first and the three opaque seams
  after it; `actionable_total` is 4 (unchanged).
- No seam moves into or out of `reachable_unrevealed`, and none moves into
  `strongly_gripped`, `weakly_gripped` or `ungripped`. Golden drift lists every seam whose class moved.
- Runtime-control integration tests (`cargo test -p ripr --test '*'`) pass
  unchanged.

## Non-Goals

- No new grip class, `static_limit_kind` or exposure class.
- No path-aware or type-aware witness search.
- No change to the activate stage producer.
- No change to pilot tie-breakers or the per-owner spread (#6294).
- No change to `ripr check` findings (#5416).
- No fix for the propagate and observe splits named under Further rules:
  a gap class can still sit beside `static_limitation`.
- No runtime mutation claim. The mutation spot check stays an independent
  measure.

## Acceptance Examples

Unit rows give stage states reach, activate, propagate, observe,
discriminate and the missing count, as `classify_seam` input.

1. yes, yes, yes, yes, yes; 0: `strongly_gripped` (unchanged).
2. yes, yes, yes, yes, yes; 1: `weakly_gripped` (unchanged).
3. yes, yes, yes, yes, weak; 0: `weakly_gripped` (unchanged).
4. yes, unknown, yes, yes, yes; 1: `activation_unknown`, missing kept
   (today `weakly_gripped`; #5946 agrees).
5. yes, unknown, yes, yes, weak; 0: `activation_unknown` (today
   `weakly_gripped`; #5946 agrees).
6. yes, unknown, yes, yes, no; 0: `reachable_unrevealed` (unchanged;
   #5946 agrees).
7. yes, yes, unknown, no, no; 0: `reachable_unrevealed` (unchanged).
8. yes, unknown, yes, yes, yes; 0: `activation_unknown` (unchanged).
9. yes, yes, unknown, yes, yes; 0: `propagation_unknown` (unchanged).
10. yes, yes, yes, unknown, yes; 0: `observation_unknown` (unchanged).
11. yes, yes, yes, yes, unknown; 0: `discrimination_unknown` (unchanged).
12. yes, unknown, yes, yes, opaque; 1: `opaque` (unchanged, rule 2).
13. no, no, no, no, no; 1: `ungripped` (unchanged).
14. yes, weak, yes, yes, weak; 0: `opaque` by rule 10 (today
    `weakly_gripped`; #5946 also `weakly_gripped`). Latent: the producer
    never emits activate `weak`.
15. yes, not_applicable, yes, yes, yes; 1: `opaque` by rule 10 (today
    `weakly_gripped`). Latent.
16. Probe c1 (production `fn suffix(v: u64) -> &'static str { if v > 1
    { "items" } else { "item" } }` called only from `impl fmt::Display for
    Plural`; test `assert_eq!(Plural(3).to_string(), "items")`): boundary
    seam `activation_unknown`, one missing discriminator `1 (boundary
    value)`, SARIF `ripr.seam.activation_unknown` / `note`, LSP
    `INFORMATION` (today `weakly_gripped`, `warning`).
17. Probe c2 (same `suffix`, `pub`, test `assert_eq!(suffix(3), "items")`):
    `weakly_gripped`, evidence record `actionable_related_test_extension`
    (unchanged).
18. Probe c3 (`pub fn label(v: u64) -> String { format!("{v} {}",
    suffix(v)) }`; test `let _ = label(3);`): `suffix` and `label` both
    read `reachable_unrevealed` (unchanged); both evidence records stay
    `static_limitation` (known remaining split).
    The fixtures `property_macro_noop_named_test` and
    `property_macro_noop_property_assertion` likewise stay
    `reachable_unrevealed` (unchanged).
19. Probe c4 (private `tier`, `route` behind `pub fn checkout`;
    `tests/checkout.rs`: `assert_eq!(opaq::checkout(150), 20)`): `tier`
    reach `opaque`, summary names
    `rust_integration_public_api_path_unresolved` and "`checks_out`
    (tests/checkout.rs:2) calls `checkout`"; class `opaque` (unchanged).
    Reach limitation category `opaque_static_evidence` (unchanged).
20. Probe c4 under `ripr pilot`: `checkout` `call_presence`
    (`weakly_gripped`) first, then the three `opaque` seams;
    "Actionable seams: 4 total" (unchanged).
21. Probe c6 (c4 with the test named `snapshot_checkout`): reach
    limitation category `opaque_static_evidence` (today
    `snapshot_field_unknown`).
22. The same chain with a test named `mock_checkout` or an entry named
    `generated_route`: category `opaque_static_evidence` (today
    `unsupported_mock_shape` or `macro_generated_value`, inferred from
    `static_limitation_category`).
23. A macro witness (test calls `pub fn run`, whose body invokes a
    same-crate `macro_rules!` that names private `tier`, and no other path
    relates a test): reach `opaque`, summary names
    `rust_macro_reach_unresolved`, category `opaque_static_evidence`
    (today `macro_generated_value`, inferred). The probe built for this
    (c8) found a related test instead, so the fixture must keep the owner
    out of the related-test relation.
24. A helper chain whose test lives in `src/` (`#[cfg(test)]` module
    calling `checkout`): summary names `rust_transitive_reach_unresolved`
    (unchanged; no test pins it today).
25. `reach_without_related_tests` with no resolved owner function: reach
    `opaque`, summary contains `(owner unresolved)`, class `opaque` (today
    reach `no`, `ungripped`).
26. Probe c5 (`pub fn tier` with no test): `ungripped`, reach summary "No
    static test path found for seam owner `src/lib.rs::tier`" (unchanged).
27. Classified `[strongly_gripped, intentional, suppressed, opaque]` under
    `--max-seams 5`: pilot top list `[opaque]` (unchanged; ordering per
    RIPR-SPEC-0237).

## Test Mapping

- Existing: `crates/ripr/src/analysis/seam_classification.rs` tests
  `given_all_ripr_stages_yes_and_strong_oracle_then_seam_is_strongly_gripped`,
  `given_related_tests_with_missing_boundary_discriminator_then_seam_is_weakly_gripped`,
  `given_no_related_tests_then_seam_is_ungripped`,
  `given_reach_yes_but_discriminate_no_then_seam_is_reachable_unrevealed`,
  `given_activation_unknown_then_seam_class_is_activation_unknown`,
  `given_propagate_unknown_then_seam_class_is_propagation_unknown`,
  `given_observe_unknown_then_seam_class_is_observation_unknown`,
  `given_discriminate_unknown_then_seam_class_is_discrimination_unknown`,
  `given_opaque_static_limitation_then_seam_class_is_opaque`,
  `weak_discriminate_maps_to_weakly_gripped_even_without_missing_discriminators`,
  `headline_eligibility_matches_spec_table`.
- Existing: `crates/ripr/src/analysis/test_grip_evidence/tests.rs` (the six
  RIPR-SPEC-0230 seam tests).
- Existing: `crates/ripr/src/analysis/classify/transitive_reach.rs` tests
  `transitive_reach_limit_kind_names_integration_test_path` and
  `macro_reach_limit_kind_names_direct_test_body_macro_path`.
- Existing: `crates/ripr/src/lsp/diagnostics.rs` tests
  `weakly_gripped_seam_emits_warning_with_stable_code`,
  `unknown_classes_emit_information`, `opaque_emits_information_severity`.
- Existing: `crates/ripr/src/output/pilot/tests.rs` test
  `pilot_ranking_excludes_solved_governed_classes` pins example 27
  (unchanged).
- From #5946 (open):
  `given_activation_unknown_then_weak_evidence_is_activation_unknown_not_weak_grip`
  and
  `boundary_with_unobserved_activation_is_activation_unknown_and_keeps_its_hint`.
- Planned: one `classify_seam` unit test per examples 1 to 15; a seam
  test for examples 16, 18, 21, 23, 24 and 25; an evidence-record test
  that a tagged reach stage maps to `opaque_static_evidence` whatever its
  summary text.

## Implementation Mapping

- `crates/ripr/src/analysis/seam_classification.rs`: the rule 5 guard;
  module doc points at this spec.
- `crates/ripr/src/analysis/test_grip_evidence/reach_limit.rs`: rule R1,
  the cache key (R4) and the typed source tag.
- `crates/ripr/src/analysis/test_grip_evidence.rs`: `reach_evidence` and
  the activate producers (unchanged behavior).
- `crates/ripr/src/analysis/test_grip_evidence/related_tests/context.rs`:
  owner lookup and the unresolved-reach cache.
- `crates/ripr/src/output/evidence_record.rs`: `push_stage_limitation`
  reads the tag before `static_limitation_category`.
- `crates/ripr/src/analysis/seam_cache.rs`: cache generations.
- Read, unchanged: `crates/ripr/src/analysis/seams.rs`
  (`is_headline_eligible`), `crates/ripr/src/config/model.rs`
  (`SeamSeverityConfig`), `crates/ripr/src/lsp/diagnostics.rs`,
  `crates/ripr/src/output/sarif.rs`,
  `crates/ripr/src/output/gap_vocabulary.rs`,
  `crates/ripr/src/analysis/repair_route.rs`,
  `crates/ripr/src/app/agent_brief.rs`.

## Metrics

- `seam_class_rule_mismatch`: acceptance examples whose class, pilot
  admission or reach limitation category differs from this spec; must be
  zero.

---
