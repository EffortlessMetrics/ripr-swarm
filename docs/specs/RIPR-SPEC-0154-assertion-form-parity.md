# RIPR-SPEC-0154: Assertion-form parity

Status: proposed

Issue: #3284 (parent #3213; builds on #3273, #3283)

## Problem

Semantically equivalent test assertions changed the reported
production-gap accounting solely by their syntactic form. A terminal
`if actual != expected { return Err(...) }` guard in a test body — the
manual expansion of a message-carrying `ensure!`/`assert!` — produced no
oracle fact at all, so the production owner's seam lost its observation
and discrimination evidence while the identical `assert!(actual ==
expected, ...)` form credited a `RelationalCheck` oracle. Harness-only
control flow (`if`, `?`, `Ok(())`) also leaked into the repo-mode
production subject inventory through a probe path that never checked the
owner's source role.

## Behavior

- A terminal Err-return guard (`if <cond> { return Err(...) }` and
  `if <cond> { return Err } else { Ok }` shapes) inside a test body is
  recognized as the assertion twin of its condition:
  `<lhs> != <rhs>` ⟷ `assert!(<lhs> == <rhs>)`, `<lhs> == <rhs>` ⟷
  `assert!(<lhs> != <rhs>)`, `!<expr>` and `!(<expr>)` ⟷
  `assert!(<expr>)`. The twin text is classified by the existing
  assertion classifier, so the guard carries exactly the oracle kind and
  strength its `assert!` form would — parity by construction, not a
  parallel strength table.
