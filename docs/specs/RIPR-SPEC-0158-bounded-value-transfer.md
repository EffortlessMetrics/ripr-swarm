# RIPR-SPEC-0158: Bounded value transfer from exact test inputs

Status: proposed

Issue: #3295 (parent #3215; builds on #3294 / RIPR-SPEC-0157)

## Problem

After #3294 connects a changed binding to its predicate use, the #3215
equality-boundary shape still reports `missing discriminator end ==
start … observed end values: unknown` even when the related test's
exact inputs determine both operands (`input = "ab"`, `delim = 'x'` →
`end = 1`, `start = 1`). Without evaluating the short deterministic
chain that produces the operands, the analyzer cannot distinguish an
exercised boundary from a missing one.

## Behavior

- One typed value-transfer interface
  (`analysis/classify/value_transfer.rs`) evaluates a single-line `let`
  initializer over one related-test call row's exact input literals.
- The value lattice is typed (`Str`, `Char`, `Bool`, `Index`, option
  forms, `CharSeq`); every exact result carries per-step provenance
  (operation family, receiver/argument rendering, chain depth) and the
  source inputs are retained on the observed-value fact text.
- Evaluated families: `find`/`rfind` (present, absent, empty,
  multibyte), `len`, `starts_with`/`ends_with`/`contains`,
  `strip_prefix`/`strip_suffix`, `chars().next()`/`next_back()`,
  `char::len_utf8`, `map_or` with a literal default and the identity
  closure, `checked_add`/`checked_sub`, and bounded string slicing
  from exact indices. Exact integer arithmetic (`+ - * / %`, with
  precedence and parentheses, failing closed on a leading sign,
  overflow, underflow, division by zero, or an operand or result at or
  above `i32::MAX`),
  `split_whitespace().count()` and
  `chars().count()` are credit-only families (#9637): a chain that
  uses one may close a boundary through an observed equality row but
  never names a missing input, so no row on the boundary leaves the
  stage unknown. A comparison operand that is itself such a chain over
  `&str`/`String`/integer owner parameters, same-file constants and
  literals resolves the same way, but only for ordering predicates
  (`<`, `<=`, `>`, `>=`): an `==` or `!=` guard never credits, because one
  on-boundary row cannot discriminate its neighbours. Integer owner
  parameters must be 32 bits or wider (`u32`, `i32`, `u64`, `i64`,
  `u128`, `i128`, `usize`, `isize`); `u8`/`i8`/`u16`/`i16` stay unknown.
  A computed or call-result argument, a generic or slice receiver and a
  rebound parameter stay unknown, and a local initializer that reads a
  parameter that is rebound, assigned, `mut`, or bound by `for`/`if let`/
  `match`/closure earlier (`let x = x + 10;`) is unresolved. A direct
  closed-form operand is known only when every related call row
  evaluates it; a computed local keeps its existing any-row semantics
  (#3295), where one evaluable row suffices. The difference is
  deliberate: the direct form is new and fails closed on a row that may
  sit on the other side.
- Failure modes are typed and named: `Unsupported` names the earliest
  edge (unknown method family, non-identity closure, `map_or_else`,
  dynamic non-input identifier, over-limit chain/literal);
  `InvalidBoundary` names a non-boundary slice index. A `None` option
  arm is consumed inside the chain by `map_or`'s default, so a bare
  not-applicable never escapes. Nothing becomes exact from token
  resemblance, an oracle expectation, or a runtime result.
- Wiring: a comparison operand resolves exactly when it is a parameter
  (the row literal) or a local binding whose live span (verified with
  the #3294 relation) covers the predicate line. Exact equality under
  any row observes the boundary: the missing-discriminator fact
  disappears, `end == start` joins the observed values, and the
  infection stage can reach `yes` at the changed boundary. When the
  evaluator refuses the chain, the operand stays unresolved: since
  #6674 the boundary reads `infection_unknown` with `Changed boundary
  input is unresolved` (RIPR-SPEC-0001) instead of a missing
  `end == start` discriminator with `observed end values: unknown`.
- Char literals in test-call arguments are extracted as exact inputs
  (lifetimes are not).
- Flag-selected locals (#9276): `if flag { A } else { B }` and
  `if !flag { A } else { B }` evaluate to the arm an exact boolean input
  selects, and the other arm is never read. A local's initializer may
  name same-file constants (`let unit = if si { KIB } else { KB };`);
  each joins the row's exact inputs only when its file declares it once
  with a plain decimal value, so an imported, duplicated or opaque
  constant leaves the name unresolved. Two plain decimal integers
  compare by digits, so a test's `1_024` meets the evaluated `1024`. The
  flag and every constant the initializer names must not be rebound in
  the owner (`let si = !si;`, a local `KB`, an assignment or `mut`
  binding); a rebound name leaves the local unresolved. A flag the
  owner mentions anywhere before the initializer (a `for si in ..`, an
  `if let Some(si)`, a closure or match-arm binding) is declined too, as
  scope is not tracked, and a constant the file also imports with `use`
  is never read from a same-named declaration. Braces inside char and
  string literals do not end an arm.

- Method hop (#9329): a helper chain may cross one receiver call
  `<ident>.<method>(args)` into a unique inherent method taking `self` or
  `&self` (never `&mut self`, which may change the field first), when `<ident>` is a parameter of the calling
  function declared exactly `Type`, `&Type` or `&mut Type` for the
  method's own plain single-segment `Type`, never rebound, with exactly
  one call site, and the file does not `use`-import `Type`. The hop
  carries its receiver separately (`HelperHop::receiver`); its positional
  `arguments` exclude it, so the argument-binding consumers stay correct,
  and the single-hop ledger and field-projection consumers decline a
  method hop. A `#[cfg(test)]` helper that makes the receiver call is the
  one evidence-role function kept as the chain's entry, because tests call
  it with the receiver's value. For a free-function callee, a caller whose
  only mentions of the name are receiver calls (`expected.to_string()`)
  is not counted as a caller: a method call never resolves to a free
  function.
- Multi-caller owner (#9329): an owner with several direct callers binds per
  caller. Each caller is resolved on its own, and the chain is the one
  caller a related test reaches (a test calls the caller or a hop above it
  by name). A caller no test reaches contributes nothing. The chain
  declines, naming the multi-caller edge, when no caller is reached, when
  two are reached (the tests' own call path to the owner is ambiguous, even
  through different tests), when a test calls the owner itself, or when the
  owner has more than eight callers; a reached caller whose hop does not
  bind carries its own stop. The binding is per chain, not per test, so a
  second reached caller is a deliberate limit.
- Asserting entry (#9329): a method hop does not forward the owner's result
  by the tail rule, but its top hop may be a `#[cfg(test)]` helper whose
  whole body is one `assert_eq!`/`assert_ne!` with one operand exactly the
  receiver call (`b.to_string_as(si)`, the hop's own arguments) and an
  other operand that names neither the receiver, the method nor a direct
  call of the owner, over hops below it that each tail-forward the call
  and each call the indexed workspace function they name: the callee is
  the one workspace function of that name, and the hop's file neither
  imports nor macro-defines another item of the name and its body does not
  rebind it (a method calling an imported `std::cmp::min` is not a caller
  of a changed local `min`)
  (`helper_transfer::asserting_method_entry`). Propagation then counts the
  chain as forwarding, and the same-test pairing
  (`boundary_pairing::entry_loan_pairs_boundary_call`) pairs the helper's
  discriminating assertion with the transferred `==` row on the line of an
  eager call to the helper, recomputed from that test alone, when that
  call is the helper's only call on the line apart from calls inside its
  own arguments. A dropped, let-bound, transformed or shadowed result, a
  second statement, or a transforming method declines and keeps the
  abstention. A free-function owner's name on a receiver call
  (`expected.to_string()`) is not a call of the owner when judging whether
  the expected side is computed through it.
- Receiver values (#9329): the entry hop's receiver parameter gets one
  extra row cell, the canonical string `Type(<v0>,<v1>..)`, built only from
  `Type(<integer literals>)` for a tuple struct declared exactly once in
  the workspace (text based, file top level, non-generic, integer fields,
  no `enum`/`type`/`fn`/`use` or macro-generated item of the same name), or
  `Type::ctor(<integer literals>)` for a unique `const fn` in an inherent
  `impl Type` whose whole body is one tail expression `Type(<expr>)` or
  `Self(<expr>)`, each `<expr>` a product of constructor parameters,
  literals and same-file integer constants declared once, not rebound and
  not imported, evaluated exactly and checked against the field type
  (`value_transfer::evaluate_integer_product`; an overflow declines). A
  leading `&` or `&mut` on the argument is the same value. Any other
  argument shape yields no cell, and the hop binding then withdraws every
  row. The method hop hands its callee the cell as the pseudo-parameter
  `self`, readable only as `self.<index>` for an index in range; the cell
  is never an owner parameter value, never copied through a plain
  argument, and never compared against a boundary.
- A `mut` parameter is the compared operand like any other parameter (#10306)
  only while the caller's value is what the comparison reads: it is never
  written, or every write (`n = ..`, `n op= ..`, `&mut n`, a method that may
  mutate it, a macro that names it) comes after the comparison's `if`
  condition or sits in the `then` block of an earlier arm of the same
  `if` / `else if` chain, with no loop, closure or second binding of the name
  in the function. Any other shape keeps the boundary operand unresolved
  (`infection_unknown`).

## Required Evidence

- The #3215 reproduction flip (`observed end values: unknown` →
  `infection yes: Detected related test input at the changed boundary`,
  `end == start` observed) and the removal experiment: without the
  evaluator call the computed-local test regresses to `unknown`.
- Table-driven family tests in `value_transfer.rs` (each family with
  positive, negative, empty, multibyte, overflow, and boundary shapes —
  different inputs produce different exact results, so no family can
  be replaced by a constant).
- The classifier wiring test
  (`activation_evidence_resolves_computed_local_boundary_operands`) and
  the end-to-end diff-analysis test.
- The public `method_hop_relations` integration target (#9603) pins
  receiver-only rejection, helper reach despite a receiver-name collision,
  genuine bare and module-qualified free calls through the body fallback,
  and the existing relation fallback without a method hop.
- Golden blast radius measured: the equality-boundary fixture flips
  (the intended behavior change) and `observation_unverified_call_deletion`
  gains the additive boolean observed-value fact; `cargo xtask goldens
  check` and `cargo xtask dogfood` green otherwise.

## Required guards

- The evaluator never runs arbitrary code; only the enumerated
  families with explicitly implemented semantics.
- Scope verification is mandatory: the initializer must feed the
  predicate through the #3294 live-span relation.
- Observation/discrimination still requires the test's own oracle; an
  exact input value establishes activation/infection evidence only.
- For a free owner reached through a method hop, related-test name matching
  uses the existing free-call reader for both captured calls and the full
  body fallback. A receiver-only spelling cannot supply owner-call identity
  or hide an actual helper relation; genuine bare and module-qualified free
  calls remain direct owner calls (#9603).
- Chain depth and literal size are bounded and the limit is disclosed
  (`Unsupported`), never silently truncated.
- Exact operand comparison uses the classifier's existing literal
  normalization (`comparable_value`): underscores are ignored and
  string quotes stripped, so a computed `"a_b"` compares equal to a
  literal `"ab"`. The normalization is pre-existing and applies
  identically to parameter operands.

- A local that folds to a plain integer is compared only against an
  integer cell of the other operand's parameter. A string cell is the
  literal inside a call argument (`to_string(parse("128.000 GiB"), true)`),
  not the compared number, so that row leaves the local unresolved and
  the boundary reads `infection_unknown` instead of a missing
  discriminator (#9276).
- An equality fact whose one side is an owner parameter and whose other
  side is a local that evaluated to a plain integer names the value, not
  the local (`bytes == 1024`, not `bytes == unit`), on either side of the
  comparison. A test helper that forwards its parameters
  (`fn expect_tier(want, bytes, si) { assert_eq!(want, tier(bytes, si)) }`)
  then pairs the call with its exact assertion through the same-test
  pairing, which matches `parameter == literal` terms against the
  literals the call feeds. A helper that computes an argument
  (`tier(bytes + 1, si)`) or asserts only that the result is non-empty
  still does not pair (#9509).

## Acceptance Examples

1. Accept: `end = input.rfind(delim).map_or(1, |idx| idx)` with
   `input = "ab"`, `delim = 'x'` → `end = 1` exactly; `start =
   delim.len_utf8()` → `1`; the boundary is observed.
2. Accept: `strip_prefix` absent → the `None` arm flows into the
   evaluated default.
3. Reject: exactness from a method outside the families
   (`input.to_ascii_uppercase().rfind(delim)`).
4. Reject: exactness through a non-identity closure
   (`map_or(1, |idx| idx + 0)`).
5. Reject: exactness from a dynamic input, a `map_or` default that is a
   local computed from the input rather than a literal or a bound input.
   When some rows evaluate and one row's chain is refused, the local stays
   unresolved: the refused row may sit on the boundary, so the other rows
   cannot support a missing discriminator.
6. Reject: exactness through a shadowed or out-of-scope binding (`let end =
   end + start - start;` between the chain and the comparison).
7. Accept: `let unit = if si { KIB } else { KB };` with `KIB = 1_024` and a
   related test calling the owner with `(1_024, true)`: `unit` is `1024`
   under that row, equals `bytes`, and the boundary is observed
   (`local-boundary-flag-hit`, `local-boundary-flag-negated`).
8. Reject: the same local with tests at `(1_000, true)` and `(1_024,
   false)`: each input is the other flag's boundary, no row sits on
   `unit`, and the verdict stays a gap (`local-boundary-flag-trap`).
9. Reject: `let si = !si;` before the same local, with tests at
   `(1_024, true)`: the flag the local reads is the opposite of the
   caller's literal, so the pinned 1_024 is not the boundary and the
   verdict stays unresolved (`local-boundary-flag-shadowed`).
10. Reject: `for si in [false] { let unit = if si { KIB } else { KB }; .. }`
    with tests at `(1_024, true)` (`local-boundary-flag-loop`).
11. Reject: a function-local `const KB: u64 = 2_000;` shadowing the file's
    `KB` (`local-boundary-flag-alias`).
12. Reject: the real `bytesize` edit `bytes < unit` to `bytes <= unit`,
    whose only direct row binds `bytes` to a string literal: the verdict
    stays `infection_unknown` (`bytesize-format-unit-first-run`).
13. Accept: `fn check(expected: &str, size: Size, si: bool)` asserting on
    `size.hit_as(si)`, `hit_as(&self, si)` calling `hit(self.0, si)`, and a
    test calling `check("1 big", Size::kib(1), true)` with `pub const fn
    kib(count: u64) -> Self { Size(count * KIB) }`: the owner row is `bytes
    = 1024`, `si = true`, so infection reads `yes` at the changed boundary.
    The helper's one assertion compares the method's result, which returns
    the owner's result unchanged, so the exact pin pairs with the boundary
    row and the verdict is `exposed` (`method-hop-field-read-hit`, and
    `method-hop-field-read-name-collision` where the owner is named
    `to_string` and the helper also calls `expected.to_string()`).
14. Reject: the same chain with rows that never equal the selected unit
    (`method-hop-field-read-trap`): no boundary input is credited.
15. Reject: a receiver rebound before the call
    (`method-hop-field-read-rebound`): the earlier stop is kept.
16. Reject: a constructor argument that is not a literal
    (`method-hop-field-read-nonliteral`): every row is withdrawn.
17. Reject: a constructor shape the product reader declines
    (`method-hop-field-read-second-ctor`, `Size::shifted(1)` shifts instead
    of multiplying): no receiver value is built.
18. Reject: a receiver that is not a parameter, a parameter typed as another
    type, two call sites, a non-unique method, a constructor whose body is
    not one tail expression, a product that overflows the field type, a
    shadowed, duplicated or imported constant, a tuple struct declared
    twice, a named-field struct, and `self.1` for a one-field struct: each
    keeps the earlier stop or withdraws the rows. These keep
    `infection_unknown`.
19. Accept: the same shape with a second caller of the owner that no test
    reaches (`method-hop-multi-caller-hit`, a `Display::fmt`).
20. Reject: one helper call reaching two callers of the owner
    (`method-hop-multi-caller-both-reached`, `infection_unknown`); a method
    that transforms the result (`method-hop-transformed-result`) and a
    helper that drops it (`method-hop-unobserved-result`), both
    `propagation_unknown`.

## Test Mapping

`analysis/classify/value_transfer.rs` `tests`;
`analysis/classify/activation.rs`
`activation_evidence_resolves_computed_local_boundary_operands`,
`exact_values_compare_plain_decimals_by_digits_only` and
`an_integer_local_meets_only_an_integer_counterpart_cell`;
`rust-verdict-corpus` cases `local-boundary-flag-hit`,
`local-boundary-flag-trap`, `local-boundary-flag-negated`,
`local-boundary-flag-shadowed`, `local-boundary-flag-loop` and
`local-boundary-flag-alias`;
`analysis/classify/helper_transfer/method_hop_tests.rs`,
`analysis/classify/helper_transfer/asserting_entry_tests.rs` (including the
wrong-owner import and rebinding negatives),
`analysis/classify/helper_transfer/multi_caller_tests.rs` (the multi-caller
rule: a chain whose callee has several direct callers binds through the one
caller a test reaches, and declines when no caller is reached, when two are
reached, when a test calls the callee directly, or when there are more than
eight callers),
`analysis/classify/helper_transfer/receiver_value/tests.rs` and
`analysis/classify/activation/method_hop_rows.rs` (every admit and decline
rule of the method hop and the receiver value);
`rust-verdict-corpus` cases `method-hop-field-read-hit`,
`method-hop-field-read-name-collision`, `method-hop-field-read-trap`, `method-hop-field-read-rebound`,
`method-hop-field-read-nonliteral` and `method-hop-field-read-second-ctor`;
`analysis/language/rust/mod.rs` end-to-end retarget+evaluation tests;
fixtures `binding_predicate_equality_boundary` (re-blessed flip) and
the #3295 family fixtures.

## Non-Goals

- No macro expansion, no arbitrary closure interpretation, no trait
  dispatch, no cross-function propagation (later #3215 slices).
- Literal-driven match arms are not an operand-value family: a match
  scrutinee reaches the retarget as a probe, but its arms feed sinks,
  not comparison operands; they stay unsupported until a sink-path
  slice consumes them.
- No propagation claim: operand exactness does not prove the changed
  value reaches an observable sink (`propagation_unknown` stays
  honest).

## Implementation Mapping

- `analysis/classify/value_transfer.rs` — the typed evaluator.
- `analysis/classify/activation.rs` — exact operand resolution and the
  boundary/missing-discriminator wiring; char-literal extraction in
  `scalar_values`; the method hop's receiver cell and `self.<index>`
  binding in `helper_transferred_rows` / `bind_helper_argument`.
- `analysis/classify/helper_transfer.rs` — the method-hop resolution
  (`method_hop`), the free-callee caller filter (`chain_callers`, which drops a
  caller that only makes `x.name()` receiver calls, since a method call cannot
  resolve to a free fn) and the multi-caller rule (`resolve_multi_caller`).
- `analysis/classify/helper_transfer/receiver_value.rs` — the one reader of
  a receiver argument and of a `self.<index>` projection.

## Metrics

No new metric; computed-local boundary shapes move from
`infection_unknown`-adjacent weakness to observed-boundary evidence in
existing activation counts.
