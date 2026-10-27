# RIPR-SPEC-0164: Bounded literal match-arm transfer

Status: proposed

Issue: #3296 (parent #3215; builds on #3295 / RIPR-SPEC-0158 and the
helper-chain slice of #3296 / RIPR-SPEC-0159)

## Problem

After the scanner slice (RIPR-SPEC-0163), the second literal-driven
family in the #3296 corpus still reports an unknown operand: a helper
whose whole body is one `match` over a string with literal arms
(`match kind { "word" => "alpha", _ => "other" }`). The let-chain
helper evaluator requires an identifier binding tail, so the match
body fails closed and a caller-side equality boundary
(`let final_label = label(input); if final_label == "alpha"`) keeps
`weakly_exposed` with a missing-discriminator hint even when every
related test row pins the exact label per arm.

## Behavior

- One match-arm authority (`analysis/classify/match_transfer.rs`)
  evaluates a helper's return when the body is exactly one `match`
  tail expression: `match <scrutinee> {` followed by literal arms and
  the closing braces. Any other line — a let-chain, a statement after
  the match, a match that is not the tail — refuses the whole shape
  (the scanner authority at RIPR-SPEC-0163 keeps the state-loop shape;
  the let-chain evaluator keeps binding tails).
- The scrutinee resolves through the shared #3295 evaluator
  (`evaluate_initializer`): a bound parameter or any supported
  family expression, and it must resolve to a string or a char. Any
  other scrutinee (an integer, an enum) is a named limitation, not a
  guess.
- An arm is `<pattern> => <value>,` or `_ => <value>,`. A pattern is
  one literal or `|`-joined alternatives, each a whole string or char
  literal that evaluates exactly through the shared #3295 literal
  parser, so an escape decodes the way the caller's argument does and
  a `|` inside a literal is part of the string. Every literal has the
  scrutinee's type; a literal of the other type refuses the whole
  match, as it would not compile. The shared char parser still rejects
  an escaped char (`'\n'`), so an escaped char pattern or argument
  refuses; escapes resolve for string literals only. Values are plain string literals or
  one nested direct call. A guard (`if`) that evaluates exactly over the
  bound inputs — one comparison whose operands each resolve through the
  shared evaluator, or `str::len()` on an exact string — selects the
  arm only when it is true; a guard that does not evaluate, a bare
  binding, a range, a
  path, or a value the shared authorities cannot evaluate refuses the
  whole match: an unresolved pattern could match anything and a
  non-literal value is not an exact return.
- A helper whose only body line is one whole literal returns it
  whatever its inputs are (the nested-call target `fn pick(_k: &str)
  -> &'static str { "alpha" }`).
- Arm selection is first-match in source order, exactly like the Rust
  match: the first arm whose literal equals the scrutinee (any
  alternative), or the first `_` arm reached, produces the return. A scrutinee no literal
  names with no `_` reached stops the evaluation (no arm is invented).
- On resolution the return is an exact typed value; the boundary
  machinery compares it against the row's other operand exactly as
  #3295 does, and the observed-value provenance names the hop
  (``label = "beta" via helper return of `label` over bound inputs (1
  hop)``).
