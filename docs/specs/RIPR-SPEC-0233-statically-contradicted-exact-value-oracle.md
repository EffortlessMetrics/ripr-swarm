# RIPR-SPEC-0233: Statically contradicted exact-value oracles

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

- #6026 (a boundary assert whose expected value contradicts static evaluation closes the gap)
- #7007 (the two-test composition — a wrong-valued assert beside a consistent strong test — still closed the gap and rendered an empty weak/unknown section)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This spec bounds oracle credit, not a surface: a related
  test whose asserted value statically contradicts the seam owner's fold
  keeps at most `weak` oracle strength, keeps the seam's gap open, and names
  the contradiction in the evidence summary. Claim boundaries remain
  governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Classified-seam cache generations bump — full `1.33` -> `1.34` and
  sharded/compact `0.39` -> `0.40`, stacked above the #5946 weak-grip
  (`1.32`/`0.38`) and #6718 bool-owner pin (`1.33`/`0.39`) transitions this
  PR rebased over — because warm entries would keep serving the
  contradicted assertion's strong credit. The #7007 activation transition
  bumps again — full `1.49` -> `1.50` -> `1.51` and sharded/compact
  `0.55` -> `0.56` -> `0.57` (stacked above #5328's constant-row-table
  `1.47`/`0.53` and table-row-boundary `1.48`/`0.54` and #5334's
  macro-generated-test `1.49`/`0.55` transitions this PR rebased over) —
  because warm entries would keep serving the contradicted test's observed
  value, the closed gap, and a count-less evidence shape.
- No repo-exposure schema version bump: the evidence-record shape is
  unchanged; only values and evidence summaries move.

## Problem

A related test that asserts `assert_eq!(owner(literal, ..), literal)` is
credited as a strong `exact_value` oracle even when the asserted literal is
statically wrong. The issue #6026 shape: a boundary crate whose owner folds
(`5_000 * 70 / 100 = 3_500` at the asserted input), plus a test that follows
the repair packet's `exact_return_value` recipe with the wrong expected
literal `5_000` — the value the `>=` -> `>` flip would produce. The test
fails at baseline and would pass under the mutation, so as mutation-exposure
evidence it is exactly inverted. Static evidence credits it as a full
repair: the seam moves `weakly_gripped -> strongly_gripped`, the outcome
receipt reports `improved; gap closed`, and the weak/unknown section is
empty. A statically-evaluably-false assert is indistinguishable from a
correct one, and a wrong actionable repair signal is worse than a missed
advisory finding.

## Behavior

### The fold and the credit bound

The verdict is decided per related-test assertion against the seam owner:

- `assert_eq!`/`debug_assert_eq!` over a bare-path call to the owner with
  literal arguments plus a literal expected value, where the owner is a free
  function of checked integer arithmetic over its parameters (`+ - * / %`,
  comparisons, boolean operators, `if`/`else if`/`else`, negation, integer
  literals with optional suffixes), folds to one integer value.
- Division or remainder by zero, arithmetic overflow, or any shape outside
  that grammar leaves the pair not evaluable — today's credit is retained.
- A fold that contradicts the asserted relation (`!=` for `assert_eq!`,
  `==` for `assert_ne!`) downgrades that assertion to at most `weak`
  strength everywhere the credit is consumed (best-oracle selection,
  discrimination evidence, sink-matching propagation), keeps the seam's gap
  open, and states the contradiction — the asserted literal and the folded
  value — in the stage and related-test evidence summaries.
