# RIPR-SPEC-0235: Perl fact-packet classification

Status: proposed

Owner: product / analysis

Created: 2026-10-04

Linked proposal:

- [RIPR-PROP-0018: Perl Repair Routing Lane](../proposals/RIPR-PROP-0018-perl-repair-routing-lane.md)

Linked ADRs:

- [ADR 0018: Perl LSP Fact Substrate](../adr/0018-perl-lsp-fact-substrate.md)

Linked plan:

- None yet

Linked issues:

- None yet. Defects found while writing this spec are to be filed
  separately.

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Perl stays preview and advisory (RIPR-SPEC-0064). No
  finding moves to a stronger class, and no displayed oracle strength or
  relation confidence rises. Rules 5, 7, 12 and 13 weaken today's output:
  - rule 5: a cross-test `exposed` moves to `reachable_unrevealed`;
  - rule 7: a mixed-relation `weakly_exposed` moves to
    `reachable_unrevealed`;
  - rule 12: a `strong_exact` label that the kind gate refuses stops
    reading `strong`, and `method_receiver` stops disclosing as a
    high-confidence direct owner call;
  - rule 13: a `static_unknown` finding loses its canonical gap and its
    `perl_suggested_test_location`, and a finding with no bound
    `direct_owner_call` / `reachable` relation loses its suggested
    location. A real-producer packet (`owner:` ids, `partial` status) keeps
    its suggested location when such a relation exists. It still gets no
    canonical gap, as today.
- Every other rule, including rules 1, 10 and 14, changes no output.
  Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. `ripr-perl-facts-v1` is unchanged. No new
  process, network, file-policy or dependency surface.

## Problem

ripr does not read Perl. The `lang-perl` adapter
(`analysis/language/perl/`) consumes a `ripr-perl-facts-v1` packet that an
external producer writes, given with `--perl-facts <path>`. The producer
decides which Perl assertion is which oracle kind and strength, and which
call is which relation kind and reachability hint. ripr then gates those
values into a class, a displayed oracle, a canonical gap and evidence
lines. RIPR-SPEC-0064 defines the packet vocabulary and says "RIPR
decides". No spec states how. RIPR-SPEC-0108 pins ten packet-backed
corpus outcomes but not the rules behind them.

Measured on main bcb0be576 with the dev `ripr check --perl-facts` binary in
a scratch repo with a non-empty diff, 34 probe packets derived from
`fixtures/evidence-promotion-honesty-corpus/perl-packets/perl_direct_owner_advisory_positive.json`
(the baseline: `Pricing::discount`, a `return_value` change with
discriminator `$amount == $threshold`, test `t/pricing.t`, oracle
`is(Pricing::discount(100), 90, 'threshold')` as `exact_return_assertion` /
`strong_exact`, relation `direct_owner_call` / `reachable`). The probe
packets were research inputs and are not kept in the repo; acceptance
examples 27 and 28 are planned as retained corpus packets. Each row
changes one fact from the baseline.

| Packet facts | Today class / displayed oracle | What the test pins |
| --- | --- | --- |
| baseline | `weakly_exposed`, `exact_value` / strong | the return for one input |
| `ok(...)` as `smoke_ok` labeled `strong_exact` | `reachable_unrevealed`, `smoke_only` / **strong** | only truthiness |
| `dies_ok {...}` as `dies_only` / `strong_exact` | `reachable_unrevealed`, `unknown` / **strong** | only that it dies |
| relation A `direct_owner_call` / `weakly_reachable`, relation B `package_reference` / `reachable`, both strong | **`weakly_exposed`** | nothing through a direct call |
| relation to test `other` carrying the oracle of `test_discount_threshold`, sinks aligned | **`exposed`** | nothing in test `other` |
| `method_receiver` | `reachable_unrevealed`, reason **`direct_owner_call` / high** | a call through a receiver |
| hint `static_unknown` | `static_unknown`, **gap and suggested location emitted** | unknown |
| hint `weakly_reachable`, or `helper_call` / `package_reference` / `test_name_match` / `method_receiver` | `reachable_unrevealed`, **suggested location emitted** from the first bound relation | no established direct call |

