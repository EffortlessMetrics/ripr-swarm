# RIPR-SPEC-0240: Unknown, not a gap

Status: proposed

Owner: product-swarm

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- release-0.11 trustworthy-verdicts scoreboard

Linked issues:

- #5327 (serde `format_u8` false gap), #5432 (sibling-arm oracles),
  #9442 and #9443 (owners reached without a placeable call)

Linked PRs:

- #5318 (labeled Rust verdict corpus), #5359 (typed assertion refusals),
  #5416 (this spec)

Support-tier impact:

- None.
- This narrows when Rust findings claim a gap. It does not promote a
  language, surface, or gate.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Add one `static_limit_kind` value and one `stop_reason` value to
  `policy/output_contracts.txt`, `docs/OUTPUT_SCHEMA.md`, and
  `docs/STATIC_LIMITS.md`. The JSON schema version does not change; the
  values are additive.

## Problem

`reachable_unrevealed` tells a developer that the tests reaching a change do
not check it, and routes repair work. ripr reports it when every related
`assert_eq!` was refused, including refusals that only say ripr could not read
the evidence. On the labeled verdict corpus
([RIPR-SPEC-0219](RIPR-SPEC-0219-labeled-rust-verdict-corpus.md)) serde
`format_u8` is reported as a gap: `test_format_u8` checks all 256 values, but
it carries a feature `cfg` that ripr does not evaluate, so its `assert_eq!` was
refused.

A wrong actionable signal costs more than a missed advisory: the developer
digs, finds the test, and stops trusting the next finding.

## Behavior

A gap is a negative existential claim, so it needs a negative step that ripr
established from evidence it read. When the step rests on an analyzer limit,
the finding is an unknown that names the limit.

[RIPR-SPEC-0197](RIPR-SPEC-0197-owner-return-pins.md) records why each related
`assert_eq!` was refused (`AssertionRefusal`).
`AssertionRefusal::is_analyzer_limit` splits the refusals in two:

| Analyzer limit (gap withheld) | Shape that can keep the assertion from discriminating (gap kept) |
| --- | --- |
| lexical fallback, unparsed file, stale source | `if` branch or other conditional path |
| unplaced module, `include!`d file | closure that may not run or may exit early |
| unidentified test (also the fallback when no gate explains a refusal), `async fn` test | opaque macro or macro operand that may leave the test |
| any other test attribute: a feature `cfg`, `#[rstest]` | `#[ignore]`, `#[should_panic]`, or a `cfg`/`cfg_attr` the shared evaluator (`attribute_test_build_availability`) reads as disabled in every test build |
| the same assertion spelled twice on one line | test nested in a function body; gated enclosing module or gated `mod` declaration |
| a binding that only may rebind the macro: foreign glob, unresolved `#[macro_use]`, macro argument, unparsed file, `no_implicit_prelude`, no site found | a `macro_rules!`/`macro` definition or `use` of the name in reach |

An attribute that cannot change whether or how the body runs (a lint level
such as `#[allow(..)]`, `#[doc = ".."]`, `#[inline]`, `#[must_use]`,
`#[track_caller]`, a `rustfmt::` or `clippy::` tool attribute) refuses
nothing: the test keeps the owner pin, in any position beside `#[test]`. A bare
built-in name that an import or `extern crate .. as` rebinds in the test's
module does not count as the built-in, and refuses as any other attribute does.

The right column is what the compiled runtime controls in
`crates/ripr/tests/owner_pin_execution.rs` prove are real gaps (the assertion
never runs, or runs as something else), plus attributes that decide the
outcome whatever the assertion does. The syntax scan names an
outcome-settling attribute as the refusal wherever it sits among the test's
attributes, ahead of `async` and other attributes, so a feature gate listed
first cannot hide an `#[ignore]`. A gated module stays a gap because
`cfg(any())` is indistinguishable from a feature gate at that refusal.
`cfg(false)` is read through the same shared evaluator, which does not yet
evaluate the literal, so a test gated only by it is withheld rather than kept.
Likewise `#[cfg_attr(test, ignore)]` and `#[cfg_attr(test, should_panic)]`
are not yet read as settling the outcome: the evaluator treats the introduced
attribute as ordinary, so the gap is withheld. Both cases err toward an
honest unknown, never toward a false gap.

An earlier `return` is a gap-keeping shape, with one value-guard exception
(trap-tally-reached-assert-control). The assertion is `assert_eq!(call, e)`
with `e` an integer literal. The test body's only `return` (outside closures
and nested items) is the whole body of a root statement
`if call <op> k { return; }` with no `else`, where `call` is the same text and one bare call `name(..)` (no path,
receiver or projection around it),
`k` is an integer literal and `<op>` is one comparison. If that condition
does not hold at `e` and also spares at least one wrong value, the
assertion runs for the value it expects and for some value it rejects, and
which one a mutant returns is a value ripr does not evaluate. A `!=` guard
spares no wrong value: the assertion runs only when it passes, so it stays a
certain gap. ripr does not know the integer type, so a neighbour of `e`
counts only when the guard's own threshold holds it (`> k` with `e < k`
spares `e + 1`) or every type holding `e` holds it: `e - 1` for `e >= 1`
or a negative `e` above every signed minimum, `e + 1` for an `e` that is no
type's maximum. `if count() > 0 { return; }` before
`assert_eq!(count(), 0)` therefore stays a gap. An `==` guard spares every
wrong value but `k`, including a default-like `k` such as `0` (#7932 tally
review). `AssertionRefusal::ValueGuard` is then an analyzer limit, and the
classifier counts it only when `call` is the owner's call. When the
condition holds at `e`, this exception does not apply and the existing
guarded-return refusal stands: its twin `trap-steps-early-return`
(`if steps(2) < 1000 { return; }` before `assert_eq!(steps(2), 14)`, which
also returns early for every wrong value below 1000) stays
`reachable_unrevealed`. A guard that holds at `e` but at no wrong value
(`== e`) lets every wrong value reach the assertion; it still reads as a
gap and is a separate, pre-existing case (#7044). A call with a
string or char literal argument stays out, since matching the guard's
call ignores whitespace. Both cases' labeled mutants were run (runtime mutant
kill truth in the verdict corpus). An unconditional `return;` before the
assertion, which the runtime controls pin, is untouched.

