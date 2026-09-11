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
- Conditions without a structural negation (opaque predicates, method
  calls, `matches!` shapes) produce no oracle: exactness is never
  inferred from messages or names.
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
  resolved callee result (`match path::to::callee(..) { .. }` — no
  method receiver, no trailing `?`, no macro, no chain) whose block
  holds at least one `Err(` arm plus an `Ok(` arm or a catch-all arm
  carries a `guarded_result_match` oracle when every Err arm both pins
  and terminates. Pins: the arm pattern proper, a
  `matches!`/`assert_matches!` pattern in body or guard, or a guard
  equality `==`/`!=` against a variant path rank strong; a bare
  concrete downcast pin ranks medium. Terminal: a loud failure body
  (`panic!`/`assert!`/`bail!`/`return`/re-raise/unwrap), or a guarded
  accept arm (`Err(e) if <pin> => {}`) whose catch-all arms all fail
  loudly — the historical `expect_response` routing shape. The oracle
  binds to the scrutinee callee, so a probe whose owner is that callee
  gains observation without changed-line token overlap. No credit from
  a bare `?`, terminal `Ok(())`, wildcard or no-op Err arms, silent
  catch-alls, message-only diagnostics, shadowed callee names, or
  shapes too weak to pin; one pinned arm beside an unpinned escape arm
  is not an exact identity and stays unrecognized.

## Required Evidence

- The `assertion_form_parity_err_guard` and
  `assertion_form_parity_assert_msg` fixtures: the same owner, boundary
  value, and observable under the two equivalent forms, with identical
  oracle kind/strength, classification, and gap accounting.
- In-crate parity pins: the guard's oracle equals its assert twin's
  (kind and strength); opaque guards stay unrecognized.
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