- Replaced constant (#10304): when the changed line is exactly
  `left == literal` (or `!=`) on both sides of the edit, with the same
  left operand and operator and two different single literals (string,
  char or integer), a related test row whose left operand resolves
  exactly to the replaced literal observes the swap: that row takes the
  other branch after the edit, as a row on the new literal does before
  it. A compound predicate, a changed left operand, a named or computed
  constant, a row on neither literal and a weak oracle keep the missing
  equality-boundary discriminator.

## Required Evidence

- A positive fixture (`fixtures/match_arm_positive`) whose diff
  changes the caller-side equality constant: the finding flips to
  `exposed` with per-row exact labels in the observed-value
  provenance, and both non-wildcard arms connect to exact assertions
  (the #3215 literal-match acceptance row).
- A controls fixture (`fixtures/match_arm_controls`) with four
  variants (computed value, guard, bare binding, char). Each test pins
  one row, and that row returns the constant the diff replaced, so all
  four read `exposed` (the replaced-constant rule above); a runtime
  mutant of each edit fails its test. The negatives that must stay
  gaps are the corpus cases `eq-swap-row-on-neither-constant`,
  `eq-swap-compound-gate-off` and `eq-swap-weak-oracle`, and the
  positives `eq-swap-string-old-constant-row`,
  `-integer-old-constant-row` and `-inequality-old-constant-row`. The
  corpus cases `spec-b-0164-alternative-pattern`, `-escaped-pattern`,
  `-char-scrutinee` and `-computed-arm-value` carry the two-row tests
  that credit the other arms.
- Unit tests in `match_transfer.rs` pinning per-arm resolution,
  wildcard/duplicate source order, no-match-without-wildcard stop, and
  the fail-closed family.
- A removal experiment: disabling the match branch in
  `helper_return_value` regresses the positive fixture to
  `infection_unknown` (the unresolved boundary input rule; before #6674,
  `weakly_exposed`).

## Required guards

- The operand path is the RIPR-SPEC-0163 operand jump (a local whose
  initializer is a direct call to a unique helper); argument binding
  stays strict — a literal or the owner's bound parameter. A computed
  argument still stops by name (RIPR-SPEC-0159 edge).
- A bare single-segment identifier is never read as a literal (the
  token-coincidence family): treating the same text as a literal would
  invent evidence. An unguarded lowercase identifier whose arm value
  never names it is read as a catch-all binding in source order (#10680),
  only when the file holding the helper shows the name is a fresh binding:
  no `const`, `static`, import, module glob, unit struct, enum, function,
  type or module of that name in the file or the helper body. A guard on
  it, a value that names it, a capitalized name, `ref`/`mut` and an
  unreadable file refuse the whole match.
- No bound is exceeded silently: the shape is all-or-nothing, and any
  unsupported edge refuses the whole helper rather than skipping the
  arm.

## Acceptance Examples

1. Accept: with
   `pub fn label(kind: &str) -> &'static str { match kind { "word" => "alpha", "text" => "beta", _ => "other" } }`
   and `classify` comparing `label(input)` with `"alpha"`, a diff changing
   the constant to `"beta"` under the tests
   `assert_eq!(classify("word"), "word"); assert_eq!(classify("text"), "blank")`
   exposes — `label("word")` = `"alpha"` and `label("text")` = `"beta"`
   are exact per row, the boundary comparison is observed, and the
   provenance names the helper hop.
2. Accept: a guard the shared authorities can evaluate selects the arm
   exactly (`"word" if kind.len() > 2 => "alpha"` with `kind = "word"`),
   and a guard that evaluates false falls through in source order.
   Reject: a guard that does not evaluate (an unknown operand, an
   unsupported method, a boolean combinator) refuses the whole match.
3. Accept: `"word" | "term" => "alpha"` selects the arm for either
   literal.
4. Accept: an unguarded lowercase binding pattern (`other => "alpha"`)
   whose name the file does not declare or import selects the arm for
   every scrutinee no earlier arm names. Reject: a bare name that is a
   `const`, `static`, import or glob-imported (or a guard or value that
   needs the bound scrutinee) refuses the whole match.
5. Accept: an arm value `pick(kind)` resolves when `pick` is a unique
   helper whose body is one literal. Reject: an arm value such as
   `pick(kind.trim())` that the shared authorities cannot evaluate
   refuses the whole match.
6. Accept: a pattern `"a\tb"` matches the scrutinee `"a\tb"` (both
   decode to a tab) and not the two-character text `a\tb`.
7. Accept: a char scrutinee selects among char-literal arms. Reject:
   a string-literal arm against a char scrutinee, or a range pattern,
   refuses the whole match.

## Test Mapping

- `crates/ripr/src/analysis/classify/match_transfer.rs::tests` —
  per-arm resolution, source order, no-match stop, fail-closed family.
- `fixtures/match_arm_positive` — the flip and the hop provenance.
- `fixtures/match_arm_controls` — two fail-closed variants with zero hop
  provenance and two resolved variants that stay `weakly_exposed`
  because their tests pin one row.

## Non-Goals

- Match arms with unevaluable guards (anything beyond one exactly
  evaluable comparison, such as boolean combinators or further method
  calls), ranges, or paths — these stay named limitations.
- Integer and enum scrutinees and non-string arm values: out of scope
  for this slice.
- A match inside a let-chain or a statement position (only the tail
  expression shape is pinned); recursion and multi-hop returns inside
  the match remain the #3296 remainder.
- Any claim beyond draft-mode exposure evidence; real mutation
  testing confirms later.

## Implementation Mapping

- `analysis/classify/match_transfer.rs` — the shape parser, literal
  rules, and first-match evaluator (new).
- `analysis/classify/helper_transfer.rs` — the match branch sits after
  the scanner branch in `helper_return_value`.
- `analysis/classify/mod.rs` — module registration.

## Metrics

- `unit_test_pass_rate` — the match_transfer unit tests.
- `golden_fixture_pass_rate` — the two fixtures' expected outputs.