The refusal scope covers only related tests that could have credited an
oracle (`oracle_crediting_relations`, shared with reveal): a name-only
relation does not count while a reach-bearing test exists.

For a string-valued predicate, a shared reason literal in an unrelated
same-file/module assertion cannot supply observation or discrimination while
an actual owner-reaching test exists (#7763). Keep that assertion visible with
`no_call_path`. A possible opaque or transitive owner call retains the existing
allowance. This does not resolve formatted/local input activation or turn a
runtime validation bypass rejection into static exposure.

The same scope applies to every value family (#9167): a same-file/module test
that calls nothing reaching the owner, with no opaque macro, supplies no
observation or discrimination while another related test reaches the owner. A
reaching test with no assertion therefore yields `reachable_unrevealed`, and the
unrelated assertion stays visible as `no_call_path`. A test that reaches the
owner through a helper keeps its credit.

The classifier marks a `reachable_unrevealed` finding whose observe summary is
`rust_assertion_context_unestablished` when it has at least one refused related
`assert_eq!` and every one is an analyzer limit.
`analysis::classify::gap_admission::withhold_unsupported_gap` is the single
owner of the decision. It runs last in the Rust limit post-pass
(`apply_probe_and_oracle_limits`, diff and repo mode), after every
producer-named limit, consumes the marker, and applies only when the finding is
still `reachable_unrevealed` with `observe` `no` and no `static_limit_kind`.

A matching finding becomes:

- `classification`: `static_unknown`;
- `static_limit_kind`: `rust_assertion_context_unresolved`;
- `stop_reasons`: `gap_evidence_unresolved`, once;
- `confidence`: rescored for `static_unknown`, which drops the bonus a gap
  earns; a lower score from an earlier limit pass is kept;
- `missing`: the limit's one-sentence description, replacing the absence
  lines that stated the withheld claim;
- `activation.missing_discriminators`: empty, so no repair placement, repair
  card, or agent packet is built from a withheld gap;
- `evidence`: a `gap_withheld: reachable_unrevealed is not claimed; ...` line
  plus the RIPR-SPEC-0114 limitation detail lines (last established edge
  naming up to three related tests, first unresolved edge, analyzer route
  `analysis/classify/gap-admission`, non-claim). The RIPR-SPEC-0197
  `assertion not credited:` line stays and says why;
- `recommended_next_step`: names the related tests and says ripr does not
  report a gap.

The stage evidence is unchanged, so `evidence_path` still shows what ripr
read. The rule never upgrades a class.

### Panic outcome of a `#[should_panic]` test

A `#[should_panic]` test with no assertion still has an oracle: it fails
when the code it runs does not panic. ripr reads such a test as having no
assertion, so a changed panic guard (`if index >= 100 { panic!(..) }`)
reached only by that test read as a certain gap even when every mutant that
stops the panic fails it. `gap_admission::withhold_should_panic_gap` withholds
that gap under `rust_panic_outcome_unresolved` when all of these hold:

- the finding is `reachable_unrevealed` with no `static_limit_kind`;
- the changed line is inside the owner function, and the owner's body
  (comments and strings masked) invokes a panicking macro (`name!(`,
  `name![` or `name!{`) (`panic!`,
  `unreachable!`, `todo!`, `unimplemented!`, `assert!`, `assert_eq!`,
  `assert_ne!` or a `debug_assert` form) on the changed line or after it;
- a related test with a `direct_owner_call` relation has no assertion and
  carries `#[should_panic]` without `#[ignore]`, either bare, with
  `expected = ""` (which matches any message), or with `expected = ".."`
  whose text is in a string literal argument of a panicking macro invoked
  on or after the changed line. A literal elsewhere in the owner does not
  count, and neither does a `format!` template. Other attribute forms
  (`#[should_panic = ".."]`, a raw string, `cfg_attr`) keep the gap.

It runs in `apply_probe_and_oracle_limits` after the log-capture limit and
before `withhold_unsupported_gap`, and writes the finding the same way, with
`next_step` naming the tests. The panic is not credited: a bare
`#[should_panic]` also passes on a panic from another callee, so its twin
`trap-row-of-unrelated-panic` (`Grid::new(0)` panics first) moves from gap
to limited, which its case accepts. An `expected` message the owner never
raises on or after the changed line, a change after the owner's last panic,
an owner with no panicking macro, a test without `#[should_panic]`, and a
`#[should_panic]` test with an assertion, and an ignored test keep the gap; only the matching
test's start line binds it to its related row.
`spec0227-check-variant-should-panic` is untouched: it reads as
`weakly_exposed` (the test's `unwrap`), and its owner has no panicking macro.
A panicking macro inside a nested `fn`, closure or `macro_rules!` in the
owner still counts; that over-reads toward `static_unknown`, never toward a
gap or credit.

### Profile-dependent `debug_assert_eq!`

`debug_assert_eq!` runs under the default `cargo test` profile and is
compiled out under `--release` (unless the profile turns debug assertions
back on). A test that pins the owner's result only with it discriminates in
the first profile and never in the second. The parser does not collect the
`debug_` forms as assertion facts, so such a test reads as having no
assertion and the finding as a certain gap.

The decision belongs to the owner-return pin (RIPR-SPEC-0197). The owner-pin
syntax pass records the assertion context of bare `debug_assert_eq!` calls
beside `assert_eq!` and `assert!`, and
`OwnerReturnPin::admits_debug_assert_eq` runs every gate `admits` applies to
`assert_eq!` (owner-side return paths, operand shape, expected value, eager
statement path, earlier returns, `#[should_panic]`, macro bindings) with the
macro spelled `debug_assert_eq`. The classifier records each related
`direct_owner_call` test without `#[ignore]` that it admits
(`ClassifiedProbeEvidence::debug_assert_eq_pins`) and hands them to the
post-pass as an evidence marker. `gap_admission::withhold_debug_assertion_gap`
consumes the marker and withholds a `weakly_exposed` or
`reachable_unrevealed` finding with no `static_limit_kind` under
`rust_debug_assertion_profile_unresolved`, naming those tests. It runs after
the `#[should_panic]` limit and before `withhold_unsupported_gap`. The
admission never credits: the `debug_` facts never enter a test's assertions.

Decision (#8051, reversible): withhold rather than credit. Crediting would
claim a discriminator that a `--release` test run never executes, and ripr
does not read the CI command or the profile settings; keeping the gap would
claim a missing test the default run does have. This matches the corpus
precedent for a production `debug_assert!` (`mined-debug-assert-only-oracle`,
RIPR-SPEC-0219), which scores `static_unknown` as `abstained`.

Two earlier cuts were rejected in review. Collecting `debug_` forms as
parser oracles let predicate, match-arm and field-projection paths credit
them as `exposed` and filled the empty assertion list the `#[should_panic]`
rule requires. A hand-written mirror of the pin's gates read the test body
and kept diverging from it (an `Ok(1)` pin of a changed early `Err` exit,
checks after an earlier `return`, `#[should_panic]` tests, `Some(..)` tails,
`async` owners), withholding gaps the `assert_eq!` twin keeps. The
`debug_assertion_limit` tests pin the twin contract: for each body, the
`debug_assert_eq!` spelling reads as a profile limit exactly where the
`assert_eq!` twin is credited, and keeps a gap otherwise, with one declared
exception: a `std::`/`core::`-qualified `debug_assert_eq!` is not recognized,
because the `std`-root proof (#7857) is spelled for `assert_eq` only, so it
keeps its gap (#8637). `debug_assert_ne!` and `debug_assert!` in any form are
not read.

### Call-fact budget of a related test line (#8600)

`extract_call_facts` keeps at most 256 distinct callees per source line. A
related test with a line at that budget may have had a call dropped, and the
dropped call may be the test's assertion helper or a second owner call, so a
negative claim over that test's calls is not established. The shared owner
`withhold_call_budget_gap` reads the existing saturation predicate
(`has_saturated_line`); no facts or cache field carries a flag. A
`reachable_unrevealed` or `weakly_exposed` finding with a saturated related
test becomes `static_unknown` under `rust_call_fact_budget` and drops its
absence claims. An `infection_unknown`, `propagation_unknown` or
`static_unknown` finding keeps its class and gains the reason. `exposed`,
`no_static_path`, a finding with a producer-named limit, and a finding whose
related tests have no saturated line are untouched. Exactly 256 is
conservatively limited: it does not prove a call was dropped. Saturation is read over every matched related test before the eight-row render cap trims the rows, and a saturated test's `no_call_path`, `no_assertion`, `missing_input` and `assertion_not_observing` row misses read as `observation_unconfirmed` (`weak_assertion` and `missing_exact_assertion` describe an assertion ripr read and stay). An already-unknown finding drops its call-derived absence lines (no oracle, no discriminator, no exact variant, no boundary input, missing-discriminator values), clears its missing discriminators and replaces its next step, as a withheld gap does. The reason never
raises a class. Scope: the finding surfaces (`check`, JSON, human, SARIF, LSP).

The `repo-exposure-json` seam reading is a separate producer
(`test_grip_evidence`), so it applies the same decision to its stage
evidence through `test_grip_evidence::call_budget`, over the same shared
predicate and the same typed `rust_call_fact_budget` kind. Saturation is read
over every ranked related test before the compact twelve-test cap. A stage
that read `no` or `weak` (activation, propagation, observation,
discrimination) becomes `unknown` and names the reason, and the seam's
missing boundary values are dropped, so a `reachable_unrevealed` or
`weakly_gripped` seam reads as an unknown class (`discrimination_unknown`,
`propagation_unknown`, ...) and its evidence record is `static_limitation`
with a `limitation` canonical item, no repair command, and a
`static_limitations[]` row whose `category` is `rust_call_fact_budget`. An
already-unknown stage keeps its class and gains the reason in its summary.
A strongly gripped seam (every stage `yes`, no missing value), an `ungripped`
seam and a seam whose reach is not `yes` or `weak` are untouched. The compact
reading behind `repo-exposure-summary-json` applies the same function, and the
classified-seam cache generation moved to `1.85` so a warm cache cannot serve
the pre-limit actionable record.

Decision (#8600): an independent empty-slice assertion beside a saturated
slice table stays `infection_unknown`, and ripr does not credit it. The
unsaturated twin with the same independent assertion is also
`infection_unknown`, because a literal-slice call is outside the eager
slice-table family that resolves the boundary; the call-fact budget adds only
the typed reason. Crediting the assertion would be a new table-family rule
that also changes unsaturated inputs, and it would move a finding toward
`exposed` (a clean claim), not toward a repair signal, so it needs its own
corpus and decision. `call_budget_slice_positive.rs` pins the three readings.

### Caller-loop selector domains (#5503)

A table-like owner (`fn max_scalar_value(n: usize) -> u32 { match n { 1 =>
.., 2 => .. } }`) is often called only from a bounded loop inside an
`Iterator::next`, and the test collects the iterator and compares the whole
collection. The classifier sees a shared name token and reads a certain
`weakly_exposed` gap. `caller_loop_selector::withhold_caller_loop_selector_gap`
withholds a `weakly_exposed` or `reachable_unrevealed` `match_arm` finding
with no `static_limit_kind` under `rust_caller_loop_selector_unresolved` when:

- the changed line is one integer-literal arm of a one-parameter owner whose
  body matches on that parameter;
- the owner's only call (no other function or test calls it) is in a `for VAR in A..B` or
  `A..=B` loop in the `next` of an `impl Iterator for TYPE`, where each bound
  is an integer literal or a uniquely defined `const`, the range holds at most
  64 selectors, the call is `owner(VAR)` once at the loop body's top level, no
  `continue`, `break`, `return` or `?` precedes it and `VAR` is not rebound;
- the arm's selector is in that range (the range and its file:line are
  recorded as `caller_loop_selector_domain` evidence);
- a non-ignored related test binds
  `let x = TYPE::ctor(..).collect()` directly and uses `x` only as a whole
  operand of `assert_eq!`, with the constructor defined once in the file and returning `Self` or the iterator type.

An arm outside the range, an unresolved, empty or oversized range, an
unreadable loop, a caller that is not `Iterator::next`, a second loop caller,
or a consumer that takes, filters, maps or mutates the items keeps the gap.
ripr does not prove the loop is entered for the test's input or that the arm's
value reaches the compared items, so nothing is credited: the corpus row
`regex-syntax-max-scalar-two-byte` moves from `gap` to `limited`, and
`regex-syntax-word-byte` stays a gap. Credit needs a producer that proves loop
entry and value flow; #5503 stays open for it.

### Framework oracles ripr does not evaluate

Six test idioms check a changed value through machinery ripr reads only at
the surface, and each used to leave a certain `weakly_exposed` claim.
`framework_oracle::withhold_framework_oracle_gap` withholds a
`weakly_exposed` or `reachable_unrevealed` finding with no
`static_limit_kind` under `rust_framework_oracle_unresolved` when a
non-ignored related test matches one of these shapes:

- **environment**: a `match_arm`, `predicate` or `error_path` probe, a
  `direct_owner_call` test with a strong assertion that calls
  `set_var("NAME", ..)` or `remove_var("NAME")`, and an owner body that calls
  `env::var("NAME")` or `env::var_os("NAME")` with the same literal name,
  with the owner called after the first such write. A different variable, a
  non-literal name, a write after the call or a relational-only test keeps the
  gap (`lib-serial-test-batch-default-relational`).
- **mock closure**: a test with a strong assertion and an `expect_<m>()` call
  whose `.returning(|..|)` closure uses one of its named (non-`_`) parameters,
  where the changed line calls `.<m>(`. A closure that ignores its arguments,
  or a changed line that does not call the mocked method (the grace predicate
  of `lib-mockall-grace-boundary-times`), keeps the gap.
- **expect-test**: a `direct_owner_call` test comparing a non-empty
  `expect![[..]]` literal with `.assert_debug_eq(..)`, where the asserted
  operand is a call to the owner or a local whose live binding where the
  assertion runs was made by `let [mut] x = ..owner(..)`. A later `let`
  (even on the same line, in a closure or callback body the assertion sits in,
  behind an attribute, or any binder of the same name whose scope holds the
  assertion: a nested `fn` parameter, a closure parameter, an `if let` / `while let` / let-chain / `for`
  pattern, a match-arm pattern or guard, a struct or tuple pattern), an
  assignment (destructuring, indexed place, one inside another `let`'s
  initializer, or a later one in the same statement such as `y = x = 5` or
  `foo(y = 2, x = 5)` included) or `&mut x` / `&raw mut x` / `addr_of_mut!(x)` borrow (any bracket), keywords read the same across
  newlines, tabs and comments, a
  binder inside the assertion's own arguments, a macro-brace `let`, a binding closed
  with its block, and a binding made only after the assertion do not reach it.
  A test that defines a `macro_rules!` or `macro` before the assertion, or
  hands the local to any macro other than the argument-reading standard and
  snapshot macros (an exact list: `assert`, `assert_eq`, `assert_ne`,
  `debug_assert*`, the insta `assert_*_snapshot` family, `format!`, `print*`,
  `matches!`, `vec!`, `dbg!`, `with_settings!`, .., spelled bare or rooted at
  `std`, `core`, `insta` or `expect_test`; any other path, a raw identifier or
  a user macro), gets no credit. Any `use` or `extern crate` item in the test
  body turns the allowlist off for that body (it can rename, glob-import or
  re-root a macro; an over-withhold). Whitespace or comments around `::` in a
  macro path are ignored, so `my ::dbg!(x)` is the qualified `my::dbg`: a macro defined
  elsewhere can bind or rewrite any name it is handed (an over-withhold). A bare allowlisted name shadowed by a module-level
  `macro_rules!` or `use` outside the test body is not detected (inherent
  residual). Residuals, kept as gaps: mutation
  through a method on a `let mut` binding (`x.push(..)`) is not tracked; the
  strict snapshot grader names the owner by its identifier, so a different
  receiver's `other.own(..)` or a shadowed `own` is read as the owner; aliasing
  and escape are not followed.
- **insta**: a `direct_owner_call` test with a snapshot macro that has no
  inline `@` literal, whose snapshotted value (after an optional literal name)
  derives from the owner as above (a live binding at the assertion), and no `add_redaction`, `with_settings` or `Settings`,
  on a changed `field:` initializer that no redaction selector (a literal
  keyed by `=>`, such as `".id"` or `"[].id"`) names. Any other changed line
  keeps the gap (the `snapshot_oracle` fixture's changed predicate), and so
  does a redacted field (`lib-insta-json-redacted-id`).
- **fixture receiver** (#9609): a `return_value` probe whose changed line
  holds a closure (`|..|`, `move |..|` or `|| ..` opening an argument; a boolean or bitwise `||` / `|` is not one) in a `&self` method, and a `direct_owner_call` test
  with a strong assertion: an `assert_eq!` with a literal (number, string,
  `true`, `false`; a bare token, not an expression such as `1 + n`) on one side and `helper().owner(..)` on the other, or
  `x.owner(..)` for a `let [mut] x = helper();` earlier in the test. `helper`
  is a function of the test's file other than the owner. An inline struct
  literal receiver, a non-literal expectation and `assert_ne!` keep the
  finding. The closure runs over data the helper builds, which ripr does not
  read; the exact pin may still miss the change, so nothing is credited.
- **recording double** (#9610): a `call_deletion` or `side_effect` probe
  whose changed line calls a method on a parameter typed `&mut dyn Trait`,
  and a `direct_owner_call` test that declares `let mut rec = Type { .. }`
  (an uppercase struct literal), passes `&mut rec` to the owner and, after the
  call, compares `rec.field` (a field, not a method call and not only its `.len()`, `.is_empty()` or `.count()`) in an `assert_eq!`.
  A concrete `&mut` parameter, a std collection, a constructor call, a method
  read or a comparison before the call keep the finding. The recorded value
  reaches the assertion through the double's own method, which ripr does not
  follow.

Any one matching related test withholds the finding: a weaker sibling test
reads as evidence ripr gathered, not as the oracle that decides, and the
matching test is the strongest oracle ripr cannot evaluate. The shapes read
the test text, as code only: a write, binding or assertion that appears only
in a comment, a nested comment or a string (raw strings included) is not
there, while a real literal name or expectation is read from the original at
the same offsets. A macro that does not qualify is skipped, so a later one that
does still counts. Two limits are accepted: the mocked-method match reads the
method name, not the receiver, and only related tests (the first eight per
finding) are judged; either can withhold a gap, which only costs an advisory.

It runs after the should-panic rule and before `withhold_unsupported_gap`,
and writes the finding the same way. Nothing is credited. Corpus effect:
`lib-serial-test-retries-default-arm`, `lib-mockall-recipient-predicate`,
`lib-expect-test-assert-debug-eq-variant-payload` and
`lib-insta-json-unredacted-sibling-field` move from `false_actionable` to
`abstained`, which their cases accept. `idiom-fixture-builder-total` (#9609)
and `idiom-hand-mock-exact-line` (#9610) move from `gap` to `limited`, which
their cases accept.

### Exact pins on shapes the owner pin does not read

An isolated test that calls the owner directly and pins its whole result with
a plain `assert_eq!` is a real oracle, but the owner-return pin (RIPR-SPEC-0197)
credits it only when the changed expression is a tail it can read. Three owner
shapes fall outside that reading, and each left a certain `weakly_exposed`
claim ("no assertion text references this probe's changed expression") on a
test that may notice the change (#9440):

- **closure tail**: a `return_value` probe whose changed line overlaps the
  owner's tail expression, where that tail holds a closure and nothing outside
  closures branches, loops, short-circuits or applies an argument only on some
  inputs. `Option` / `Result` combinators (`map`, `and_then`, `then`, ...) are
  such inputs; `map` and `filter` count as unconditional only on a receiver
  chain that reaches an iterator source (`iter`, `into_iter`, `chars`,
  `windows`, ..., or a range) through iterator adaptors alone, so
  `xs.iter().next().map(..)` stays an `Option` combinator. The changed line
  must overlap a closure of the tail. A spawned thread joined in the tail (except the
  whole-body, branch-free `std::thread::spawn(move || e).join().unwrap()` that
  RIPR-SPEC-0265 rule 4 credits), a returned closure
  the test applies, and an adaptor chain the owner or the test collects are
  this shape. The owner has no `return`, `?`, `break` or `continue` outside
  closures.
- **returned receiver**: a `call_deletion` or `side_effect` probe whose
  changed statement is a method call on `x`, in an owner whose tail is exactly
  `x` (`v.sort_by_key(..); v.truncate(2); v`).
- **let-else exit**: a `return_value` probe whose changed line is inside the
  `else` block of a `let ... else` (not its initializer), in an owner whose
  every other `return`, `?`, `break` or `continue` outside closures is inside
  an `else` block too.

`pinned_shape::withhold_pinned_shape_gap` withholds a `weakly_exposed` finding
with no `static_limit_kind` under `rust_pinned_shape_unresolved` when the
shape holds, reach and infection read `yes` and propagation reads `yes` or
`weak`, the discriminator reads `(observation_unverified)`, and some related
`direct_owner_call` test (not `#[ignore]`, no `#[should_panic]`) has a strong
exact-value `assert_eq!` with one operand that is the owner's call itself
(`owner(..)`, applied again as `owner(..)(..)`, or consumed by `.collect()` /
`.unwrap()`, never a projection such as `.len()`, an operator or a wrapping
call) and the other not naming the owner. A call on an empty or zero input
(`[]`, `vec![]`, `""`, `None`, `::new()`, a bare `0`) is not such a pin: the
closure or exit may not run, so the gap stays. One matching test suffices.
The rule runs after the framework-oracle rule and before
`withhold_unsupported_gap`, writes the finding the same way, and credits
nothing.

The slice-parameter adaptor tail of RIPR-SPEC-0265 is left to that spec's own
pin: it credits a non-empty literal input and keeps the gap for an empty one
(`trap-iter-empty-slice`, `grid-iter-property`), so this rule skips any owner
whose body that spec admits.

Decision (#9440, reversible): withhold rather than credit. Whether the pinned
input runs the changed closure or takes the `else` exit is a question ripr
does not answer here, so neither the gap nor credit is established. Crediting
the shapes whose closure the call runs on every input (a spawned and joined
closure, a returned closure the test applies, a `Result` collect) needs those
shapes added to the pin rules with runtime controls; that is follow-up work,
not part of this rule. RIPR-SPEC-0265 rule 4 has since taken the spawned and
joined closure whose body runs on every input. The cost is that a true gap in these shapes reads
`static_unknown` too: `shape-let-else-branch-unreached` and
`shape-windows-strict-vs-loose`, whose cases accept `limited`. The zero-input
rule keeps the gap for `shape-closure-returned-zero-input`.

Corpus effect on main: `shape-sort-direction-truncate` and
`shape-let-else-branch-pinned` move from
`false_actionable` to `abstained`; `shape-let-else-branch-unreached` and
`shape-windows-strict-vs-loose` move from `ideal` to `abstained`. The cases
`shape-closure-returned-adder`, `shape-collect-result-short-circuit` and
`shape-impl-trait-return-filter` (same rule, `false_actionable` to
`abstained`) and `shape-closure-returned-zero-input` (gap kept) are added by
#9463; their rows are blessed there. No other row moves.

### Methods the language calls implicitly (#9441)

A trait-impl method often runs with no call that names it: `Money(2) +
Money(3)` runs `Add::add`, the end of a block runs `Drop::drop`,
`Countdown(3).collect()` runs `Iterator::next`, and `shapes[0].area()` on a
`Vec<Box<dyn Shape>>` runs whichever `area` the box holds. Name-based reach
then finds only tests that share the owner's file or a name token, the reach
stage reads `weak` ("No test is seen calling `add`"), and the finding read
`weakly_exposed` although the test pins an exact value.
`implicit_call::withhold_implicit_call_gap` withholds a `weakly_exposed` or
`reachable_unrevealed` finding with no `static_limit_kind` and reach `weak`
under `rust_implicit_call_unresolved` when the owner is a trait-impl method and
a non-ignored indexed test with a strong assertion, read as code with
comments and strings masked, matches:

- **syntax trait**: the trait is an operator trait (`Add` .. `ShrAssign`,
  `Neg`, `Not`), `Drop`, `Iterator`, `IntoIterator`, `DoubleEndedIterator`,
  `ExactSizeIterator`, `Deref`, `DerefMut`, `Index`, `IndexMut` or
  `PartialOrd`, the test names the impl's self type as a whole identifier,
  and its code spells that syntax. An operator, index or ordering trait calls
  the method on its `Self` operand: the left operand of a binary or assigning
  operator (`+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`, `<`, `>`
  including `<=` and `>=`, and the `=` forms), the base of `[`, the operand
  after a unary `-` or `!`. A `-` that is `->` or `-=`, a doubled `&&`/`||`,
  a closure parameter list and a macro bang are not operators. `PartialOrd`
  is also reached through `partial_cmp`, `max`, `min`, `sort` and `clamp`.
  `Drop` needs `drop(` or an inner block; the iterator traits need an
  iterating `for`, any method call (a terminal adapter such as `any`, `len` or
  `next_back` consumes the value) or a path call into an iterator trait; `Deref` has no
  syntax to inspect. A workspace declaration of a trait with an operator
  trait's name is a different trait the operator cannot call, so the gap
  stays;
- **dispatch**: any other trait except those the gated std rules own
  (`Display`, `Debug`, `PartialEq`, `Eq`, `Ord`, `Hash`, `Clone`, `Default`,
  `FromStr`, `Serialize`, `Deserialize`, `Visitor`, `Arbitrary`, `Error`), the
  test names the self type, it calls `.<method>(` on a receiver, the method
  takes `self` (an associated function is called by path, never on a
  receiver), and the trait is declared once with one impl of the method for
  that type. A test-local shadow trait or second impl makes `Sample.render()`
  run the shadow, a real gap (`rust_trait_impl_shadow`,
  `rust_trait_parent_shadow`).

The pass asks the fail-closed question: is the test's use of the syntax
*provably unrelated* to the impl (#9540)? Naming the type beside an unrelated
use does not run the method, so these keep the gap:

- the `Self` operand is positively not the impl's type: a numeric or boolean
  literal (`assert_eq!(1 + 2, 3)` beside an unused `Money(0)`), or an
  identifier whose `let` has a type annotation or a constructor or
  struct-literal initializer (`Tile(1)`, `Tile { .. }`) naming another type.
  A path call such as `Factory::new()` or a helper `money(2)` stays unknown,
  and a literal is not another type when `Self` is a primitive;
- the result is thrown away: `let _ = a + b;`, or a `let` whose name nothing
  reads again, beside an unrelated assertion;
- a dispatch call whose receiver is a binding of another type
  (`let tile = Tile(3); tile.area()`), or whose result is thrown away.

An operand ripr cannot type (a helper's return value, an associated constant,
a field, a destructured tuple, a name with no `let`, a name bound more than
once by a shadowing `let`, `for`, closure parameter or match arm, an operand
that a tighter operator or a postfix `.`, `?` or `[` takes first, as in
`Money(2) * 3 + a` or `-t[0]`) might be the impl's, so the withholding stands: a wrong actionable signal is worse than a missed
advisory. A reassignment (`acc = acc + x`), a destructuring `let` (`let Money(v) = ..`),
a compound assignment or `IndexMut` write (it changes its operand even when
the unit result is discarded) and an assertion statement count as using the
result; statements nest braces, so a
struct literal or closure body does not end one.

A test with no strong assertion (`negation_is_only_exercised`,
`assert!(a < b)`), a test that never names the self type, a dispatch test that never calls the
method name, an ignored test, and a finding whose reach is not `weak` keep
their class: the rule only withholds, so the actionability flip stays
fail-closed. It does not credit the test: whether the operator or virtual call
lands on the changed method is not established. Corpus effect:
`shape-op-add-impl`, `shape-drop-impl-side-effect` and
`shape-dyn-trait-method-exact` move from `false_actionable` to `abstained`,
which their cases accept. `shape-dyn-trait-method-twin-input` is a true gap
(`partially_discriminated`: the input cannot tell the twin impls apart) that
now reads `abstained` instead of `ideal`; its case accepts `limited`, and ripr
cannot tell twin impls apart from a dyn call, so the rule trades that one
advisory for removing three false actionable claims. The finding names at most
three judged tests.

### Owners reached without a call ripr can place (#9442, #9443)

A gap pairs an assertion with a call to the owner. Three source forms give
ripr no such call although the test's oracle is real, and each used to read
`weakly_exposed`. `indirect_owner::withhold_indirect_owner_gap` withholds a
`weakly_exposed` or `reachable_unrevealed` finding with no `static_limit_kind`
under `rust_indirect_owner_call_unresolved`:

- **function value**: a non-ignored test with an assertion, in the owner's file
  or an integration test, names the owner without calling it: an argument
  (`.map(inc)`) or a binding (`let f: fn(u32) -> u32 = inc; f(2)`). A call
  `inc(..)`, a macro `inc!`, a method `.inc`, a `use` line, a field label
  `inc:`, a `fn inc` definition and a test that binds a local or parameter
  named `inc` are not value uses. A mention only in production code keeps the
  gap, since no test is shown to run it.
- **generated owner**: the changed line lies in no function of its file and the
  changed expression is an item-position macro invocation
  (`def_add!(add_five, 2 + 3);`). The definition the macro emits is not source
  ripr reads, so a related test's assertion cannot be paired with the changed
  value.
- **executor-wrapped call**: a related `direct_owner_call` test passes the
  owner call as the only argument of `block_on(..)`, written bare, as a path
  (`futures::executor::block_on`) or as a method (`rt.block_on(..)`), or of a
  helper in the test's file whose signature mentions `Future`. A second
  argument, or a helper that does not take a `Future`, keeps the gap. The
  assertion reads the helper's result, and a bounded rule that follows a
  single-argument helper to the future's output is not specified, so the
  finding is limited rather than credited.

These forms are limited, not credited: crediting needs the iterator chain or
the executor's return evaluated, and a wrong credit is worse than an honest
unknown. Both `credited` and `limited` satisfy the corpus rows
`shape-hostile-owner-as-path-value`, `shape-hostile-owner-through-fn-pointer`,
`shape-async-fn-block-on` and `shape-hostile-macro-defined-owner`.

Two reading defects behind a neighbouring row are fixed instead of limited:

- `#[cfg_attr(<predicate>, test)]` introduces the built-in `#[test]` when the
  shared evaluator reads the predicate as true in a test build (`all()`,
  `test`) and `test` is the only attribute introduced:
  `cfg_predicates::attribute_introduces_only_test` for the fact normalizer,
  the parser and the owner pin. A sibling such as `ignore` or `should_panic`
  introduced beside `test` is not read as a test at all, because the readers
  of a test's attributes (`is_ignored`, `expects_owner_panic`) see only the
  literal attribute and would count a test a default run skips or inverts as
  a discriminator. A feature, target or custom predicate stays unknown and
  does not make the function a test. `shape-hostile-cfg-attr-test` is
  credited; `trap-cfg-attr-test-beside-ignore` and
  `trap-cfg-attr-test-beside-should-panic` are not.
- A comment after the owner's tail (`x + 1 // assert_eq!(f(1), 3);`) is not
  part of the tail: the owner pin compares the tail with comments masked on
  both sides.

## Scope boundary

- Rust check findings only. The pilot seam rule is RIPR-SPEC-0230 (#5569).
- `weakly_exposed` findings are out of scope except for the framework
  oracles above and the profile-dependent `debug_assert_eq!` above (which the
  owner pin would credit as `assert_eq!`): a credited weak oracle is read
  evidence even when a stronger assertion was refused.
- Two broader rules were built and dropped. Withholding every refused-context
  gap hid the runtime-controlled gaps above. Withholding `weakly_exposed` when
  a strong oracle's text does not name the changed arm hid the sibling-arm
  gaps that the `match_arm_*` integration tests pin (#5432).
- A third, withholding reach with no retained related test, moved no corpus
  case and withheld four correct fixture gaps, so it was dropped too.

## Required Evidence

- Unit tests pin the withholding, its confidence rescore, idempotence, the
  marker contract, and that named limits and non-gap classes are untouched.
- Unit tests pin `is_analyzer_limit` for each refusal kind and attribute.
- A paired fixture: a default-feature `cfg` test is withheld
  (`gap_withheld_feature_gated_test`); an `#[ignore]` test keeps the gap
  (`gap_kept_ignored_test`).
- `pinned_shape_limit.rs` pins each shape's withholding and the controls that
  keep the gap: `Option::map`, another exit, a non-exact, non-isolated,
  ignored or `#[should_panic]` pin, the RIPR-SPEC-0265 empty slice, an effect
  on a value the owner does not return, and an `if` guard exit.
- Every runtime control in `owner_pin_execution` and `match_arm_*` keeps its
  class. The one control that changes, an unresolved `#[macro_use]` whose test
  catches the mutant at runtime, moves from a false gap to `static_unknown`.
- On the verdict corpus, false actionable falls and false silent and false
  exposed do not rise.

## Measured effect

Verdict corpus (203 cases, corpus 2026-10-04.8), measured against main at
80975c807 (#5359 merged):

| Rate | Before | After |
| --- | --- | --- |
| False actionable (of discriminated) | 58/106 | 57/106 |
| False actionable, upstream cases | 7/20 | 6/20 |
| False exposed (of not fully discriminated) | 6/97 | 6/97 |
| False silent (of not fully discriminated) | 0/97 | 0/97 |
| Abstained | 58/203 | 59/203 |

The first measurement, on the 34-case corpus 2026-10-04.3 on top of #5359,
moved false actionable on upstream cases from 9/20 to 8/20 through the same
single case.

serde `format_u8` hundreds moves from `reachable_unrevealed` to
`static_unknown`. No existing fixture golden changes class. The remaining false
gaps are `weakly_exposed` (sibling-arm oracles #5432; refused assertions behind
a credited weak oracle) and semver `op()` (reach with no retained test, #5344).

## Non-Goals

- Crediting any assertion, or changing equality admission.
- Evaluating Cargo features or `cfg` predicates.
- A population claim. The rates describe the corpus only.

## Acceptance Examples

- serde `format_u8`, `n >= 100` rewritten as `n > 99`: `test_format_u8`
  carries `#[cfg(any(feature = "std", not(no_core_net)))]`, ripr refuses its
  `assert_eq!`, and the finding is `static_unknown` with
  `rust_assertion_context_unresolved`, naming `test_format_u8`.
- `owner_return_pin_if_false`: the assertion sits in `if false { .. }`; the
  finding stays `reachable_unrevealed`.

## Test Mapping

Tests in `crates/ripr/src/analysis/classify/gap_admission.rs`:

- `analyzer_limit_refusals_withhold_the_gap_and_name_the_test`
- `withholding_drops_the_gap_confidence_bonus_and_never_raises_a_score`
- `an_unmarked_refusal_keeps_the_gap`
- `a_marker_without_the_refused_observe_keeps_the_gap_and_is_consumed`
- `an_existing_named_limit_and_non_gap_classes_are_untouched`
- `withholding_twice_adds_one_stop_reason`
- `a_panic_macro_is_read_by_its_own_name`
- `a_should_panic_test_expects_the_owner_panic_only_when_the_message_matches`

Tests in `crates/ripr/src/analysis/classify/framework_oracle.rs` and
`crates/ripr/tests/framework_oracle_limit.rs`:

- `a_mock_closure_counts_only_when_it_reads_a_named_argument`
- `an_arm_chosen_by_the_environment_is_withheld` and its keeps-the-gap control
- `an_expect_debug_literal_is_withheld` and its keeps-the-gap control
- `an_unredacted_insta_field_is_withheld` and its keeps-the-gap control
- `a_helper_built_receiver_pinned_to_a_literal_is_withheld` and
  `a_fixture_receiver_shape_that_differs_keeps_the_finding`
- `a_hand_written_recorder_compared_by_field_is_withheld`,
  `a_recorder_shape_that_differs_keeps_the_finding` and
  `a_vec_pushed_into_by_the_changed_call_keeps_the_gap`

Tests in `crates/ripr/src/analysis/classify/implicit_call.rs` and
`crates/ripr/tests/implicit_call_limit.rs`:

- `method_calls_need_a_dot_receiver_and_a_whole_name`
- `operator_syntax_is_read_per_trait`
- `a_drop_impl_reached_at_scope_exit_is_withheld`,
  `an_iterator_impl_reached_through_collect_is_withheld`,
  `an_operator_impl_reached_through_the_operator_is_withheld` and
  `a_dyn_trait_method_called_on_an_untyped_receiver_is_withheld`
- keeps-the-gap controls (each asserts a gap remains and the limit is absent):
  `an_operator_impl_controls_keep_the_gap` (no test names the type, smoke
  test, wrong operator, ignored test), `a_dispatch_shape_that_differs_keeps_its_class`
  (never calls the method, never names the type) and
  `a_test_local_shadow_trait_keeps_the_gap` and
  `a_workspace_trait_named_like_an_operator_trait_is_not_the_operator`.
  `an_operator_impl_controls_keep_the_gap` also holds the provably unrelated
  shapes (an operator on integers beside an unused `Money`, operands bound to
  another type, a discarded result) and
  `a_dispatch_shape_that_differs_keeps_its_class` the receiver of another
  type; `an_operator_bound_through_lets_is_withheld` pins the forms that stay
  withheld (a bound result, bound operands, a helper's value, a reassigned
  accumulator) (#9540). The `rust_trait_impl_shadow` and
  `rust_trait_parent_shadow` goldens pin the same shadow rule.

Tests in `crates/ripr/src/analysis/classify/owner_pin/tests.rs`:

- `analyzer_limit_refusals_are_limits_of_ripr_reading`
- `refusals_that_can_keep_an_assertion_from_running_are_not_limits`
- `an_outcome_settling_attribute_is_the_refusal_wherever_it_sits`
- `inert_attributes_keep_the_pin_and_rebound_or_gating_ones_refuse_it`
- `a_return_guard_that_spares_the_expected_value_is_an_analyzer_limit`
- `a_direct_call_name_is_one_bare_call_ending_the_subject`

Integration: `crates/ripr/tests/owner_pin_execution.rs`
(`owner_pin_matched_static_and_runtime_controls`,
`macro_use_module_admission_matches_runtime`) and
`crates/ripr/tests/should_panic_limit.rs`
(`a_should_panic_test_on_a_changed_panic_guard_reads_as_static_unknown`,
`a_panic_outcome_the_rule_does_not_cover_keeps_the_gap`) and
`crates/ripr/tests/call_budget_limit.rs`
(`a_saturated_related_test_line_withholds_the_gap_under_the_budget_reason`,
`saturation_that_does_not_limit_the_proof_keeps_the_finding`,
`an_independently_established_positive_keeps_its_class`,
`a_saturated_test_past_the_related_row_cap_still_withholds_the_gap`,
`an_already_unknown_finding_names_the_budget_and_drops_call_derived_absence`), plus the unit test
`gap_admission.rs::tests::a_related_test_line_at_the_call_budget_withholds_a_gap_and_names_an_unknown`.

`crates/ripr/tests/indirect_owner_limit.rs` (function value, pointer binding,
local and known executors, item macro, `cfg_attr` and trailing-comment
controls) and `cfg_predicates::tests::cfg_attr_introduces_test_only_under_a_predicate_known_true`.

Fixtures: `gap_withheld_feature_gated_test`, `gap_kept_ignored_test`.

Wire strings are pinned by
`crates/ripr/src/domain/language.rs::tests::static_limit_kind_wire_strings_are_stable`
and `crates/ripr/src/domain/probe.rs::tests::stop_reason_glosses_are_clean_prose`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/owner_pin.rs`:
  `AssertionRefusal::is_analyzer_limit`.
- `crates/ripr/src/analysis/classifier/evidence.rs` computes the refusal
  scope; `crates/ripr/src/analysis/classifier/finding.rs` marks the finding.
- `crates/ripr/src/analysis/classify/gap_admission.rs` owns the decision.
- `crates/ripr/src/analysis/classify/implicit_call.rs` owns the implicit-call
  rule (#9441).
- `crates/ripr/src/analysis/classify/indirect_owner.rs` owns the
  indirect-owner decision.
- `crates/ripr/src/analysis/language/rust/mod.rs` calls it last in
  `apply_probe_and_oracle_limits`.
- `crates/ripr/src/domain/language.rs` and `crates/ripr/src/domain/probe.rs`
  own the new wire values.

## Metrics

- `verdict_corpus_false_actionable_rate`
- `verdict_corpus_false_verdict_rate`