The other rows (strong-eligible kinds, weak strengths, oracle target,
hint order, relation caps, sink equality, partial packets, runner and
limitation caps, unrecognized limitation kinds) match the rules below and
are listed as unchanged acceptance examples. Today the suggested location
needs only a concrete discriminator, a class other than `exposed`, no
blocking static limit and one bound relation. It does not check
`packet_status` or the owner id scheme, and rule 13 keeps it that way.
Two more facts:

- the packet fingerprint (`recompute_packet_fingerprint`) does not hash
  `relation_kind`, `reachability_hint`, oracle `kind` or `strength`,
  `changed_observable`, `changed_text_digest`, `behavior_hint`,
  boundaries, limitations or `packet_status`. Unit tests such as
  `relation_gate_rejects_file_proximity_only` edit `relation_kind` without
  calling `bless_fingerprint` and still pass ingestion. The comment
  at step 8 of `validate_ingestion` says "oracle_id+kind+target", but kind
  is not hashed;
- `h1_file_proximity_relation_is_not_eligible` asserts no suggested
  location, but its packet's discriminator is `sha256:...`, so the
  assertion holds for the wrong reason (inferred from code).

## Behavior

### Scope

These rules govern what ripr does with packet values after ingestion. They
do not govern how a producer maps `is`, `is_deeply`, `ok`, `like`,
`throws_ok`, `use`, `require` or method calls to packet values. ripr never
parses Perl to second-guess a packet value. It may refuse, cap or ignore a
value, and it never raises one. `ripr doctor` framework detection is
display only and plays no part here.

### Ingestion (unchanged)

`consume_fact_packet` first rejects a `schema_version` other than
`ripr-perl-facts-v1`. `validate_ingestion` then rejects a packet:

- whose `packet_status` is `unavailable`;
- whose producer is not on the allowlist;
- with a duplicate id in any array, or an id that is empty, contains
  whitespace, or carries a host, temp or drive marker;
- with a `file.path` or `root.repo_relative` that is not repo-relative;
- with a relation whose change, owner, test or oracle id does not
  resolve (except the `change:unresolved` change id), or a change whose
  file or owner does not resolve;
- with more than 10,000 entries in a fact array;
- with tests or oracles but no `test_facts` capability;
- whose fingerprint does not recompute;
- whose `input.base` differs from `--base`, when both are present;
- whose `input.head` is missing or empty;
- whose declared digest differs from a file present on disk. A file not
  on disk is skipped.

A rejected packet yields no Perl finding and a recorded unavailable
language run.

### Classification order

For each change, the first match wins:

1. a class-capping static limit (rule 1) gives `static_unknown`;
2. no bound relation (rule 3) gives `no_static_path`;
3. established sink alignment (rule 9) gives `exposed`;
4. otherwise the aggregate of the per-relation classes (rule 8).

### Rules

1. **Static-limit cap.** A boundary applies when its `owner_id` is the
   change owner, or, with no owner, when its `file_id` is the change file
   or a bound relation's test file. Every applicable boundary blocks
   repair-shaped output. Every boundary kind except `missing_test_runner`
   also caps the class at `static_unknown`. A limitation applies when its
   `evidence_refs` is empty (global) or intersects the change or relation
   evidence ids. A limitation whose kind is in the blocking list
   (`limitation_kind_blocks_strict_actionability`) or has a static-limit
   label blocks repair-shaped output. Only a `dynamic_dispatch` limitation
   caps the class. A limitation kind outside the RIPR-SPEC-0064
   enumeration (for example `partial_inference`, `range_precision`,
   `unverified_provenance`) has no effect, as
   `perl_packet_contract_migration_corpus_pins_real_producer_dispositions`
   pins.
2. **No relations.** A change with no bound relation is `no_static_path`,
   reach `no`.
3. **Relation binding.** A relation binds to a change only when
   `relation.change_id` is the change id and `relation.owner_id` is the
   change owner, compared as exact strings. A relation whose change id is
   the `change:unresolved` sentinel binds to no real change. Ingestion
   does not reject a `changes[]` entry whose id is literally
   `change:unresolved`; such an entry would bind sentinel relations by
   string equality, and a producer must not emit one.