- The same fold also withholds the contradicted test's call-site credit
  (#7007): its owner-call argument values do not count toward the seam's
  activation/observed-values evidence, its arguments do not satisfy
  boundary-equality coverage, and its bare owner call grants no
  value-insensitive activation credit in either the full or the compact
  path, because completing the equality boundary or crediting a
  baseline-failing call in the mutant's favor is exactly the inverted
  signal. Consistent and not-evaluable tests keep their activation credit
  unchanged.
- The evidence carries a producer-owned contradiction count over the FULL
  related set (#7007 review). The evidence record's `related_tests` array
  is a capped projection and each entry names only its best oracle, so the
  count is the completeness authority: outcome parsing keeps the gap open
  on a positive count even when the rendered subset names no contradicted
  test, and a snapshot that records no count establishes nothing either
  way.
- While a contradicted related test remains in an evidence set, the outcome
  receipt cannot report that evidence set's gap closed, and the receipt's
  weak/unknown section names the contradiction instead of rendering its
  empty fallback. Detection keys on the shared disclosure prefix the
  analysis producer writes into the related-test evidence summary, plus
  the producer count. New seams (present only in the after snapshot) carry
  the same disclosure, and contradiction disclosures are exempt from the
  section's generic item cap so a capped attention row cannot push them
  out.

## Rule

When a related test's equality assertion (`assert_eq!`/`assert_ne!` family)
carries literal arguments and a literal expected value, and the seam's owner
is a free function whose body constant-folds at those arguments:

- The fold result is compared against the asserted relation
  (`assert_eq!` contradicts when the folded value differs; `assert_ne!`
  contradicts when it is equal).
- On contradiction the oracle keeps at most `weak` strength, contributes no
  strong discrimination, and the seam's gap stays open. The evidence
  summary names the contradiction with both values, in the same
  evidence-record channel the receipt already renders.
- On contradiction the test's call-site values contribute no activation
  credit: they are excluded from the seam's observed activation values and
  from boundary-equality coverage. When that leaves no credited activation
  evidence at all, the activation stage discloses the withholding instead
  of reading as if nothing ran.
- While a contradicted related test remains in an after evidence set, the
  outcome receipt reports the movement as improved rather than closed,
  carries the contradiction in the moved entry's evidence delta, and lists
  it under remaining weak or unknown.
- The per-seam stage evidence records the contradiction in its summary.
- When the owner is not statically evaluable — a method or nested function,
  a body with calls, `let` bindings, early returns, non-literal operands,
  or arithmetic that is undefined at the asserted input — the verdict is
  not evaluable and today's behavior is retained, including its limitation.
  No taxonomy or value is manufactured from an unavailable fold.
- The fold can only withhold credit, never grant it: a consistent or
  non-evaluable verdict leaves the extracted oracle strength unchanged.

## Discriminating evidence

`wrongval_boundary_assert_does_not_close_the_gap` builds the issue's exact
crate and fails on the pre-fix behavior (the gap closes); the control
`correct_boundary_literal_still_closes_the_gap` keeps the strong credit and
the closure for the correct literal `3_500`; and
`non_evaluable_owner_keeps_the_prior_credit` pins the retained limitation.
`wrongval_assert_beside_consistent_test_keeps_gap_open` builds the issue
#7007 two-test composition — the wrong-valued assert beside a consistent
happy-path test — and fails on the pre-#7007 behavior (the sibling strong
oracle carried the seam to `strongly_gripped`); it pins that the wrong
test's `5_000` argument is not credited as observed, the equality boundary
stays missing, and the contradiction stays named. On the receipt side,
`contradicted_related_test_keeps_gap_open_and_names_the_contradiction`
(red on the pre-#7007 `closed` movement) pins the non-closure, the named
moved-entry delta, and the non-empty weak/unknown section, while
`prose_that_mentions_contradiction_without_the_disclosure_does_not_block_closure`
pins that only the shared disclosure prefix can hold a gap open. The fold's
own contract is pinned by `value_contradiction.rs` unit tests
(wrong-valued and correct equality, inverted `assert_ne!`, qualified paths,
else-if chains, negative and suffixed literals, and the not-evaluable
refusals).

## Non-Goals

- No mutation engine and no runtime test execution: the fold is a static,
  bounded constant evaluation, and a consistent verdict is still static
  evidence, never a runtime pass.
- No new oracle kind: a contradicted assertion keeps its extracted
  `exact_value`/`whole_object_equality` kind; only the strength and the
  disclosure move.
- No widening of the fold to method owners, `let`-bound locals, closures,
  or wrapping arithmetic; those stay not-evaluable until a producer owns
  them.
- No change to the repair packet's `exact_return_value` recipe; the packet
  already recommends the correct shape, and the contradiction discloses the
  wrong literal.

## Acceptance Examples

Source: the issue #6026 crate — `discounted_total` with an
`amount_cents >= discount_threshold_cents` boundary and a discounted arm of
`amount_cents * 70 / 100`.

1. Test `assert_eq!(discounted_total(5_000, 5_000), 5_000);` added: the
   owner folds to `3_500` at that input, so the assert is contradicted — the
   oracle reads at most `weak`/`exact_value`, the seam stays
   `weakly_gripped`, the outcome reports no gap closure, and the evidence
   summary names `asserts 5_000, owner folds to 3500`. The same holds in
   the issue #7007 composition where a consistent happy-path test is also
   present: the wrong test's `5_000` argument is not credited as an
   observed activation value, the equality boundary stays missing, and the
   seam stays `weakly_gripped`.
2. Test `assert_eq!(discounted_total(5_000, 5_000), 3_500);` added: the fold
   agrees — the oracle stays `strong`, and the gap closes exactly as before.
3. Owner whose discounted arm calls a helper, same wrong literal as (1):
   not statically evaluable — today's credit and the closure are retained,
   with no manufactured evidence either way.
4. Even when a consistent test alone covers the boundary so the class
   reaches `strongly_gripped`, a contradicted test remaining in the
   evidence set keeps the receipt from reporting `gap closed` and names the
   contradiction in the weak/unknown section.

## Test Mapping

- Landed: `value_contradiction.rs` unit tests pin the fold verdicts;
  `test_grip_evidence/tests.rs::wrongval_boundary_assert_does_not_close_the_gap`
  (red on the pre-fix behavior),
  `::correct_boundary_literal_still_closes_the_gap`, and
  `::non_evaluable_owner_keeps_the_prior_credit` pin the credit bound
  end-to-end through `evidence_for_seam` and `classify_seam`.
- #7007 composition:
  `test_grip_evidence/tests.rs::wrongval_assert_beside_consistent_test_keeps_gap_open`
  (red on the pre-#7007 behavior) and
  `output/outcome/mod.rs::contradicted_related_test_keeps_gap_open_and_names_the_contradiction`
  (red on the pre-#7007 receipt) pin the activation withholding and the
  receipt contract, with
  `::prose_that_mentions_contradiction_without_the_disclosure_does_not_block_closure`
  pinning the disclosure-prefix identity.
- #7007 review:
  `test_grip_evidence/tests.rs::contradiction_count_counts_a_test_beside_its_best_oracle`
  and `::wrongval_only_test_grants_no_activation_credit_on_value_insensitive_seam`
  (both red on the pre-review behavior), plus
  `output/outcome/mod.rs::producer_contradiction_count_holds_the_gap_open_without_a_named_entry`
  and `::a_new_seam_with_a_contradicted_test_surfaces_in_the_weak_section`
  (red on the pre-review receipt), pin the completeness count, the
  value-insensitive activation bound, and the new-seam/cap-exempt
  disclosures. The RC rehearsal
  (`crates/ripr/tests/cli_smoke.rs::agent_repair_after_a_failing_test_says_the_test_was_not_run`)
  pins the agent-repair surfaces under the corrected unchanged movement.
- Not pinned by the existing honesty corpora: the RIPR-SPEC-0108 corpus
  asserts diff-scoped finding classifications and the actionable-gap corpus
  joins hand-authored receipts, while this rule is the repo-scope seam
  credit. Graduating the wrongval shape into a pinned corpus first needs a
  grip-scoped corpus surface (or the diff-scope counterpart of this rule);
  until then the discriminating tests above are the pinned proof.

## Implementation Mapping

- `crates/ripr/src/analysis/test_grip_evidence/value_contradiction.rs`: the
  fold — assertion shape, owner-call argument extraction, the constant
  folder over the owner's body, and the verdict.
- `crates/ripr/src/analysis/test_grip_evidence.rs`: the credit bound —
  `effective_oracle_strength` feeds `best_oracle`, `discriminate_evidence`,
  and `oracles_match_sink`, so a contradicted assertion keeps at most
  `weak` strength, the gap stays open, and the stage and related-test
  summaries name the contradiction. `test_has_contradicted_assertion`
  (#7007) additionally withholds the contradicted test's activation
  credit: its call-site values and boundary-equality arguments do not count
  toward `observed_values` or boundary coverage in `activate_evidence`, and
  its bare owner call grants no value-insensitive activation credit in the
  full or compact activation paths (#7007 review). The evidence records
  `statically_contradicted_related_tests`, the producer count over the
  full related set.
- `crates/ripr/src/output/evidence_record.rs`: the record renders the
  producer count (`statically_contradicted_related_tests`) only while it is
  positive, so its presence is the completeness signal (#7007 review).
- `crates/ripr/src/output/outcome/`: the receipt bound (#7007) —
  `contradicted_related_tests` parses the shared disclosure prefix from the
  after evidence set, a non-empty set — or a positive producer count —
  downgrades `closed` to `improved`, the moved entry's evidence delta names
  each contradiction, and the weak/unknown review section lists them
  instead of its empty fallback, ahead of the capped attention rows and
  including new seams (#7007 review).
- `crates/ripr/src/analysis/seam_cache.rs`: the classified-seam cache
  generation bumps so warm entries cannot keep serving the strong credit
  (`1.34`, then `1.50` for the #7007 activation transition and `1.51` for
  the producer contradiction count).

## Required Evidence

- The issue #6026 wrongval reproduction: the boundary crate with the
  assert-the-mutant's-value test leaves the seam `weakly_gripped`, the
  outcome receipt reports no gap closure, and the evidence summary names the
  contradiction (red on the pre-fix behavior).
- The issue #7007 two-test composition: the wrong-valued assert beside a
  consistent strong test leaves the seam `weakly_gripped`, the wrong test's
  `5_000` argument stays uncredited, the receipt does not report the gap
  closed, and the weak/unknown section names the contradiction (red on the
  pre-#7007 behavior).
- The correct-literal control (`3_500`) still closes the gap with a strong
  oracle and an unchanged `exact value assertion` summary.
- A not-evaluable owner with the same wrong literal keeps today's credit.
- `cargo xtask goldens check` and `dogfood` show no drift on the existing
  calibration corpus (every pinned fixture assertion folds consistently).

## Metrics

- `statically_contradicted_exact_value_credit`: seams whose only
  strong-oracle path is a statically contradicted equality assertion; the
  credit bound must hold (zero closures from contradicted asserts). Derived
  from the evidence summaries the receipt channels already carry.
