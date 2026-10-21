# RIPR-SPEC-0224: Examined-test miss evidence

This spec is the one language-neutral contract for `RelatedTest.miss` and its
rendered `why` (#5566, parent #5445). The sections below that name Rust,
Python, TypeScript or Perl describe each adapter's slice of it.

Status: proposed

Owner: analysis

Created: 2026-10-04

Linked issues:

- #5344 (finding says "Related tests were found" while listing none)
- #5329 (finding reports `reach: yes` with `related_tests_total: 0`)
- #5356 (`ripr explain` repeats the human-full block)
- #5508 (`observation_unconfirmed` read as an established miss)
- #5498 (Perl findings list tests without a reason)
- #5566 (promote to a language-neutral related-test miss contract)
- #5445 (Python, TypeScript and Perl related tests without a reason)

Support-tier impact:

- None. No language support claim changes; the contract discloses facts a
  producer already computes and promotes no adapter tier.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- The controlled output enum `related_test_miss` is registered in
  `policy/output_contracts.txt` and `docs/OUTPUT_SCHEMA.md` by the Rust slice.
  This revision adds no policy entry and changes no gate, badge or report file.

## Problem

A gap finding named at most the tests whose assertions matched the change. A
related test whose assertions all failed to match was dropped, so the finding
could say "Related tests were found" and `reach: yes` while listing no test
(16 findings across 4 of 12 shared-corpus repos, #5344). Where tests were
listed, the finding did not say why each one fails to catch the change, so a
developer who disagreed had to re-derive the analysis.

## Language-neutral contract

`miss` explains one related-test row. It is evidence, not a verdict, and a
renderer never decides it.

### Producer and renderer responsibility

A language producer owns, per test row: which test is related; relation
certainty and provenance; assertion admission state; the assertion judged for
the changed behavior family; row-local activation or boundary evidence;
row-local owner/sink observation state; row-owned discriminator support; and
the final typed miss, if one is established. Renderers, transports and policy
facades own none of these.

### Class gate

- `exposed`: no class-derived row miss. Row-level facts such as a
  `no_call_path` on a matched row whose relation does not establish reach may
  still appear.
- `static_unknown` and every other explicitly unknown, blocked or partial
  state: no speculative row miss. A withheld `static_unknown` finding may
  restore the misses its reveal pass established for its rows.
- A concrete gap class may carry a miss only on a row for which the producer
  established it. A row inside a concrete gap stays `miss: null` when its own
  evidence is unresolved.
- A producer never changes the finding class to obtain a more satisfying
  explanation. Language deviations follow the existing classification
  taxonomy; no cross-language class exists for this feature.

### Explanation is non-decisional

`miss` and `why` cannot determine gate, actionability or fix-site eligibility
(#5563). A row's decision or evidence role is a separate producer-owned fact,
so changing only the explanation changes no decision field.

### Row-owned support

- `missing_input` requires a typed predicate-activation or boundary fact that
  applies to the same row.
- `missing_exact_assertion` requires a typed changed-observable or
  discriminator fact that applies to the same row.
- A finding-wide fact is never attached to one test by renderer convention. A
  row without its own support falls back to `observation_unconfirmed` or no
  miss; it never borrows another row's fact (#5564).

### Assertion admission

Three states stay distinct: a recognized assertion; established absence of any assertion-like
construct after complete extraction; and an assertion-like, refused, custom or
dynamic form whose admission is unresolved. An empty recognized-assertion
vector is not by itself `no_assertion`. `assertion_not_credited` is not a
bucket for unsupported syntax; only a typed established refusal (#5446) may use it. Rust's current
emission on a refused row predates this rule and stays a documented exception
until #5446 lands.

### Minimum evidence per miss

| miss | Minimum evidence |
| --- | --- |
| `no_call_path` | no established path to the changed owner under the producer's relation model |
| `no_assertion` | path established, extraction complete, no executable assertion-like construct |
| `assertion_not_observing` | affirmative evidence the admitted assertion observes another owner, value or sink |
| `assertion_not_credited` | a typed established refusal accepted by #5446 |
| `weak_assertion` | an admitted, family-relevant oracle at a weak or smoke tier, below the producer's strong or exact threshold |
| `missing_input` | a row-owned predicate-activation or boundary omission |
| `missing_exact_assertion` | a row-owned observed-surface or exact-discriminator omission |
| `observation_unconfirmed` | a path/assertion relationship exists but observation can be neither proved nor refuted |

The typed `ProbeFamily` is authoritative for behavior-family prose; source
text such as ` == ` never decides whether a fact is a boundary, return value,
field, error variant, effect, call deletion or match arm.

### Adapter capability table

This table documents which propositions each adapter may emit today. It is
normative documentation, not a second capability registry; once #5531 lands it
is validated against or derived from that registry (which has no `miss` field yet;
adding one is a follow-up). An adapter that reports an
unsupported state as unknown and fabricates no reason conforms.

| miss | Rust | Python | TypeScript | Perl v1 |
| --- | --- | --- | --- | --- |
| `no_call_path` | yes | yes (name- or location-only relations) | pending #5495 | no |
| `no_assertion` | yes | no (#6657) | pending #5495 | no |
| `assertion_not_observing` | yes | no (credit-only alignment; `is_unmatched` coupling) | pending #5495 | no (#3223) |
| `assertion_not_credited` | yes, unsplit until #5446 | no (#5446) | no (#5446) | no |
| `weak_assertion` | yes | yes (weak and smoke only) | pending #5495 | no |
| `missing_input` | yes | yes (predicate boundary, row-owned) | pending #5495 | no (#4061, perl-lsp-swarm#17237, #5562) |
| `missing_exact_assertion` | yes | no (no row-owned fact) | pending #5495 | no |
| `observation_unconfirmed` | yes | yes (strong assertion not credited by its own row) | pending #5495 | yes |

Rust "yes" means the reason is emitted; it does not claim conformance with every
law above (see Current Rust conformance gaps).

"Pending" means the producer has not mapped that reason yet; the row-evidence
prerequisites for it are specified in this document. Perl's narrower coverage is
a capability difference, not a parity failure.

Python (#5491) writes a miss only on a `weakly_exposed` finding without a
static limit, one row at a time, in `python/row_miss.rs`:

- `no_call_path`: `same_stem`, `test_name_similarity` and `fixture_name`
  rows. `dunder_protocol` rows reach the owner through an implicit protocol
  call, so they keep no miss, as do fixture-owner, helper-owner and
  activation-limited rows.
- `weak_assertion`: the family-selected assertion is weak or smoke. A medium
  assertion (a mock that may pin the changed call) keeps no miss.
- `missing_input`: a strong family-selected assertion on a predicate change
  whose own owner calls all bind, resolve both compared operands and place
  neither on the boundary. An unresolved operand or an unreadable call is
  unknown, and another row's input never counts.
- `observation_unconfirmed`: a strong family-selected assertion that the row,
  evaluated alone, does not credit as observing the changed sink. Python's
  alignment only credits; no credit never becomes an affirmative mismatch.
  A row whose callee is a local stand-in of the owner stays here: stand-in
  detection is textual and errs toward withholding credit, so it cannot
  support an affirmative no-call claim.

Python does not yet write:

- `no_assertion`: `extraction_complete_no_assertion` is not an established
  absence while the cross-file blind spots of #6657 (conftest autouse
  fixtures, plugins, ini `filterwarnings`, bare sibling imports) are
  unaccounted for.
- `assertion_not_observing`: alignment cannot separate an affirmative sibling
  sink from an unresolved one, and the value marks a row examined-only
  (`RelatedTest::is_unmatched`), which `oracle_related_tests` and its
  decisional consumers read; emitting it would move ordering and witnesses.
- `missing_exact_assertion`: `missing_discriminators` is finding-wide and no
  row-owned observed-surface fact exists.
- `assertion_not_credited`: no typed refusal (#5446). Skipped, xfail and
  expected-failure tests keep the classifier's `static_unknown` and no oracle
  credit.

### Projection law

Every surface that carries related tests derives from the stored enum and the
shared renderer: human and human-full, JSON/context/JSONL, explain, LSP, MCP,
applicable agent and repair projections, and single-row TypeScript preview
cards or packets. No transport defines its own miss semantics.

### Compatibility

`miss` is optional; artifacts that predate it read as no miss. The enum is
controlled in `policy/output_contracts.txt`. Additions are additive and bump
the schema only as `docs/OUTPUT_SCHEMA.md` requires. Explanatory prose is
excluded from deterministic identity.

### Current Rust conformance gaps

The contract above is the target. The Rust slice described under Behavior does
not yet satisfy all of it, and these gaps are tracked rather than claimed away:

- Decision independence (#5563): `RelatedTest::is_unmatched()` is derived from
  `miss == assertion_not_observing`, and `Finding::oracle_related_tests`,
  `exact_oracle_gap_next_step` and the diagnostic witness read it, and
  `exact_oracle_gap_next_step` also branches directly on
  `miss == missing_exact_assertion`, and the Rust oracle pass reads
  `assertion_not_credited` when choosing the macro-wrapped-assertion limit and
  its next step. Changing a row's miss can therefore change triage ordering,
  next step, limit kind or fix-site selection. These readers are examples, not
  a complete inventory; #5563 owns the inventory and the decoupling.
- Row-owned support (#5564): the annotation pass computes boundary and
  exact-assertion presence once from the finding-wide `missing_discriminators`
  list and assigns `missing_input` or `missing_exact_assertion` to eligible
  rows, so a row can carry a fact that belongs to another test.
- Family-typed rendering (#5493): the reason renderer ignores the finding's
  `ProbeFamily` and hard-codes its fact lookups, and `input_boundary_fact`
  selects predicate facts by ` == ` text.

Until those land, Rust is listed as supported in the capability table only for
the vocabulary it emits, not as conforming to decision independence, row
ownership or typed-family rendering. New adapters must meet the contract as
written and must not copy these gaps.

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
  `exposed` findings and the unknown classes get no class-derived miss; row-level
  facts such as `no_call_path` can still appear on them.
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
- `miss` is evidence only (target; the Rust exceptions are listed under
  Current Rust conformance gaps). The target is that no stage, class,
  confidence, stop reason, limit or next step reads it; the Rust readers are
  those gaps. Post-classification gates that asked whether any related
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
- `output::related_test_miss::oracle_projection` owns the oracle phrase of a
  related-test row, after "uses" (#8388): `<strength> <kind> oracle` for a
  graded row; `<kind> oracle, not graded against this probe` when the
  strength is `unknown` (the probe family has no typed oracle relationships,
  so the assertion is not graded, not of unknown quality); `an unclassified
  oracle, not graded against this probe` when the kind is also `unknown` and
  an assertion was selected; and `no oracle` when no assertion was selected.
  Pinned by `oracle_projection_separates_ungraded_assertions_from_absent_ones`.
  Finding hover, fix-site diagnostic links, seam diagnostic links and targeted
  agent briefs use this same phrase (#8951). The diagnostic witness owns the
  selected test; prose does not reselect a stronger row. Finding hover retains
  its reason-only form when no assertion is recorded. Compact seam-hover
  metadata and structured oracle kind/strength fields retain their raw tags.
- `ripr explain` adds a "Why this verdict" section: every retained examined
  test with its verdict and checked assertion, what a test would need to change
  the verdict (gap classes only), and the meaning of each stop reason.

### Assertion admission prerequisite (Python, TypeScript)

Which adapters write a miss is stated in the capability table above. Before a
producer may write a `no_assertion` miss, each extracted test carries an assertion
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
   - Outside every test body (with its imports dropped), the test file holds
     only inert lines: `describe('name', () => {` openers, their closers and
     comments. A hook, helper, global getter or loader there refuses,
     whatever its spelling. It has no side-effect import (`import './setup'`)
     and no `import x = require(...)` or `import (...)`; each import must be
     one complete static import with nothing after its terminator.
   - The file imports only the owner's names, owner-module namespaces,
     the owner module's own `const` integers and test frameworks. An
     UPPER_CASE import from any other module refuses, since it may be a
     function or getter that reaches the owner.
   - The body is closed: every statement is
     `expect(<one owner call or body local>)<literal matcher chain>`, a
     `const` bound to one owner call, or a `const` integer. Matcher
     arguments are literals (numbers, plain strings, `true`, `false`,
     `null`, `undefined`, object keys) or body `const` integers.
   - Every owner-call argument, read or not, is a plain integer or a
     resolved constant, every read one is off the boundary, and no
     constant argument is rebound by an enclosing scope.

   Project-level setup files (vitest `setupFiles`, jest
   `setupFilesAfterEnv`) are outside this rule's view; a hook there that
   calls the owner is a known limit, as is a module the owner's own module
   imports for its side effects that registers such a hook.

   A visible owner call at the boundary is checked first and yields
   `reached_without_discriminator`. `missed_boundary` is the only state that
   may support a row-owned `missing_input`.
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
  cases; that needs the typed admission refusal tracked by #5446 (see #5359).
- Per-adapter miss mappings: the capability table states which reasons each
  adapter writes, and each producer PR owns its own column. Richer Perl reasons that need row-owned facts (#5562).
- Any producer implementation, parser, call-graph or oracle expansion,
  support-tier promotion, or runtime or mutation-kill claim.

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

- The `oracle_wording` controls in `lsp/tests.rs`, the seam diagnostic tests and
  `output/agent_seam_packets.rs` pin ungraded classified and unclassified rows,
  graded rows, reason-only absent-assertion hover and producer-owned fix-site
  selection beside a stronger row.
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
  data, and MCP gap evidence; no projected surface adds a decision that reads
  `miss` beyond the Rust gaps listed above.
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
| `crates/ripr/src/domain/diagnostic_witness.rs` | selected typed fix-site test shared with diagnostic prose |
| `crates/ripr/src/output/agent_seam_packets.rs` | targeted brief oracle prose |
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