4. **Hint order.** Per relation, `static_unknown` gives `static_unknown`,
   then `weakly_reachable` gives `reachable_unrevealed`, then a relation
   with no `oracle_id` gives `reachable_unrevealed`.
5. **Oracle binding.** An oracle counts for a relation only when it is
   named by `relation.oracle_id`, its `test_id` is the relation's
   `test_id`, and its `target_owner_id` is the change owner. Sink
   alignment (rule 9) uses the same binding. Today sink alignment skips
   the test-id check, so a relation can be `exposed` on another test's
   oracle. An oracle that fails the binding is treated as absent for that
   relation. Ingestion does not reject the cross-link.
6. **Strong-eligible kinds.** An oracle is strong only when its strength
   is `strong_exact` and its kind is one of `exact_return_assertion`,
   `predicate_boundary_assertion`, `exception_observer`,
   `hash_or_object_field_assertion`, `output_observer`, `warn_observer` or
   `log_observer`. `smoke_ok`, `mention_only`, `dies_only`,
   `unknown_helper`, `dynamic_framework_indirection` and `unknown` are
   never strong, whatever their label. `weak_smoke`, `weak_broad`,
   `mention_only` and `unknown` strengths are never strong. A relation
   with a bound strong oracle reads `weakly_exposed`; with any other bound
   oracle, `reachable_unrevealed`.
7. **Relation-kind cap, per relation.** Only `direct_owner_call` supports
   positive reachability. A relation of any other kind
   (`method_receiver`, `helper_call`, `package_reference`,
   `test_name_match`, `file_proximity`, `fixture_setup`, `unknown`) is
   capped at `reachable_unrevealed` before aggregation. Today the cap
   checks whether any bound relation is `direct_owner_call`, so a strong
   advisory relation beside a weak direct one reads `weakly_exposed`.
8. **Aggregation.** The change takes the strongest per-relation class:
   any `weakly_exposed`, then any `reachable_unrevealed`, otherwise
   `static_unknown`.
9. **Exact sink alignment.** A change is `exposed` only when one bound
   relation is `direct_owner_call` with hint `reachable`, its oracle
   passes rules 5 and 6, and the trimmed non-empty `observed_sink`
   equals the trimmed `changed_observable`, or equals the observable after
   one leading `return ` is removed and the rest is trimmed again (so
   `return   $x` aligns with `$x`). The strip is one way. Apart from
   that edge trimming, equality is exact: no substring match and no
   whitespace normalization inside the text. An `exposed` change emits
   `perl_already_discriminated:` and
   no gap, suggested location, suggested assertion or missing
   discriminator.
10. **Partial packets.** `packet_status: partial` does not cap the class.
    A sink-aligned change in a partial packet is `exposed`. A partial
    packet records a partial language run and gets no canonical gap. It
    keeps `perl_suggested_test_location` under rule 13.
11. **Inputs that do not move the class.** Relation, oracle and test
    `confidence`, and test `framework`, do not change the class. They feed
    only strict actionability, which is reachable from tests only.
12. **Displayed oracle and relation.** The displayed values never exceed
    the class rules.
    - Kind: `exact_return_assertion` shows `exact_value`,
      `predicate_boundary_assertion` shows `relational_check`, `smoke_ok`
      shows `smoke_only`, every other kind shows `unknown` (unchanged).
    - Strength: `strong` only when rule 6 admits the oracle. A
      `strong_exact` label that rule 6 refuses shows `smoke` for
      `smoke_ok` and `unknown` otherwise. `weak_smoke` shows `smoke`,
      `weak_broad` shows `weak`, `mention_only` and `unknown` show
      `unknown` (unchanged).
    - Relation: `direct_owner_call` shows `direct_owner_call` / high,
      `helper_call` shows `helper_owner_call` / high, `package_reference`
      shows `import_path_affinity` / medium, `test_name_match` shows
      `owner_named_test` / medium (unchanged). `method_receiver`,
      `file_proximity`, `fixture_setup` and `unknown` show null / null
      (today `method_receiver` shows `direct_owner_call` / high).
