# RIPR-SPEC-0224: Examined-test miss evidence

Status: proposed

Owner: analysis

Created: 2026-10-04

Linked issues:

- #5344 (finding says "Related tests were found" while listing none)
- #5329 (finding reports `reach: yes` with `related_tests_total: 0`)
- #5356 (`ripr explain` repeats the human-full block)
- #5508 (`observation_unconfirmed` read as an established miss)
- #5498 (Perl findings list tests without a reason)

Support-tier impact:

- None. No language support claim changes; the slice discloses facts the Rust
  classifier already computes.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- Adds the controlled output enum `related_test_miss` to
  `policy/output_contracts.txt` and `docs/OUTPUT_SCHEMA.md`. No gate, badge,
  or report file changes.

## Problem

A gap finding named at most the tests whose assertions matched the change. A
related test whose assertions all failed to match was dropped, so the finding
could say "Related tests were found" and `reach: yes` while listing no test
(16 findings across 4 of 12 shared-corpus repos, #5344). Where tests were
listed, the finding did not say why each one fails to catch the change, so a
developer who disagreed had to re-derive the analysis.

## Behavior

- Every related test the Rust classifier examines is retained in
  `related_tests`, including a test whose assertions match nothing. Such a
  test carries its first assertion as `oracle` (checked text), `oracle_kind:
  unknown` and `oracle_strength: none`, so it never supplies an oracle.
- Each retained test may carry `miss`, a `RelatedTestMiss` naming why it would
  not notice the change: `no_call_path`, `no_assertion`,
  `assertion_not_observing`, `assertion_not_credited`, `weak_assertion`,
  `missing_input`, `missing_exact_assertion`, `observation_unconfirmed`. The
  analyzer sets `no_assertion`, `assertion_not_observing`,
  `assertion_not_credited` and the name-only `no_call_path` while matching
  assertions; these are facts about the test and can appear under any class.
  The finding builder sets `no_call_path` for `no_static_path`, and
  `weak_assertion`, `missing_input` (predicate boundary facts and
  RIPR-SPEC-0229 unselected-arm facts),
  `missing_exact_assertion` (error-variant and field facts) and
  `observation_unconfirmed` for `weakly_exposed` and `reachable_unrevealed`.
  `exposed` findings and the unknown classes get no class-level miss.
- `observation_unconfirmed` is an unknown, not an established miss: its sentence
  says ripr could not confirm that the assertion observes the changed
  behavior (#5508). Only `assertion_not_observing` claims the assertion
  observes something else.
- The Perl v1 producer (#5498) sets `observation_unconfirmed` on a row only
  when the finding is `weakly_exposed` and comes from a complete packet with
  no blocking limit (a concrete discriminator is not required), the row is a reachable direct owner call, its linked
  oracle is the strong exact, owner-targeted oracle that earned the weak
  exposure, and the shared sink-alignment check establishes no alignment for
  that row. Unequal sink text stays unconfirmed. Every other Perl row keeps no
  miss: advisory relations, weak or missing oracles, partial or blocked
  packets, and the finding-wide discriminator, which has no test identity.
- `miss` is evidence only. No stage, class, confidence, stop reason, or next
  step reads it. Post-classification gates that asked whether any related
  test survived oracle matching use `Finding::oracle_related_tests`, which
  skips `assertion_not_observing` rows, so their decisions are unchanged.
- Oracle rows are ranked and packed into the eight-row window exactly as
  before; `assertion_not_observing` rows take only the slots left free, so no
  window, fix site, exact-oracle alignment or repair readiness changes.
  Fix-site selection (`DiagnosticWitness`) reads only oracle rows.
- One prose owner, `output::related_test_miss`, renders the reason. The human
  digest appends it in parentheses after the related test; human-full prints
  `misses: <why>; checked <assertion>`; JSON, the context packet and MCP gap
  documents carry `miss` and `why`; LSP hover uses human-full's label and
  reason with its own row shapes (a matched row keeps its oracle strength and
  kind, only an unmatched row uses `<label>: ...; checked ...`, and a row with
  no recorded oracle shows only the reason, where human-full still prints its
  `uses none unknown oracle` projection first), and diagnostics add up to three
  related-information rows that open the examined test.
- `output::related_test_miss::related_test_miss_label` owns the word before
  the reason: `unconfirmed` for `observation_unconfirmed`, `misses` otherwise.
  Human-full, `ripr explain`, LSP hover and LSP related information use it;
  JSON, MCP and the context packet carry no label.
- `ripr explain` adds a "Why this verdict" section: every retained examined
  test with its verdict and checked assertion, what a test would need to change
  the verdict (gap classes only), and the meaning of each stop reason.

### Assertion admission prerequisite (Python, TypeScript)

Python and TypeScript record no miss yet. Before a producer (#5491, #5495)
may write a `no_assertion` miss, each extracted test carries an assertion
admission state, computed fail-closed from the test's syntax:

- `recognized_assertion_present`: the oracle extractor credited an assertion.
- `extraction_complete_no_assertion`: every callee and value in the test, its
  hooks and its file or `describe` setup resolves to something known not to
  assert. Only this state may later justify a `no_assertion` miss.
- `assertion_like_present_but_unresolved`: anything else, including custom
  matchers, helpers, third-party callees, `throw`, rejected promises,
  `done(err)` and asserting setup.

A related test whose state is not `recognized_assertion_present` adds the
evidence line `test_assertion_admission: <state> (<test>)` to its finding.
The line is evidence only: it changes no verdict, ranking, miss or
actionability. Blind spots the scan cannot see (conftest and setup files,
runner plugins) are tracked in #6657 and #6889.

### Family-relevant assertion selection (Python)

A Python miss producer (#5491) must read the assertion a finding actually
judged. Each related test therefore projects one assertion selected for the
changed line, not its strongest assertion overall (#5572):

1. Keep the assertions whose oracle shape can observe the changed probe
   family. Value families (return value, field construction) never take an
   exception assertion; an error path never takes a normal-value assertion or
   a mock expectation, except on a changed `try:`, `except` or `finally:` line,
   where the handler's result is observed by any assertion. Predicates and
   effects admit every shape.
2. Rank an assertion on the changed sink first (for a field change whose
   changed attribute or dict key is known, a sibling-field assertion ranks
   last), then the strongest, then a family-preferred shape, then a whole
   value over a `len(...)` aggregate, then shape, then the later source line,
   then text. Extractor traversal order never decides.
3. The row, the `test_oracle` evidence, sink alignment, the error-path,
   boundary and changed-default gates, and test-side static limits all read
   that one selection. A strong assertion of another family never suppresses
   a static limit.
4. A test whose assertions all observe another family shows no oracle, adds
   `test_oracle_shape: no_<family>_relevant_assertion (<test>)`, and its
   finding's prose names the kind of the strongest assertion it does have.

Class and stages may move only where the strength-only pick was wrong-family
or depended on source order. Delegation stays fail-closed: a weakly exposed
finding with a no-family-relevant row surfaces `alignment_reason:
no_family_relevant_assertion`, and one whose row now shows a different
assertion than the strength-only pick surfaces
`other_behavior_assertion_passed_over`. The gap ledger never delegates either
reason, so no repair card becomes agent-packet eligible because of this
selection.

### Family-relevant assertion selection (TypeScript)

TypeScript classification already judges assertions one at a time through
`ts_oracle_kind_matches_seam` (RIPR-SPEC-0104). Each related-test row now
projects the same choice instead of the test's strongest assertion overall
(#5525):

1. Keep the assertions whose oracle kind `ts_oracle_kind_matches_seam`
   admits for the changed probe family. No second family table exists.
2. The strongest one wins. Equal strength is broken by the later source
   line, then the rendered oracle text, observed expression, expected value,
   matcher and kind. The order of the assertion inventory never decides; the
   later line keeps the previous projection for equal candidates.
3. The row's `oracle_kind`, `oracle_strength` and `oracle` text, and every
   other fact read from it, come from that one assertion.
4. A test whose assertions all observe another family shows no oracle
   (`unknown`) rather than the strongest wrong-family assertion.

Exposure verdicts and the aggregate strongest-family result are unchanged:
the classifier already reads the same rule. Owner-level candidate ordering
and Bun bridge rows have no TypeScript probe family and keep the strongest
assertion. Delegation stays fail-closed: the repair-packet target row and
verify command are chosen by row strength, so when a row's selected strength
differs from the strength-only pick the finding adds
`typescript_assertion_selection: no_<family>_relevant_assertion (<test>)` or
`typescript_assertion_selection: other_behavior_assertion_passed_over
(<test>)`, and `typescript_gap_record_for` emits no repair packet for it.

### Owner-path disposition (TypeScript)

Before a TypeScript miss producer (#5495) can say why a related test does not
catch a change, each candidate must say what it establishes about the
changed owner. `TypeScriptRelationKind` stays the relation's provenance; each
`TypeScriptRelatedCandidate` also carries one internal
`TypeScriptOwnerPathDisposition`, computed once by the same relation and
identity gates when the candidate is built (#5523):

1. Established or present: `trusted_owner_path` (a trusted relation calls
   the owner) and `module_entry_path` (a same-module entry reaches the
   owner; observation in each test is unresolved).
2. Unknown: `owner_name_call_unanchored`, `heuristic_only` (proximity or
   name evidence, no owner-name call), and `unresolved_alias_or_reexport` (an
   import or `require` specifier the resolver cannot place).
3. Affirmative mismatch: `rejected_local_shadow`,
   `rejected_unrelated_import_or_destructure`, `rejected_owner_module_mock`
   and `rejected_spy_fabrication`. An affirmative mismatch wins over an
   unresolved binding in the same test.

`candidate_observes_owner_call` is a projection of the disposition, not a
second gate run. The disposition is not serialized, promotes no relation, and
changes no rank, confidence, class, stage, actionability or rendered output.

### Predicate-boundary activation (TypeScript)

A `missing_input` reason must belong to the row that lacks the input. The
finding-wide RIPR-SPEC-0027 boundary witness cannot carry that: in a mixed
finding one test hits the changed boundary and another misses it. Each
related candidate therefore gets one internal
`TypeScriptPredicateActivation`, computed from the same parsing, owner-call
identity, receiver resolution, shadow guard, constant resolution and
position rules as the witness (#5527):

1. `not_applicable`: the changed line is not a predicate.
2. `witnessed`: a strong, pinned, family-matching assertion of this test
   observes an owner call carrying the boundary input, and its expected side
   is live. The finding-wide witness is "some row is `witnessed`".
3. `reached_without_discriminator`: an owner call in this test carries the
   boundary input, but no such assertion witnesses it. The input is present,
   so this is never a missing input.
4. `missed_boundary`: the owner module pins the boundary statically (the
   `typescript_boundary_input` or `typescript_boundary_parameters` fact), and
   every input the test can feed the owner is visible, by one closed rule:
   - The owner module names the owner only at its declaration, so no
     recursion, mutual recursion or self alias re-enters it.
   - The test reaches the owner through a direct, import-alias or namespace
     call, and its assertion admission is `recognized`.
   - Outside every test body (with its imports dropped), the test file never
     names the owner, its aliases or its namespaces, and it loads no module
     dynamically.
   - The file imports only the owner's names, owner-module namespaces,
     constant-shaped names and test frameworks.
   - The body is closed: every statement is
     `expect(<one owner call or body local>)<literal matcher chain>`, a
     `const` bound to one owner call, or a `const` integer.
   - Every owner call passes plain integer inputs off the boundary, and no
     constant argument is rebound by an enclosing scope.

   A visible owner call at the boundary is checked first and yields
   `reached_without_discriminator`. This is the only state that may support
   a row-owned `missing_input`.
5. `unresolved`: anything else (an untrusted or shadowed path, an unparsed or
   underived boundary, a computed, absent or spread input, or an owner
   reference that is not a plain call). Never a missing input.

Activation is not serialized and changes no class, stage, missing text,
actionability or rendered output.

## Required Evidence

- A unit test that a test whose assertions match nothing is listed as
  `assertion_not_observing` with `none` strength and `unknown` kind, and the
  stages stay `no`.
- A unit test that eight matched tests keep the window when a ninth examined
  miss exists, and the total counts all nine; and that a test's second, strong
  assertion row keeps its slot when seven examined misses exist.
- A unit test that a finding whose only related tests are examined misses has
  no fix site.
- A unit test that each miss renders a reason naming a checkable fact.
- Golden fixtures re-blessed with no change to any finding's classification,
  confidence, severity, stop reasons, missing entries, or next step.

## Non-Goals

- Changing any verdict. Whether a self-contradicting gap should become an
  unknown is RIPR-SPEC-0221's admission rule.
- Splitting `assertion_not_credited` into proven-inert and admission-unproven
  cases; that needs the typed admission refusal from #5359.
- Python and TypeScript miss producers (#5491, #5495); only their admission
  prerequisite is specified here. Richer Perl reasons that need row-owned facts (#5562).

## Acceptance Examples

1. semver 1.0.23, `src/parse.rs:272` changed from `Some(&b'=')` to
   `Some(&b'>')`: the finding now lists `test_comparator_parse` with
   `miss: assertion_not_observing` and the checked
   `assert_to_string(parsed, "^1.2.3-alpha")`; before, it listed none while
   reporting `reach: yes`.
2. On the 20 fast-tier corpus repos, findings whose observe summary says
   "Related tests were found" with zero listed fall from 13 to 0, and
   `reach: yes` with zero listed falls from 5 to 0. Default human output keeps
   its line count (868 lines); JSON grows 5.1%.

## Test Mapping

- `crates/ripr/src/analysis/classify/reveal.rs::tests` — examined misses are
  listed and rank after matched tests.
- `crates/ripr/src/output/related_test_miss.rs::tests` — reason prose.
- `crates/ripr/src/analysis/language/perl/tests.rs` — the Perl v1 rule: only
  a direct, reachable, strong row without sink alignment is unconfirmed. These
  controls consume frozen packets (#5510); whether the live perl-lsp emitter
  produces complete, unblocked packets is tracked by #3216 and #3223.
- `crates/ripr/src/analysis/language/perl/tests.rs` (#5510 section) — the
  packet-backed matrix: advisory, deferred, weak, missing and limited rows keep
  no miss; the reason follows relation, test and oracle IDs under reordered
  packet arrays; and the rule writes no field but `miss`.
- `crates/ripr/src/output/related_test_miss.rs::tests`,
  `crates/ripr/src/output/human.rs::tests`, `crates/ripr/src/lsp/tests.rs`,
  `crates/ripr/src/mcp/gaps.rs::tests` — the same packet-backed row, token and
  shared sentence in check JSON, the context packet, human-full, `ripr
  explain`, the human digest, LSP hover, related information and diagnostic
  data, and MCP gap evidence; no projected decision reads `miss`.
- `crates/ripr/src/output/human/explain.rs::tests` — an unconfirmed row is not
  labelled a miss.
- `crates/ripr/src/analysis/language/python/python_tests.rs` and
  `crates/ripr/src/analysis/language/typescript/admission_tests.rs`
  (`assertion_admission_separates_no_assertion_from_unresolved_assertion_like_forms`)
  — table tests separating established no-assertion from unresolved
  assertion-like forms.
- `crates/ripr/src/analysis/language/python/assertion_selection_tests.rs` —
  the family-relevant selection controls (each required control, parsed
  inventories, reordering, stronger orthogonal assertions, single assertions,
  handler lines, the later-line tie-break, static-limit suppression, and the
  classified row and evidence sharing one selection).
- `crates/ripr/src/analysis/language/typescript/assertion_selection_tests.rs`
  — the TypeScript selection controls (parsed inventories with an
  `old_differs` column, Jest/Vitest, AVA, Node assert and chai forms,
  reordering, the later-line tie-break, unmerged facts, row moves, and
  classifier rows matching the aggregate family result).
- `crates/ripr/src/analysis/language/typescript/owner_path_tests.rs` — the
  owner-path disposition controls: each parsed case asserts relation,
  disposition, parity with the frozen pre-#5523 boolean, and the row oracle.
- `crates/ripr/src/output/typescript_packet_projection.rs`
  (`moved_assertion_selection_is_never_agent_packet_eligible`) — a moved row
  keeps the finding out of the repair packet.
- `crates/ripr/src/app/tests/python_family_selection_packets.rs` — end-to-end
  check output through the gap ledger: no-family-relevant and passed-over
  cards are never agent-packet eligible, and a changed raise keeps its class
  in either assertion order.
- Golden fixtures under `fixtures/*/expected/` — rendered parity.

## Implementation Mapping

| Surface | Responsibility |
| --- | --- |
| `crates/ripr/src/domain/probe.rs` | `RelatedTestMiss`, `RelatedTest::miss`, `Finding::oracle_related_tests` |
| `crates/ripr/src/analysis/classify/reveal.rs` | retain examined tests; set assertion-level misses; ranking |
| `crates/ripr/src/analysis/classifier/finding.rs` | class-level misses |
| `crates/ripr/src/analysis/language/perl/mod.rs` | Perl v1 `observation_unconfirmed` rows |
| `crates/ripr/src/analysis/language/{python,typescript}/admission.rs` | fail-closed assertion admission state per extracted test |
| `crates/ripr/src/output/related_test_miss.rs` | the one prose and label owner |
| `crates/ripr/src/analysis/language/python/assertion_selection.rs` | family-relevant assertion selection per related test |
| `crates/ripr/src/analysis/language/python/{classify,related_tests,boundary,no_behavior,static_limits}.rs`, `python/repo/evidence.rs` | consumers of the one selection; non-delegatable alignment reasons |
| `crates/ripr/src/output/gap_decision_ledger.rs` | never delegates `no_family_relevant_assertion` or `other_behavior_assertion_passed_over` |
| `crates/ripr/src/analysis/language/typescript/assertion_selection.rs` | TypeScript family-relevant assertion selection per related test |
| `crates/ripr/src/analysis/language/typescript/{related_tests,classifier}.rs` | row projection from the selection; `typescript_assertion_selection` move disclosure |
| `crates/ripr/src/analysis/language/typescript/{types,related_tests}.rs` | `TypeScriptOwnerPathDisposition` per candidate; `candidate_observes_owner_call` projects it |
| `crates/ripr/src/output/typescript_packet_projection.rs` | no repair packet for a finding with a moved row |
| `crates/ripr/src/output/human/{sections,evidence_lines,explain}.rs`, `output/json/report.rs`, `lsp/{hover,diagnostics}.rs`, `mcp/gaps.rs` | projections |

## Metrics

- `trust.self_contradictions` (dx-scoreboard rule R3): target 0.
- Share of gap findings whose listed tests carry a reason: 141 of 195 on the
  fast tier at this slice.
