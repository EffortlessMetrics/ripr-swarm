# RIPR-SPEC-0210: Installed TypeScript journey fixture

Status: proposed

Owner: test-infra

Created: 2026-10-04

Linked issues:

- #4519 (this slice: the literal installed TypeScript repair journey)
- #4508 (parent installed-journey matrix; TypeScript row)
- #4600 (blind installed-agent acceptance; consumes the row)
- #3797 (blind journey authority)
- #4510 (shared candidate/process harness; admission authority, referenced not
  copied)
- #4604 / RIPR-SPEC-0205 (executor consumer; the corpus rows this fixture
  names must execute through its gate)
- #4603 / RIPR-SPEC-0200 (blind-journey contract; every emitted packet is
  stamped and validated through it)
- #9103 (slice D: the scripted rows follow the gap-bound repair start, after
  phase and receipt that RIPR-SPEC-0311 delivered)
- #4410 / #4429 (accepted TypeScript same-file constant-threshold
  implementation authority; defects return to its owner)

Support-tier impact:

- None. The fixture is a retained small npm/Vitest repository plus one
  manifest; it runs no process, edits nothing, calls no provider and collects
  no telemetry. [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. The fixture adds committed text/JSON under `fixtures/`; the dedicated
  `check-fixture-contracts` validator owns its contract and reads only the
  committed bytes.

## Problem

RIPR-SPEC-0205 (#4604) made the #4516/#4518/#4519 literal journeys
*scriptable* but deliberately left them *unauthored*, and RIPR-SPEC-0207
(#5126) delivered the Rust language row. #4519 owns the TypeScript language
row: one bounded npm/Vitest repository with a committed production boundary
change and a related test that misses the exact boundary, plus every
rebindable, imported, shadowed, computed and nonliteral negative shape the
accepted #4429 implementation contract leaves unresolved, following only
public docs/help and literal product-emitted commands. Without a retained,
digest-bound fixture, a scripted TypeScript row could claim a journey over a
repository the gate never sees, and the #4508/#4600 consumers would have no
fixture identity to reference.

## Behavior

`fixtures/blind_journey_installed_typescript/` commits the fixture:

- `repository/base` and `repository/head` — two bounded snapshots of one
  small npm project (`package.json`, `package-lock.json`, `src/pricing.ts`,
  `test/pricing.test.ts`). The committed production change moves one
  discount-threshold comparison from exclusive (`subtotal > 10000`) to
  inclusive (`subtotal >= 10000`); the related Vitest test covers subtotals
  9999 and 10001 and misses exactly 10000, so the expected repair is one
  equality assertion and no production edit, independently understandable
  from source. Every snapshot's `package-lock.json` is the full lockfile (vitest 5.0.5,
  `npm ci`-installable) that backs the
  npm-runner selection that makes the printed `vitest run test/pricing.test.ts`
  command executable in the documented npm environment.
- `repository/variant-*` — eight negative snapshots of the same changed
  comparison through each shape the accepted contract fails closed on:
  rebindable `let` binding, reassignment, shadowing, imported constant,
  object/namespace write, enum or computed initializer, nonliteral
  arithmetic, and the control-flow reach barrier.
- `manifest.json` (`blind_journey_installed_typescript_fixture.v1`) binds
  every snapshot file by SHA-256, records the reproducible git commit/tree
  identities (pinned author, fixed timestamps, `core.autocrlf=false`), the
  recorded Node/npm/Vitest environment identities, the journey answer
  surface (eligible item, quiet neighbor, expected edit cage, forbidden
  edits, discriminator family), the printed command templates with their
  substitution contract, the seventeen scripted scenario ids, and the control
  mapping.
- `SPEC.md` states the Given/When/Then/Must-Not fixture contract.

`fixtures/blind_journey_execute/corpus.json` gains the seventeen
`installed_typescript_*` scripted journeys (#4519's positive row, its
failure controls, one row per negative variant, foreign-CWD decoy and
second-root repeat included). They execute through the unchanged
RIPR-SPEC-0205 executor: the event chain, per-kind digest presence,
observation binding and derived terminals are exactly the consumer's, so a
hand-edited expectation cannot make a wrong journey emit. The derived
terminals are the honest machine results — `passed_blind_journey` for the
positive row, the pilot/status route that resolves to the gap-bound start, the
second-root repeat and the completed-attempt rerun re-observation (the
row id keeps `interrupted_receipt_write_recovers`, but no interruption or
lost-receipt recovery is exercised or claimed);
`verification_not_run_visible` for the eight negative variants (packet
readiness false with the precise printed reason, never a delegatable packet
or fabricated receipt) and the skipped-verification row;
`verification_failure_visible` for the failing printed Vitest command with
static movement kept on its own axis; `product_discoverability_failure` for
the retained cyclic pilot/agent-status control route; `honest_limitation` for the deleted
before-artifact row that names its printed recovery; and
`candidate_identity_failure` for the planted workspace binary.

The dedicated validator `validate_blind_journey_installed_typescript_fixture`
(`cargo xtask check-fixture-contracts`) owns the fixture contract: the
manifest schema, the per-file digest bindings recomputed against the bytes
on disk, the well-formed recorded git identities, the edit-cage invariants
(selected repair target inside the cage; cage and forbidden edits
disjoint), the requirement that every scripted scenario id the fixture names
exists in the committed executor corpus, and the requirement that every
named scenario's candidate identities bind the manifest snapshot its
`scenario_snapshot_bindings` entry designates — never the unchanged `base`
snapshot and never another variant.

For the Rust and Python rows, the receipt template and three positive scripted
receipt inputs per language include the workflow verification artifact, the
language's authored seam identity and JSON output required by the public parser.
The existing public CLI receipt-attempt control projects each committed template
onto a genuine Rust repair attempt and checks receipt binding plus
incomplete-argument refusals. That parser/control proof does not execute the
authored installed-language fixture or establish blind acceptance; the real
candidate-bound installed journey remains separate.

The TypeScript rows follow the gap-bound route that RIPR-SPEC-0311 delivered
(#9103, slice D). `ripr check --base 'HEAD~1'` prints the gap-bound start
`ripr agent repair --gap-id <gap> --base 'HEAD~1' --phase before` for the
complete packet; `ripr pilot` and `ripr agent status` route back to `ripr check`
and terminate there instead of cycling; the printed `ripr continue --attempt`
runs the gap-bound after phase, which closes the gap only on positive `exposed`
evidence and issues the gap-bound receipt itself. The manifest therefore carries
no separate `agent_receipt` template, each TypeScript positive row records the
issued receipt once and scripts no seam-bound `ripr agent receipt` command, and
the CLI receipt control asserts exactly that for TypeScript. The eight negative
variants retain the verbatim not-ready diagnostics of `ripr check` and of the
refused `agent repair --gap-id ... --phase before` (compared against the real
binary by the installed journey test) and are offered no gap-bound start. All
other scripted output bytes are semantic summaries, not literal transcripts. The
status warning `pilot_routed_to_check_no_repair_target` derives from the same
gap-start admission the routed `ripr check` uses (#10001): when check prints the
gap-bound start, the warning names that start instead of claiming `ripr check`
"offers no `ripr agent repair` start", and for the eight refused variants it
keeps that claim; the installed journey test pins both. The receipt is static exposure evidence only
(`verification_not_run`); the printed Vitest command remains a separate axis.
The retained cyclic row stays as a negative control proving the executor still
derives `product_discoverability_failure` for a route that cycles without a
terminal instruction; the real TypeScript product no longer produces that
cycle, which the added `installed_typescript_pilot_status_route_resolves_to_gap_start`
row records (pilot to status to check ending in the gap-bound start).
Python keeps its #9102 terminal reason and its rows are unchanged.

`cargo xtask check-fixture-contracts` also rebuilds the snapshot repository
from `repository/<snapshot>/` using the manifest `construction_recipe` (offline,
a handful of local git calls in one scratch repository, with the user and system
git configuration, host identity and index environment, global ignore file,
signing and hooks all neutralized; the variant commit dates and messages follow
the recipe's documented derivation) and refuses any recorded commit or tree id the recipe does not
reproduce, so a well-formed but hand-edited id cannot pass (#9235).

## Required Evidence

- `cargo xtask check-fixture-contracts` recomputes every snapshot digest and
  cross-checks the manifest against the committed executor corpus on every
  policy pass.
- `cargo xtask blind-journey-execute` executes all seventeen
  installed-TypeScript rows through the real executor in CI, requires the
  live RIPR-SPEC-0200 validator to accept every emitted packet, and writes
  `target/ripr/reports/blind-journey-execute.{md,json}`.
- The executor decision receipt
  `metrics/blind-journey-execute/executor-receipt.json` binds the enlarged
  corpus scenario count and records the fixture-binding limitation.
- `cargo test -p xtask installed_typescript_fixture` covers the validator
  units: committed-manifest digest binding, corpus cross-reference,
  unknown-scenario refusal, drifted-digest refusal, unlisted-snapshot-file
  refusal, scenario designated-snapshot binding (including wrong-variant and
  missing-binding refusal) and git-identity well-formedness.

## Non-Goals

- No candidate execution, install, packaging, process admission or release
  verdict; #4510 stays the harness authority and the frozen #1609 candidate
  run stays the real execution authority.
- No product command or repair-implementation change; #4410/#4429 keep code
  ownership and a discovered defect returns to the narrow owner.
- No Rust or Python rows; #4516/#4518 stay the sibling journey owners.
- No blind-operator assessment; #4600 keeps that authority and the scripted
  prompt surfaces stay generic and contamination-free.
- No claim that every TypeScript project or boundary expression is supported
  and no mutation-adequacy or runtime-mutation result anywhere in the rows.

## Acceptance Examples

1. The positive row follows only public docs/help and literal
   product-emitted commands from `ripr doctor` through `ripr check`,
   `ripr pilot`, `ripr agent status`, the printed gap-bound `ripr agent
   repair --gap-id <gap> --base 'HEAD~1' --phase before` that emits the
   packet naming `src/pricing.ts` owner, the 10000 equality input and the
   executable `vitest run test/pricing.test.ts` command, the bounded edit,
   the exact printed Vitest command and the printed `ripr continue
   --attempt` after phase, which issues the gap-bound receipt; every
   emitted packet is stamped and accepted by the live validator with
   `passed_blind_journey`. The pilot/status route row resolves to the same
   start without cycling.
2. Each negative variant stops before any edit: packet readiness stays
   false with the precise printed reason for its binding or reach shape,
   and the row derives `verification_not_run_visible` without a delegatable
   packet or fabricated receipt.
3. The failing printed Vitest command derives `verification_failure_visible`
   while the static movement stays on its own axis and the static-only
   receipt does not claim the test passed; the skipped-verification row keeps verification
   `not_run` even though static movement improved.
4. The retained cyclic pilot/agent-status control route derives
   `product_discoverability_failure` with the exact blocked public input;
   the deleted before-artifact row stops with `honest_limitation`, names
   the printed recovery route, and claims no static or receipt state.
5. The planted workspace binary derives `candidate_identity_failure`; the
   foreign-CWD decoy row and the second-root repeat keep the selected root
   authoritative and share one portable identity.
6. A manifest whose snapshot bytes drift, whose git identities are
   ill-formed, which names a scenario the corpus does not contain, or whose
   scenario candidate binds a snapshot other than its designated entry fails
   `check-fixture-contracts`.

## Test Mapping

- `xtask/src/fixture_contracts/installed_journey_recipe.rs`
  (`installed_journey_recipe_tests`): the committed fixtures reproduce every
  recorded commit and tree id from `construction_recipe`; a forged but
  well-formed commit id is refused; a changed snapshot byte changes the
  rebuilt ids and is refused; a recipe without the base and head steps fails
  closed; inherited git repository selectors do not redirect the rebuild; a
  snapshot carrying `.git` is refused; a recipe that no longer states the
  variant derivation is refused.
- `xtask/src/fixture_contracts/general_validators.rs`
  (`installed_typescript_fixture_tests`): committed manifest binds the
  snapshot bytes; committed scenarios exist in the executor corpus; an
  unknown scenario id is reported; a drifted snapshot digest is rejected; an
  unlisted snapshot file is rejected; every named scenario candidate binds
  the manifest snapshots; a candidate bound to the wrong variant snapshot or
  missing its designated-snapshot binding is rejected; git identity binding
  accepts only lowercase 40-hex.
- `xtask/src/reports/blind_journey_execute.rs` gate tests: the enlarged
  committed corpus and receipt validate against the live executor and the
  required-scenario coverage cannot be dropped.
- `cargo xtask blind-journey-execute` and `cargo xtask check-fixture-contracts`
  are themselves the CI-executable proof; hosted CI is the execution
  authority for every step that would need a built binary.

## Implementation Mapping

| Surface | Responsibility |
| --- | --- |
| `fixtures/blind_journey_installed_typescript/` | retained snapshot repository, digest-bound manifest, fixture SPEC |
| `fixtures/blind_journey_execute/corpus.json` | seventeen scripted `installed_typescript_*` journey scenarios with committed executor expectations |
| `metrics/blind-journey-execute/executor-receipt.json` | ratification receipt binding the 62-scenario corpus with fixture limitations |
| `xtask/src/blind_journey_execute.rs` | required-scenario coverage extended to the seventeen installed-TypeScript rows |
| `xtask/src/fixture_contracts/general_validators.rs` | dedicated fixture validator with digest recomputation and corpus cross-reference |
| `xtask/src/reports/fixtures.rs` | manifest-only fixture directory registration |
| `docs/specs/README.md` + `.ripr/traceability.toml` | spec registration and test traceability |

## Metrics

- `blind_journey_execute_scenarios` (62, including 17 installed-TypeScript
  rows)
- `blind_journey_execute_positive_scenarios` (13, including the four
  installed-TypeScript positive rows)
- installed-TypeScript row terminals by control (positive, not-run,
  verification-failure-visible, discoverability-failure,
  honest-limitation, candidate-identity-failure)
- fixture snapshot digest binding failures (target: 0)