13. **Repair-shaped output.** Two outputs, two gates.
    - A canonical gap needs all of: a `changed_text_digest` starting with
      `discriminator:`; a class other than `exposed` and
      `static_unknown`; no blocking static limit (rule 1);
      `packet_status: complete`; no dynamic boundary whose `owner_id` is
      the change owner; and a canonical owner identity, which needs an
      owner id starting with `perl:`, an owner `kind` other than
      `unknown`, and an owner `file_id` that resolves. The unknown-kind
      condition keeps RIPR-SPEC-0064:274-276: an unresolved owner must not
      become an actionable repair packet. Only the `static_unknown`
      condition is new.
    - `perl_suggested_test_location` and `perl_suggested_assertion` need:
      a `discriminator:` digest; a class other than `exposed` and
      `static_unknown`; no blocking static limit; and a bound relation
      that is `direct_owner_call` with hint `reachable`. The location
      names that relation's test. They do not need `packet_status:
      complete` or a canonical owner identity, so a real-producer packet
      keeps its location. The `static_unknown` and relation conditions are
      new: today a `static_unknown` hint, a `weakly_reachable` hint and a
      capped relation still emit a location built from the first bound
      relation, and a `static_unknown` hint still emits a gap.
14. **Fingerprint scope.** The packet fingerprint is a consistency check
    on identity facts, not an authenticity control: it is an unkeyed
    SHA-256 that any writer can recompute. No output, doc or comment may
    say that a verdict-bearing field is fingerprint-verified unless the
    recipe hashes it. Extending the recipe to the fields rules 1 to 13
    read changes the producer contract and is tracked as an issue, not
    done here.
15. **Constant stages.** Reach is `yes` when any relation binds, whatever
    its kind or hint. Infect and propagate are `unknown`. Confidence is
    `0.5`. ripr never reports a runtime mutation outcome from a packet.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Cross-test oracle.** Adopted: treat the oracle as absent for that
   relation (rule 5), which removes the false `exposed`. Rejected:
   reject the packet at ingestion, because the real producer's behavior
   on this link is not measured and one bad relation would drop every
   Perl finding in the run.
2. **Mixed relations.** Adopted: cap per relation before aggregation
   (rule 7). Rejected: require every bound relation to be
   `direct_owner_call`, because one advisory relation would then hide a
   real direct-call `weakly_exposed`.
3. **Displayed strength.** Adopted: show `strong` only when rule 6 admits
   the oracle. Rejected: keep showing the raw label, because a reader of
   `smoke_only` / strong cannot tell it earned no credit.
4. **Observer kinds.** Adopted: keep showing `unknown` for exception,
   hash, output, warn and log observers. Rejected: map them to Rust
   domain kinds such as `exact_error_variant`, because that claims a
   shape the packet does not state. Showing less than the class earns is
   not over-credit.
5. **`method_receiver` disclosure.** Adopted: null / null like the other
   capped kinds without a clean domain reason. Rejected:
   `direct_owner_call` with a lower confidence, because domain relation
   confidence is derived from the reason.
6. **Repair output on `static_unknown`.** Adopted: no gap and no
   suggested location (rule 13). RIPR-SPEC-0064:428 says "RIPR decides
   the final reachability and actionability state", and a class ripr
   could not decide is not actionable.
   Rejected: keep the gap and drop only the location, because a gap on
   an unknown class reads as known debt.
7. **Unrecognized limitation kinds.** Adopted: keep today's behavior;
   a kind outside the RIPR-SPEC-0064 enumeration has no effect (rule 1),
   as `perl_packet_contract_migration_corpus_pins_real_producer_dispositions`
   pins. The real producer emits `partial_inference`, `range_precision`
   and `unverified_provenance` with `evidence_refs: []`, so they apply to
   every change; giving them blocking power would strip repair output
   from every real packet for routine provenance notes. Rejected: let an
   unrecognized kind block repair-shaped output (the literal `unknown`
   kind already does). Rejected: let it cap the class.
8. **Partial packets.** Adopted: keep today's `exposed` on sink alignment
   (rule 10), already pinned in
   `fixtures/perl_packet_contract_migration/expected/consumer-dispositions.v1.json`.
   Missing facts can only add relations or limits; the alignment present
   is a positive fact. Known limit: a partial packet can omit a limit,
   such as a `dynamic_dispatch` boundary, that a complete run would
   report, and the completed packet would then read `static_unknown`.
   The real producer marks every packet `partial`, so capping on that
   flag would remove `exposed` from every real Perl finding, and a
   `weakly_exposed` cap would put a false repair signal on each
   sink-aligned one. Closing the limit needs the producer to say which
   limit families it scanned, which is a producer change tracked on
   #6606. Rejected: cap partial packets at `weakly_exposed` or
   `static_unknown`.
9. **Fingerprint.** Adopted: state the scope (rule 14) and file the
   recipe extension as a cross-repo issue. Rejected: change the recipe
   here, because the real producer computes the same recipe and a
   one-sided change rejects every real packet.
10. **Suggested location on a partial packet.** Adopted: rule 13 keeps
    `perl_suggested_test_location` and `perl_suggested_assertion` on a
    partial packet that has a bound `direct_owner_call` / `reachable`
    relation. RIPR-SPEC-0064 lists the suggested test location as one
    field of an actionable repair packet (its "Strict actionability
    boundary"), and a partial packet still never gets that packet: rule
    13 withholds the canonical gap, and the finding carries
    `language_limitation_reason: packet_status partial: findings are
    advisory only`. The standalone location is advisory evidence that
    names a test the packet itself relates, not an actionable repair.
    The real producer always emits `partial`, so withholding it would
    remove the location from every real packet. Rejected: gate both
    suggestions on `packet_status: complete`. Reopen this if a corpus
    case shows a partial packet's omitted facts moving the right test.

## Required Evidence

- Each Problem-table row reads the class and display the rules give.
- Each unchanged acceptance example keeps its class, display and repair
  output.
- The ten packet-backed RIPR-SPEC-0108 Perl corpus reports keep their
  pinned class and `expected_oracle`. None uses a refused `strong_exact` label, a
  cross-test oracle, mixed relations or a `static_unknown` hint.
- No finding moves to a stronger class, a stronger displayed strength or
  a higher relation confidence. Golden drift lists every moved field.
- The h2 tests that assert only `!= Exposed` assert the exact class.
- `perl_packet_contract_migration_corpus_pins_real_producer_dispositions`
  stays unchanged and passing.
- A partial packet with `owner:` ids and a bound `direct_owner_call` /
  `reachable` relation keeps its suggested location and gets no canonical
  gap.

## Non-Goals

- No Perl parsing and no Perl assertion or call recognizer in ripr.
- No rule for how a producer chooses oracle kind, strength, relation kind
  or reachability hint. Those belong to the producer (RIPR-SPEC-0064).
- No change to the `ripr-perl-facts-v1` schema or the fingerprint recipe.
- No change to strict actionability, the repair card or the agent packet,
  which are reachable only from tests.
- No support-tier promotion and no gate, badge or baseline contribution.
- No runtime mutation or test execution.

## Acceptance Examples

Each example is the baseline packet with the facts shown changed.
Production: `sub discount { my ($amount) = @_; return $amount * 0.9 }` in
`lib/Pricing.pm`, a `return_value` change with discriminator
`$amount == $threshold`. Test `test_discount_threshold` in `t/pricing.t`.
"Location" means `perl_suggested_test_location`. The research probe
packets are not in the repo; the planned corpus packets retain these
cases.

1. Baseline: `is(Pricing::discount(100), 90, 'threshold')` as
   `exact_return_assertion` / `strong_exact`, `direct_owner_call` /
   `reachable`: `weakly_exposed`, `exact_value` / strong, gap, location
   `t/pricing.t::test_discount_threshold` (unchanged).
2. `ok(Pricing::discount(100))` as `smoke_ok` / `weak_smoke`:
   `reachable_unrevealed`, `smoke_only` / smoke, gap, location
   (unchanged).
3. `ok(Pricing::discount(100))` as `smoke_ok` / `strong_exact`:
   `reachable_unrevealed`, `smoke_only` / smoke (today `smoke_only` /
   strong).
4. `dies_ok { Pricing::discount(-1) }` as `dies_only` / `strong_exact`:
   `reachable_unrevealed`, `unknown` / unknown (today `unknown` / strong).
5. `throws_ok { Pricing::discount(-1) } qr/negative/` as
   `exception_observer` / `strong_exact`: `weakly_exposed`, `unknown` /
   strong (unchanged).
6. `is_deeply(Pricing::quote(100), {total => 90})` as
   `hash_or_object_field_assertion` / `strong_exact`, and
   `stdout_is(sub { Pricing::show(100) }, "90\n")` as `output_observer` /
   `strong_exact`: `weakly_exposed`, `unknown` / strong (unchanged).
7. `like(Pricing::discount(100), qr/9/)` as `exact_return_assertion` /
   `weak_broad`: `reachable_unrevealed`, `exact_value` / weak
   (unchanged).
8. A test that only names `Pricing::discount`, as `mention_only` /
   `mention_only`: `reachable_unrevealed`, `unknown` / unknown
   (unchanged).
9. Oracle `target_owner_id: null`: `reachable_unrevealed` (unchanged).
10. Relation without `oracle_id`: `reachable_unrevealed`, `unknown` /
    unknown (unchanged).
11. Hint `weakly_reachable`: `reachable_unrevealed`, gap, no location
    (today location emitted).
12. Hint `static_unknown`: `static_unknown`, no gap, no location (today
    both emitted).
13. `my $p = Pricing->new; is($p->discount(100), 90)` as
    `method_receiver` / `reachable`: `reachable_unrevealed`, relation
    null / null, no location (today `direct_owner_call` / high, location
    emitted).
14. `helper_call`: `reachable_unrevealed`, `helper_owner_call` / high, no
    location (today location emitted).
15. `package_reference`: `reachable_unrevealed`, `import_path_affinity` /
    medium; `test_name_match`: `reachable_unrevealed`, `owner_named_test`
    / medium. Neither has a location (class unchanged; today location
    emitted).
16. `relations: []`: `no_static_path`, reach `no`, gap, no location
    (unchanged).
17. `changed_observable: "return $amount * 0.9"`, `observed_sink:
    "$amount * 0.9"`: `exposed`, `perl_already_discriminated:`, no gap, no
    location (unchanged).
18. Reversed: `changed_observable: "$amount * 0.9"`, `observed_sink:
    "return $amount * 0.9"`: `weakly_exposed` (unchanged).
19. `changed_observable: "$amount*0.9"`, `observed_sink: "$amount * 0.9"`:
    `weakly_exposed` (unchanged).
20. Example 17's sinks with `method_receiver`: `reachable_unrevealed`
    (unchanged).
21. Example 17's sinks with hint `weakly_reachable`:
    `reachable_unrevealed` (unchanged).
22. Example 17's sinks with `packet_status: partial`: `exposed`, no gap
    (unchanged).
23. Example 17's sinks with a `missing_test_runner` boundary on
    `t/pricing.t`: `exposed`, `perl_missing_test_runner:` evidence
    (unchanged).
24. Limitation `framework_indirection`, `evidence_refs: []`:
    `weakly_exposed`, no gap, `static_limit_kind`
    `opaque_custom_assertion_helper` (unchanged).
25. Limitation `dynamic_dispatch`, `evidence_refs: []`, or a
    `dynamic_dispatch` boundary on the owner: `static_unknown`, no gap
    (unchanged).
26. Limitation `partial_inference`, `evidence_refs: []`:
    `weakly_exposed`, gap, location (unchanged).
27. Relation A `direct_owner_call` / `weakly_reachable` and relation B
    (test `test_pkg`) `package_reference` / `reachable`, both with bound
    strong oracles: `reachable_unrevealed` (today `weakly_exposed`).
    Control with A as `file_proximity`: `reachable_unrevealed`
    (unchanged).
28. A relation to test `other` whose `oracle_id` names the oracle of
    `test_discount_threshold`, with example 17's sinks:
    `reachable_unrevealed`, no `perl_already_discriminated:` (today
    `exposed`).
29. Relation `confidence: low`, or test `framework: unknown`:
    `weakly_exposed` (unchanged).
30. `packet_status: partial`, or the owner id written as
    `owner:lib/Pricing.pm:sub:Pricing::discount` in every fact:
    `weakly_exposed`, no gap, location
    `t/pricing.t::test_discount_threshold` (unchanged; from code, not
    probed).
31. Owner `kind: unknown`: `weakly_exposed`, no gap, location (unchanged;
    from code, not probed).

## Test Mapping

- Existing: `crates/ripr/src/analysis/language/perl/tests.rs`:
  `perl_related_test_linking_classifies_reachability_and_revealability`
  (rules 3, 4, 6), `perl_to_domain_oracle_mapping_preserves_signal_kinds`
  (rule 12 kind), `h2_sink_aligned_oracle_classifies_exposed_and_suppresses_repair_gap`
  and `h2_return_prefix_aliasing_aligns` (rule 9),
  `h2_non_aligned_sink_stays_weakly_exposed`,
  `h2_advisory_relation_with_matching_sink_does_not_promote` and
  `h2_token_substring_coincidence_does_not_align` (rule 9, assert only
  `!= Exposed` today), `h1_ownerless_file_boundary_still_blocks`,
  `h1_boundary_on_explicit_other_owner_does_not_block` and
  `h1_dynamic_dispatch_limitation_*` (rule 1),
  `h1_concrete_discriminator_attaches_canonical_gap` and
  `h1_generic_discriminator_produces_no_canonical_repair_gap` (rule 13),
  `ingestion_*` (ingestion),
  `perl_packet_contract_migration_corpus_pins_real_producer_dispositions`
  (rule 1 unrecognized kinds, real-producer owner ids),
  `relation_gate_rejects_file_proximity_only` (edits `relation_kind`
  without re-blessing; rule 14).
- Existing: RIPR-SPEC-0108 byte-pinned reports under
  `fixtures/evidence-promotion-honesty-corpus/reports/perl_*.json`, and
  `fixtures/perl_packet_contract_migration/expected/consumer-dispositions.v1.json`
  (rule 10).
- Planned: one adapter unit test per acceptance example asserting the
  exact class, displayed kind, strength and relation, and the presence or
  absence of the gap and location.
- Planned: `h1_file_proximity_relation_is_not_eligible` uses a
  `discriminator:` digest so its no-location assertion discriminates.
- Planned: corpus packets for examples 27 and 28 beside the RIPR-SPEC-0108
  Perl packets.

## Implementation Mapping

- `crates/ripr/src/analysis/language/perl/mod.rs`:
  - `packet_to_findings`: classification order, rule 7 cap, rule 13
    output gates, rule 12 display;
  - `PerlFactPacket::related_test_evidence_for_change`: rule 3;
  - `PerlFactPacket::classify_related_relation`: rules 4 to 7;
  - `PerlFactPacket::classify_change_from_related_tests`: rule 8;
  - `sink_aligned_observation`: rules 5 and 9;
  - `OracleFact::is_strong_exact`: rule 6;
  - `RelationKind::supports_positive_reachability`: rule 7;
  - `perl_oracle_kind_to_domain`, `perl_oracle_strength_to_domain`,
    `perl_relation_to_domain`: rule 12;
  - `canonical_gap_identity_for_change_with_assertion_shape` and
    `PerlFactPacket::canonical_owner_identity`: rule 13 gap gate,
    unchanged except the `static_unknown` condition;
  - `limitation_kind_blocks_strict_actionability`: rule 1 blocking list,
    unchanged;
  - `recompute_packet_fingerprint` and the step-8 comment in
    `validate_ingestion`: rule 14.
- `crates/ripr/src/analysis/language/perl/static_limit.rs`: rule 1.

## Metrics

- `perl_packet_class_overstated`: acceptance examples whose class,
  displayed strength, relation confidence or repair-shaped output exceeds
  what the rules give; must be zero.

---