- Conditions without a supported structural negation (an unnegated opaque
  predicate, method call or `matches!` expression) produce no oracle:
  exactness is never inferred from messages or names. A negated expression
  such as `!matches!(result, Expected::Good(_))` has the supported
  `assert!(matches!(result, Expected::Good(_)))` twin; its classification
  comes from that assertion operand, without importing the failure body's
  `Err` constructor or diagnostics. A discarded matcher computation has
  no twin and remains outside assertion admission (RIPR-SPEC-0001, #5713).
- The same bounded twin admits a whole first `panic!(...)` or `bail!(...)`
  statement in the guard body (#5713). These conventional failure macros are
  recognized only with parenthesized arguments and a statement/body terminator;
  preceding statements, quoted/commented invocations and surrounding recovery
  expressions are refused. Classification and observed tokens come only from
  the condition. This is a narrow extension of the Err-return grammar, without
  general body-divergence analysis or macro expansion.
- Repo-mode probe seeding filters shapes whose owning function carries
  the test/evidence role (`FunctionFact::source_role`, the typed
  function source role), mirroring the diff
  path and the seam inventory: harness plumbing inside production files
  never enters the production subject inventory.
- `#[cfg(all(test, ...))]` module members carry the evidence role like
  plain `#[cfg(test)]`; `cfg(not(test))` and `cfg(any(test, ..))` stay
  production.
- `Result<()>`, terminal `Ok(())`, `?`, and `map_err` create no
  recursive production obligations in any form.
- Broad versus exact oracle forms remain different wherever the
  semantics differ; wrong-target, unrelated-strong, and opaque-helper
  controls stay non-crediting.
- (#3709, bounded addition) A guarded Result match over a direct,
  resolved callee result now produces its own owner-bound oracle
  (`guarded_result_match`). That contract — recognition grammar, pin and
  termination rules, confirmation gates, fixtures, and cache generations
  — is owned by RIPR-SPEC-0175
  (`docs/specs/RIPR-SPEC-0175-guarded-result-match-observations.md`);
  this spec carries no separate guarded-form behavior. The standing
  boundary stays the non-goal below: the guarded Result match is a
  separate, owner-bound producer, not an assertion twin of this spec's
  bounded twin grammar.

## Required Evidence

- The `assertion_form_parity_err_guard` and
  `assertion_form_parity_assert_msg` fixtures: the same owner, boundary
  value, and observable under the two equivalent forms, with identical
  oracle kind/strength, classification, and gap accounting.
- In-crate parity pins: comparison and negated matcher guards equal their
  assert twins in kind and strength, including an explicit exact/strong
  constructor-pattern control. Opaque guards contribute zero facts, including
  diagnostic-only assertion/matcher text and unnegated matcher controls.
- One-line and multiline literal-2 and whole-wildcard twins retain their
  kind/strength through lexical, parsed, inline Trial and resolved helper
  routes (#5713). Named subjects and owner calls are required; condition
  continuation rows belong to the guard, while body and sibling assertions
  keep their distinct coordinates. Discarded computations stay non-crediting.
- Negated block conditions retain their real inline guard boundary. Wrapped
  discarded matcher statements cannot borrow sibling assertions; known pure
  block wrappers retain actual scrutinee observers and their line padding.
  Panic/bail guards retain exact literal and weak wildcard twins, with
  nonfirst, quoted and recovered invocation controls. Independent compiled
  controls separate discarded booleans, consuming assertions and a locally
  resolved bail macro; static fixture recognition adds no runtime claim.
- The repo-mode leak reproduction (cfg(test) helper shapes seeded repo
  probes on main; none after the owner filter) with the production
  shapes still seeding.
- The `cfg(all(test, ..))` role pin with the `cfg(not(test))` control.
- Existing exact-vs-broad oracle fixtures remain green (the classifier
  is unchanged for recognized forms).

## Required guards

- No inference from messages, names, or payload text.
- The existing assertion classifier remains the single classification
  authority; the guard path only constructs the twin text.
- Harness-role owners are excluded from repo probes by role, not by
  syntax.
- Production-source functions using the same `if`/`?`/`map_err` shapes
  remain ordinary production subjects.

## Acceptance Examples

- Accept: `if actual != expected { return Err(format!(...)) }` credits
  the same oracle as `assert!(actual == expected, ...)`.
- Accept: `if !matches!(value, 2) { return Err(()) }`
  credits `ExactValue`/`Strong`, equal to its assertion twin; the Err-return
  body does not make it an `ExactErrorVariant` oracle.
- Accept: a cfg(test) helper's `if result != expected { panic!(...) }`
  seeds no repo probe while the production predicate still does.
- Reject: a guard with an opaque condition becoming an oracle; a broad
  `.contains` guard becoming ExactValue; the assert! twin and the guard
  diverging in kind or strength.

## Test Mapping

`analysis/extract/oracles/scan.rs` `err_guard_parity_tests` (twin parity,
opaque rejection); `analysis/probes/repo.rs` `cfg_test_leak_tests` (repo
leak + production control); `analysis/syntax/ra.rs`
`cfg_all_test_tests` (role pin); fixtures `assertion_form_parity_*`.

`analysis/extract/oracles/discarded_matches_tests.rs` pins lexical/parsed
layout parity, wrapped-statement ownership, first failure macro refusal and
independent runtime controls; `analysis/facts/harness_registry/tests.rs`
pins inline/helper twins, negated block boundaries, discarded/opaque controls
and inert macro input. The exact behavior test names are mapped in
`.ripr/traceability.toml`.

## Non-Goals

- No recognition of `match`-arm Err returns in this spec's bounded twin
  grammar; the guarded Result match is a separate, owner-bound producer
  (RIPR-SPEC-0175, #3709), not an assertion twin.
- No recognition of `assert_cmd` chains, or
  stdout `.contains` integration forms (later slices of #3284's corpus
  table).
- No change to recognized-form classification strengths.
- No cross-surface role projection (#3285).

## Implementation Mapping

- `analysis/extract/oracles/scan.rs` — guard recognition +
  twin construction.
- `analysis/probes/repo.rs` — owner-role filter.
- `analysis/syntax/ra.rs` — `cfg(all(test, ...))` membership.

## Metrics

No new metric; existing oracle-kind histograms now count the guard form
under its twin's kind.
