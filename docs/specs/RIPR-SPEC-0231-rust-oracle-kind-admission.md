# RIPR-SPEC-0231: Rust oracle kind and strength admission

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

- #5513 (observer substrings grant medium `mock_expectation` credit)
- #6991 (exact-equality `.any()` membership graded weak; rule 7)

Linked PRs:

- #5410 (stop crediting unguarded wildcard assertions)
- #5416 (unknown, not a gap: rule 3 reads the nearest oracle's strength)

Support-tier impact:

- No tier change. The `oracle_kind` and `oracle_strength` that ripr reports
  for a Rust related test stop overstating what the assertion pins. Rules 1
  to 6 move no finding to a stronger class. Rule 7 raises one narrowly
  admitted shape from weak to strong; the grade is verdict-neutral on the
  Rust verdict corpus (283 cases, zero rows moved), because reveal still
  requires observation confirmation before a strong oracle reads `exposed`.
  Under the #5416 unknown-not-a-gap rule 3, a finding whose only strong
  oracle this spec weakens may move from `static_unknown` back to its named
  gap. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. Kind and strength values are unchanged; only which
  assertions earn them.

## Problem

Every Rust related test carries an `oracle_kind` and `oracle_strength`, shown
in JSON, repair cards and the "Strong oracle found" discriminate summary.
They are decided by one precedence chain of substring checks
(`analysis/extract/oracles/classify.rs`, `classify_assertion`, and
`patterns.rs`). No spec defines that chain. Several links overstate the
oracle. Measured on origin/main e177461 with `ripr check --diff` on a
one-function crate. The oracle code is unchanged through cd5f0d473 except
for #5410 (de56e085a, the wildcard pre-check below), which affects no row:

| Test assertion | Reported kind / strength | What it pins |
| --- | --- | --- |
| `assert_ne!(score(2), 0)` | `exact_value` / strong | only that the result is not 0 |
| `assert_ne!(check(20), Ok(20))` | `exact_value` / strong | only that the result is not `Ok(20)` |
| `assert!(matches!(check(20), Err(e) if !e.is_empty()))` | `exact_error_variant` / strong | any non-empty error |
| `let e = String::from("big"); assert_eq!(check(20), Err(e))` | `exact_error_variant` / strong | the exact error value (correct) |
| `assert!(score(2).to_string().len() == 1 \|\| is_present())` | `mock_expectation` / medium | nothing about an effect: "sent" in `is_present` |

None of these reads `exposed` on main, because reveal still needs a token or
owner-pin confirmation. The strength still matters:

- the predicate finding prints "Strong oracle found: exact value or pattern
  assertion" for the `assert_ne!` test;
- the #5416 unknown-not-a-gap rule 3 withholds a `weakly_exposed` gap as
  `static_unknown` when the nearest tests hold a **strong** oracle. A test
  whose only assertion is `assert_ne!(score(2), 0)` therefore turns a real
  gap into "unknown" once #5416 lands;
- `mock_expectation` medium credit lets an effect family confirm on an
  unrelated identifier.

The other overstatements in the chain:

- `whole_object_equality` / strong for any `assert_eq!` or `assert_ne!` line
  containing `{` (governed by RIPR-SPEC-0225);
- `matches!` and `assert_matches!` as `exact_value` / strong whatever the
  pattern, except `Err(..)` patterns, which steps 1 and 2 decide (wildcards
  are #5410);
- a custom helper is `exact_value` / strong when its name contains `_eq`,
  `_equal` or `_matches` and it has two arguments (one for a `.assert_*`
  method call), or when its name ends in `eq`, `equal` or `matches`, so
  `assert_not_equal(a, b)` qualifies;
- `smoke_only` and `broad_error` are decided by substrings (`is_ok`,
  `is_err`), so an identifier such as `is_okay` or `this_errs` qualifies.

## Behavior

### One authority

`classify_assertion` stays the single classifier for Rust oracle kind and
strength. The only later adjustment is the RIPR-SPEC-0106 upgrade in
`scan.rs`, which turns an `exact_value` or `whole_object_equality` assertion
on an `unwrap_err`-bound variable into `exact_error_variant`. Kind is decided
from the assertion's operand text after message arguments are removed
(`assertion_oracle_text`), never from a format string or a diagnostic
argument, for the macros `assertion_oracle_text` recognizes; a custom helper
is read from its whole line. The RIPR-SPEC-0106 upgrade never applies to an
assertion that an admission rule below weakened.

### Assertion macro paths

The parser's assertion recognizer (`is_assertion_macro`) reads a macro called
through one path segment into a drop-in assertion crate as its leaf:
`std::`, `core::` (each optionally `::`-rooted), `pretty_assertions::` and
`similar_asserts::` change only the failure output, and `assert_matches::`
exports the std-shaped `assert_matches!`. So `pretty_assertions::assert_eq!`
classifies exactly as `assert_eq!`. Any other crate path
(`other::assert_eq!`), a deeper path (`pretty_assertions::nested::assert_eq!`)
or a leaf outside the std set stays unrecognized. A recognized qualified
`assert_eq!` faces the same RIPR-SPEC-0197 execution admission as the bare
spelling, so the qualified path never gains credit the bare one is refused.
The parser refuses a qualified path whose crate root may be rebound
(example 30): a same-file module, import of another item, `extern crate`
rename or item-position macro, or a function read without its file.
The token-level `is_known_rust_assertion_macro` in
`analysis/language/rust/oracles.rs` reads any path's last segment; aligning
the two crate rules is outside this spec.

### Precedence

The chain runs in this order; the first match wins. Before it, the
wildcard pre-check (#5410) assigns `relational_check` / weak to an
`assert!`, `debug_assert!` or `ensure!` whose whole condition is
`matches!(x, _)`, and to an `assert_matches!` or `debug_assert_matches!`
whose pattern is a whole unguarded `_`. Admission rule 3 widens this
pre-check to every whole irrefutable pattern, guarded or not, except for a
guard that pins a value (decision 7).

0. an `ensure!` condition runs its own sub-chain
   (`classify_fallible_assertion`): `exact_error_variant` / strong, then
   `broad_error` / weak, then an exact comparison that is not duplicative as
   `exact_value` / strong (rule 1 says which `==` is one), then `smoke_only` / smoke, otherwise
   `relational_check` / weak
1. `exact_error_variant` / strong
2. `broad_error` / weak
3. duplicative equality (the same expression on both sides) as
   `relational_check` / weak
4. `whole_object_equality` / strong
5. `exact_value` / strong
6. `snapshot` / medium (known `insta` and `expect_test` forms only)
7. `smoke_only` / smoke
8. scalar path-versus-integer relation as `relational_check` / weak
9. `mock_expectation` / medium
10. exact custom helper as `exact_value` / strong
11. other custom helper as `unknown`
12. exact-equality `.any()` membership as `exact_value` / strong (rule 7)
13. `<` or `>` anywhere in the operand text, `is_empty`, `contains` or a
    bare `assert!` as `relational_check` / weak
14. otherwise `unknown`

This order is the existing behavior and is normative, with step 12 added
by rule 7 (#6991); the former steps 12 and 13 are now steps 13 and 14.
The admission rules below change what a step admits. A rule that takes an
assertion out of a step assigns the kind it states at that step's
position, so the assertion does not fall through to a later step by
accident. Rules 1 to 6 never raise a strength. Rule 7 is the deliberate
exception: it raises the one shape it admits from weak to strong, with
the corpus evidence below.

### Admission rules

Two pattern terms are used below.

- An **irrefutable pattern** accepts every value of its type: `_`; a
  catch-all binding (a lowercase-initial identifier other than `true` and
  `false`, including `_name`, optionally with `ref`, `mut` or `ref mut`);
  `name @ p`, `&p` or `box p` where `p` is irrefutable; a tuple pattern,
  or a struct or tuple-struct pattern whose path is shown to name a struct
  type, whose every sub-pattern is irrefutable or `..` (`(_, _)`,
  `S { .. }`); the sole variant of an enum shown to have one variant,
  with irrefutable sub-patterns; the slice patterns `[..]` and `[name @ ..]` (other slice
  patterns check a length); and an or-pattern whose alternatives
  together cover every constructor of the type (`Some(_) | None`,
  `Ok(_) | Err(_)`). `name @ E::X` and `name @ Some(_)` are read through
  their sub-pattern.
  When a path cannot be shown to name a struct (for example `Cfg { .. }`
  under a glob `use` of an enum), it is read as a variant and is not
  irrefutable.
- A **side-only pattern** checks only which side of an `Option` or `Result`
  the value is on: `Some(p)`, `Ok(p)` or `Err(p)` where every payload
  sub-pattern is irrefutable, `..` or itself side-only (`Some(Ok(_))`), and
  `None` (RIPR-SPEC-0227). `&p`, `box p` and `name @ p` are side-only when
  `p` is.

1. **Inequality is never exact.** At steps 0, 1, 5 and 10, an inequality
   assigns `relational_check` / weak, whatever its operands: `assert_ne!`
   (and any `*assert_ne!` spelling such as `soft_assert_ne!`), `!=` in an
   `ensure!` condition, a negated `!matches!(..)` (which today reads
   `exact_error_variant` / strong at step 1 when its pattern is
   `Err(E::X)`; also `matches!(..).not()` and the `prop_assert!` form), an
   assertion macro whose name has an inequality segment
   (`assert_not_matches!`), a `matches!` comparison that is not positive
   (below), and a custom helper whose
   name has a `ne`, `neq`, `not`, `noteq`, `notequal`, `nonequal`,
   `unequal`, `inequal`, `differ`, `differs`, `different` or `distinct`
   segment that step 10 admits today; such a name never earns step 10's
   clear-exact credit. One structural polarity rule reads a `matches!`
   compared with `==` or `!=`, as the operands of an equality macro
   (`assert_eq!`, `assert_ne!` and their `debug_` and `prop_` spellings)
   or at the top level of an `assert!`, `debug_assert!`, `prop_assert!` or
   `ensure!` condition, in either operand order: against a boolean literal
   or a negated literal, `== true`, `!= false` and `== !false` are
   positive and keep the pattern's reading; `== false`, `!= true`,
   `== !true` and `!= !false` are negative. A negative comparison, a
   comparison against anything that is not a boolean literal (a variable,
   a call, another `matches!`), a negated pattern operand
   (`!matches!(..) == false`, a double negation not read through) and a
   compared pattern nested in a larger condition all fail closed at
   `relational_check` / weak. `assert_ne!` stays an inequality whatever
   the literal. A `!=` in a boolean assertion condition is rule 1's
   inequality only when `!=` is the condition's only comparison
   (`ensure!(a != b)`); a condition that also has a pinning `==` (below)
   keeps the reading that `==` earns
   (`ensure!(score == 4 && other != 1)` is `exact_value` / strong). That
   `==` must be a top-level `&&` conjunct whose own top-level operator is
   `==`: an `==` inside a closure, call argument or negation pins nothing
   (`ensure!(f(|x| x == 1))`, `ensure!(!(a == b))` and
   `ensure!(xs.iter().position(|x| *x == 1) != None)` read
   `relational_check` / weak). A disjunction that decides the condition
   passes when any one arm holds, so no arm lends it an exact kind. In an
   `assert!`, `debug_assert!`, `prop_assert!` or `ensure!` condition, a
   top-level `||`, or one at the top level of a parenthesized group that
   is the whole condition (the group read again by the same rule), caps the
   reading at `relational_check` / weak, including a literal `|| false`
   arm, which is not read through, and an arm such as `r.is_err()` that
   would otherwise earn `broad_error` or an observer such as
   `events_sent.contains(&id)` that would otherwise earn step 9's
   `mock_expectation` (`assert!(ready || events_sent.contains(&id))` passes
   without observing the event), or a snapshot in a block arm that would
   otherwise earn step 6's `snapshot`. A parenthesized `&&` conjunct that
   such a `||` decides pins nothing, so, as beside a negated pattern, the
   other conjuncts decide with their own reading, or `relational_check` /
   weak when no other conjunct remains (`ensure!(c == d && (a || b))` reads
   as `ensure!(c == d)`). The group drops out before the negated-pattern
   and compared-pattern caps in this rule apply, so a negated or compared pattern inside
   it caps nothing (`ensure!(c == d && (!matches!(r, Err(E::X)) || y))`
   reads `exact_value` / strong). A `||` inside a call argument, closure or pattern
   does not decide the condition
   (`assert!(matches!(pick(|x| x > 1 || x < 0), Ok(3)))` keeps
   `exact_value`). A boolean-valued operand compared with a boolean
   literal is read as what it asserts (#7598): in an equality macro with
   one boolean-literal operand (`assert_eq!(X, true)`, `assert_ne!(false,
   X)` and the `debug_` and `prop_` spellings), or a boolean assertion
   whose whole condition is such a comparison (`ensure!((a == b) ==
   false)`), when `X` has a top-level `==`, `!=`, `<`, `>`, `<=` or `>=` (a
   turbofish, `->`, `=>` and a shift are not comparisons), `||` or `&&`
   (through any `!`, `.not()` or comparison with a boolean literal, read
   first: `x != true` is `!x` and pins a bool value), an `X` asserted false,
   an `X` with a deciding `||`, and an `X` whose comparison is `!=` or
   relational all cap the reading at `relational_check` / weak (a `<`
   that opens the operand or follows `::` opens a path, not a comparison:
   `<T as Tr>::f(x) == y`). An `&&`
   chain asserted true with a `matches!` conjunct and no pinning `==`
   conjunct reads like the same chain in `assert!` (rule 3); with no
   pinning `==` conjunct, a negated test (`!matches!(..)`, `!(a == b)`,
   `(a == b) == false`) or a `!=` or relational comparison as a conjunct,
   through any literal comparison (`(r != Err(E::Bad)) == true`), caps
   before the chain is read (`assert_eq!(score(2) != 0 && flag, true)`,
   `assert_eq!(flag && !matches!(r, Err(E::Bad)), true)`); a bool value
   compared with `false` pins it like `!x`. With a pinning `==` conjunct,
   the relation-only conjuncts lend the chain no kind: it reads as the same
   equality assertion over its other conjuncts, never higher than the whole
   (`assert_eq!(a == b && r != Err(E::Bad), true)` reads as
   `assert_eq!(a == b, true)`; #7947). A leading `!`
   negates only its own operand (`!flag && matches!(..)` is a chain). A comparison
   asserted true (`assert_eq!(r == Err(E::Bad), true)`), a bare value or
   call (`assert_eq!(!is_valid(x), true)` pins a bool result) and a whole
   `matches!` (the polarity rule above) keep their readings. Spellings that
   keep a comparison or a literal read the same way (#7931): a comparison
   method on a receiver or a two-argument path call to it
   (`r.eq(&Err(E::Bad))`, `score(2).ne(&0)`, `PartialEq::eq(&r, &x)`,
   `Result::eq(&r, &x)`, `<_ as PartialEq>::eq(&r, &x)`; #7993;
   `.lt`, `.le`, `.gt`, `.ge`) reads like its operator, so only `eq`
   asserted true may pin, and an `eq` or `ne` with a literal side is that
   literal comparison (`(r == Err(E::Bad)).eq(&false)` is `== false`); a
   literal behind `&`, a one-expression block, `as bool` or
   `bool::from(..)` is that literal, and a `!` over such a spelling negates
   it (`!&true` is `false`; #7993); and the operand through a
   one-expression block, `&`, `as bool` or `bool::from(..)` is the operand
   (`assert_eq!({ r == Err(E::Bad) }, false)`,
   `assert_eq!(bool::from(r == Err(E::Bad)), false)`). Grouping
   parentheses around these spellings change nothing (`(false as bool)`). A
   method call binds tighter than any operator, so its receiver is one
   operand: `flag && r.eq(&x).eq(&false)` is a chain whose last conjunct is
   the call. Peeling a spelling can raise a doubled negation to the
   comparison's own reading (`assert_eq!((r == Err(E::Bad)) == false as bool,
   false)` and `assert_eq!(&(r == Err(E::Bad)) == &false, false)` assert
   `r == Err(E::Bad)`). A test (a comparison, `||`, `&&`, comparison call, whole
   `matches!`, or `^`, `|` or `&` over a comparison or pattern, through the
   spellings above; a bitwise operator over plain values is no test)
   compared by an equality macro or a whole `==` or `!=` condition with a
   side that is no boolean literal caps, because that side may be `false`
   (`assert_eq!(r == Err(E::Bad), expected)`,
   `assert_eq!(matches!(r, Err(E::Bad)).eq(&true), expected)`; #8006); a
   bool value compared with one still pins (`assert_eq!(is_valid(x),
   expected)`). A cast of a test to an integer type, or `u8::from` and the
   other integer `From` calls, carries the test, and an integer is no boolean
   literal, so `0`, `1` or a variable on the other side caps
   (`assert_eq!((r == Err(E::Bad)) as u8, 0)`,
   `assert_eq!(u8::from(r == Err(E::Bad)), n)`; #8093), and so do the
   `From`/`Into` path spellings of that conversion (`From::from(..)`,
   `<u8 as From<bool>>::from(..)`, `Into::<u8>::into(..)`,
   `std::primitive::u8::from(..)`; #8101); a cast of a value
   still pins (`assert_eq!(x as u8, 3)`). A value wrapper holds the test
   it wraps, and a boolean literal it wraps is that literal: a constructor
   (`Some(..)`, `Ok::<bool, ()>(..)`, `E::V(..)`), a `new` or `from` path
   call (`Box::new(..)`), a one-element or repeat array (`[..]`,
   `[..; n]`), `vec![..]`, and `.unwrap()`, `.expect(..)`, `.clone()` or
   `.to_owned()` on one operand, so `assert_eq!(Some(r == Err(E::Bad)),
   Some(expected))` caps and `assert_eq!(Some(r == Err(E::Bad)),
   Some(true))` reads as the comparison asserted true (#8101); a wrapped
   plain value still pins (`assert_eq!(Some(x), Some(3))`), a lowercase
   call stays an opaque call (`wrap(r == Err(E::Bad))`), and a wrapper
   compared with a bare literal stays an unread call over its pattern
   (`assert_eq!(Wrapper(matches!(..)), true)`). Such a comparison asserted by a
   boolean literal caps the same way (`assert_eq!((r == Err(E::Bad)) as u8
   == 0, true)`, `assert_eq!((r == Err(E::Bad)) == flag, true)`), and a
   path call to `not` negates like `!` (`assert_eq!(Not::not(r ==
   Err(E::Bad)), true)`, `bool::not(..)`; any path, because a function named
   `not` most likely inverts and the reading only lowers). A negation of a whole boolean
   assertion condition that holds a `matches!`, through grouping, a
   one-expression block, `.not()`, a `Not::not(..)` path call or a negating
   literal call on any wrapper, caps (`!(flag || matches!(..))`,
   `!{ matches!(..) }`, `Some(matches!(..)).unwrap().not()`,
   `(matches!(..),).0.eq(&false)`; #8006; `Not::not(flag || matches!(..))`;
   #8101), and a one-expression block
   passes a pattern through to `^` or `|` (`flag ^ { matches!(..) }`).
   `^` and a
   single `|` over an operand holding a `matches!` or a comparison, in
   operator or call spelling, with no top-level comparison of their own,
   also cap (`assert_eq!((r == Err(E::Bad)) ^ true, true)`,
   `assert_eq!(flag ^ r.ne(&Err(E::Bad)), true)`); over plain bool values (`is_a(x) ^ true`)
   they still pin a bool result. A top-level single `&` over booleans is a
   conjunction and reads like `&&`; beside a top-level comparison it is a
   bitwise operand (`a & mask == b`). A string or char literal before `<`
   is an operand (`"x+" < s` compares). In a boolean assertion condition, or an
   equality-macro operand compared with a boolean literal, a `matches!`
   that is an operand of `^`, `|`, `<`, `>`, `<=` or `>=` (on either
   side; a closure's parameter bars are not `|`), cast with `as`, the
   receiver of a method call through only grouping parentheses or
   one-expression blocks
   (`matches!(..).eq(&false)`, `({ matches!(..) }).then(|| false).unwrap_or(true)`;
   a method after a call or closure that takes the `matches!` is the call's,
   and an `eq`/`ne` chain against literals that keeps the pattern's polarity,
   `matches!(..).eq(&true)`, asserts the pattern when it is the whole
   condition; asserted false, behind a call or inside a chain it caps), an
   argument at any position of a call that is the whole condition
   (`std::ops::Not::not(matches!(..))`, `check::<bool>(1, matches!(..))`,
   directly or through nested calls; a call's own parenthesis, turbofish
   included, is no grouping, so `foo::<T>(matches!(..)).is_ok()` reads as
   `foo(matches!(..)).is_ok()` does, and a comparison inside a call's
   arguments is not the operand's own test, so `flags | compute(|v| v == 1)`
   still pins; #8101), or inside the condition or
   scrutinee of an `if`, `while` or `match` anywhere in the condition
   (`flag == if ready && matches!(..) { false } else { true }`; a guard
   inside an earlier `matches!(..)` and a raw `r#match` are not such
   keywords) may be inverted or
   discarded, so it caps the reading like a negated pattern. In a boolean
   assertion's top-level `&&` chain with no top-level `||` its conjunct
   drops out and the other conjuncts decide
   (`ensure!(check(matches!(r, Ok(3))) && score == 4)` reads as
   `ensure!(score == 4)`); in the equality-macro form the whole compared
   operand caps (`assert_eq!(score == 4 && check(matches!(r, Ok(3))),
   true)` reads `relational_check` / weak). A call whose result is read again
   (`foo(matches!(..)).is_ok()`, `Some(true) == Some(matches!(..))`), a
   `matches!` scrutinee (`matches!(build(matches!(..)), Ok(_))`) and a
   closure body (`xs.iter().any(|x| f(matches!(x, Some(3))))`) keep their
   readings. A whole condition or equality-macro operand compared with a
   literal whose operand is an `&&` chain with a `matches!` conjunct keeps
   the compared-pattern cap above
   (`assert_eq!((flag && matches!(r, Ok(3))) == true, true)`). At
   step 4, every `assert_ne!` assigns weak strength: a struct-literal
   operand keeps the `whole_object_equality` kind, as RIPR-SPEC-0225 says,
   with no field credit; any other `{` (a closure or block operand) assigns
   `relational_check`. A negated pattern
   assertion takes rule 1 before rules 2 and 3. A negated pattern is an
   inequality only when it is the whole condition; in a top-level `&&` chain
   with no top-level `||` the other conjuncts decide the reading, and
   anywhere else (`||` at any level, a nested group, except inside a
   deciding `||` group that drops out as above) the reading is capped
   at weak. An inequality-named helper is recognized before any exact-kind
   step, so a `matches!(.., Err(E::X))` argument cannot lend it a kind, and
   the cap keeps a reading already below weak. A call-count or
   observation helper keeps its own reading below the cap:
   `mock.assert_not_called()` stays `mock_expectation` / medium, and
   `assert_not_sent(..)` reads unknown, as `assert_sent(..)` does under
   rule 5, rather than a relation. Only the helper name
   and its receiver decide that exemption, never its arguments, and a
   name with a value-comparison segment (`equal`, `eq`, `ne`) is never
   exempt: `assert_not_equal(expect_value(), 3)`,
   `mock.assert_ne(x, 3)` and `assert_ne_state(machine, State::Idle)`
   read weak. No probe-family strength override in reveal lifts an
   inequality above its classified strength.
2. **Patterns with bindings are not variant pins.** At steps 0 and 1, a
   `matches!` or `assert_matches!` whose `Err(..)` inner pattern is
   irrefutable, `..` or pins no value (rule 3's test) assigns
   `broad_error` / weak, guard or not, unless the guard pins a value
   (decision 7).
   `Err(E::X)`, `Err(E::X(..))` and `Err(E::X { .. })` stay
   `exact_error_variant`. `assert_eq!` against `Err(value)` with any
   expression stays `exact_error_variant`, because equality pins the value.
3. **Pattern assertions follow their pattern.** Before step 0, with the
   wildcard pre-check, a `matches!` or `assert_matches!` whose whole
   pattern is irrefutable assigns `relational_check` / weak, guard or not
   (except a guard that pins a value, decision 7),
   so `Ok(_) | Err(_)` never reaches step 2 as a result-side oracle that
   RIPR-SPEC-0227 rules 3 and 3b could credit. Rule 2 takes precedence
   over the rest of this rule for an `Err(..)` pattern it covers, so
   `Err(_)` and a guarded `Err(e)` stay `broad_error` / weak. At steps 0,
   1 and 5, any other side-only pattern assigns `smoke_only` / smoke when
   unguarded, because it only checks the side, and `relational_check` /
   weak with a guard; a
   range pattern, or a constructor whose payload is a range (`1..=5`,
   `Some(1..=5)`), assigns `relational_check` / weak, and an `Err` range
   payload (`Err(1..=5)`) assigns `broad_error` / weak. At steps 0, 1 and
   5, an or-pattern that is not irrefutable but has alternatives on both
   sides (`Ok` and `Err`, or `Some` and `None`) assigns `relational_check` /
   weak, because it does not observe the side (`Ok(_) | Err(E::X)`); any
   other or-pattern reads as its weakest alternative. Any other pattern
   stays `exact_value` only when it pins a value: it contains a literal
   outside a range, a constant, or a variant of an enum shown to have more
   than one variant (the `Option` and `Result` constructors are read as
   side-only above). A pattern that pins no value assigns
   `relational_check` / weak: a slice that only fixes a length (`[_, ..]`,
   `[_]`), or the sole variant of a single-variant enum (`Only::Value`,
   `Only::Value(_)`). A variant whose enum cannot be resolved counts as
   pinning, which keeps today's reading. In a top-level `&&` chain with
   at least one whole `matches!` conjunct and no pinning `==` conjunct
   (rule 1), the chain reads no stronger than its strongest conjunct read
   alone, and keeps its kind only when such a conjunct earns that kind at
   that strength; otherwise it takes the first such conjunct's reading.
   So a `matches!` keeps its pattern's reading inside the chain (#7695).
   A pinning `==` keeps the chain's reading, because the test does fix
   that value; a self-comparison (`value == value`) pins nothing and
   neither exempts the chain nor earns rule 1's exact credit.
   `prop_assert!` reads a whole `matches!` condition as `assert!` does.
   Outside those whole forms, a `matches!` invocation earns no exact credit
   from its text alone. One nested in a call or closure
   (`xs.iter().any(|x| matches!(x, Some(_)))`) or compared with a bool
   (`matches!(r, Ok(_)) == true`, with or without redundant parentheses,
   which is no pinning `==` either) is credited only when its own pattern
   pins a value or a pinning `==` conjunct keeps the chain (#7715). A
   `matches!` named only in a comment or string is no invocation and earns
   nothing, and a comment around the pattern's constructor
   (`matches!(x, /* , */ Some(_))`) does not hide it (#7772). A user macro, by path (`custom::matches!`,
   `helpers::std::matches!`) or by name (`expect_matches!`), keeps today's
   reading; only `matches!`, `std::matches!` and `core::matches!` (with an
   optional leading `::`) are the std pattern test.
4. **Method checks match whole method names.** At steps 0 and 7, `is_ok`,
   `is_some` and `is_none` count only as a method-call segment (`.is_ok(`,
   `Option::is_some(`), not as a substring of another identifier such as
   `is_okay`. The combinator forms `.is_some_and(`, `.is_ok_and(`,
   `.is_err_and(` and `.is_none_or(` count as the same segment. `.unwrap(`
   and `.expect(` already match this way. At step 2, `is_err` matches the
   same way, so `this_errs` is not a broad error. A condition that tests both sides
   (`is_ok()` with `is_err()`, or `is_some()` with `is_none()`, joined by
   `||`) assigns `relational_check` / weak at steps 0, 2 and 7, because it
   accepts every value.
5. **Effect observer words match whole identifier segments.** At step 9,
   `event`, `emitted`, `published`, `sent`, `saved`, `persist`, `state`,
   `stored`, `metric`, `counter` and `recorded` count only as a whole
   `_`-separated or case-split segment of an identifier, with an optional
   trailing `s` (`events_sent`, `sentCount`, `events`, `state`), never inside
   another word (`present`, `statement`, `consent`). The segment must sit in
   the asserted subject, not only in an unrelated call; today any operand
   text counts, including a call such as `is_present()`. The `mock` and
   `expect_` call checks keep their current form.
6. **Custom helpers by name are strong only for equality names.** Step 10
   needs a name whose last `_`-segment is `eq`, `equal` or `equals`, or
   which ends in `_eq` / `_equal` / `_equals` / `_matches`, with no `ne`,
   `not` or `neq` segment, and at least two arguments (one for a
   `.assert_*` method call). An inequality-named helper is rule 1's case.
   Any other helper is step 11 `unknown`.
7. **Exact-equality `.any()` membership is exact.** At the new step 12,
   an `assert!` or `debug_assert!` whose whole condition (after message
   arguments are removed, ignoring redundant wrapping parens) is exactly
   `<receiver>.any(<closure>)` assigns `exact_value` / strong when every
   guardrail below holds; anything wider falls through to step 13 weak.
   The check fails for any wrong member value, so it pins an exact value
   exactly like `assert_eq!` (#6991).
   - The receiver ends in a no-argument `.iter()`, `.iter_mut()` or
     `.into_iter()` call over a value path of identifiers, `::` segments,
     field selections, and bare `()` calls: `lines`, `self.buf`,
     `Config::ITEMS`. Receivers with call arguments, turbofish, indexing,
     or operators never match, and neither does a bare `.any()` without
     the iteration call.
   - The closure has one identifier parameter (an optional type
     annotation is ignored; commas nested in `()`, `[]`, or `{}` still
     read as one parameter, while a comma nested only in `<>` stays
     rejected): no `move`, no patterns, no second parameter. Its body
     is exactly one `==` with nothing else: no
     negation, no `<`, `>`, `!=`, `&&`, `||`, no second `==`, no
     parens, brackets, blocks, method calls, or closures. String and
     comment contents are masked before this reading, so an operator
     lookalike inside a literal (`"a>b"`) neither defeats nor forges
     the shape.
   - The left side is the bound element only: the parameter, optionally
     dereferenced (`*l`) or field-selected (`l.total`, `l.0`). Calls
     (`l.len()`), indexing, and foreign roots never match.
   - The right side is a boolean, numeric (with an optional `e`/`E`
     decimal exponent: `1e-3`, `1.5E+6`), string, or character literal,
     or a `::` path whose last segment starts uppercase
     (`Config::LIMIT`, `Color::Red`), or bare `None`. Calls,
     constructors (`Some(1)`), lowercase bindings, and signed or
     borrowed constants never pin. A bare uppercase name (`EXPECTED`)
     never pins either: it may be a local assigned from dynamic data
     (`let EXPECTED = xs[0]`), which no name-only check can
     distinguish from a constant; only a `::` path or `None` (which no
     local binding can shadow) is unforgeable. An operand containing a
     comment (`x == /* budget */ 7`) stays weak: the literal check
     reads the unmasked operand, and comment-tolerant reading is a
     fail-closed residual.
   - A `.any()` shape inside a string literal, a negated condition, a
     compound condition, or an `== true` comparison never matches.
   - `.contains()` keeps its step 13 weak reading; it is out of scope.
   - The grade is classification input, not confirmation: reveal's token
     and effect-observer rules still decide whether a strong oracle
     observes the changed sink, so an admitted assertion on an
     unconfirmed probe keeps its gap. `cell-0096-log-info-captured` no
     longer does: since #7133 a captured-log assertion withholds the gap as
     `static_unknown` under `rust_log_capture_unresolved` (see
     `docs/STATIC_LIMITS.md`), because ripr does not resolve the logger or
     the log format.

### Reveal admission of an excluded sibling

Rule 1 sets the kind and strength of an inequality; reveal still decides
whether it observes the change. When the changed line constructs an error
variant (`return Err(E::Big)`), RIPR-SPEC-0106 Part B refuses an assertion
that names the enum only through sibling variants, because the shared
qualifier is not the changed error's identity. An inequality is the
exception (examples 19 and 20): `assert_ne!(check(20), Err(E::Bad))` or
`assert!(!matches!(check(20), Err(E::Bad)))` excludes the sibling from the
owner's own result, so a change that returned `E::Bad` would fail it.
Reveal admits it as `relational_check` / weak for every family, whatever
the classifier read, when all of these hold:

- the assertion is `assert_ne!` with one of its two operands exactly a
  bare call to the changed owner, or `assert!` whose whole condition is one negated `matches!` with
  that call as the whole scrutinee and an unguarded pattern (message
  arguments never count, and nothing such as `||` or `^` may sit beside
  the `matches!`);
- no foreign import, same-named function in the test's package, nested
  `fn` or local binding of the owner's name makes the call ambiguous;
- no binding visible to the test (a `macro_rules!`, an import or a
  `macro_use`) may rebind `assert!`, `assert_ne!` or `matches!`, and the
  test's file and module chain pass the shared assertion-context gates;
- the other operand, or the pattern, names the changed error's enum only
  through sibling variants.

Any other sibling-only assertion stays refused, so an inequality on a value
that is not the owner's result shares only the enum qualifier and observes
nothing. The admitted oracle is weak, so the finding reads
`weakly_exposed`, never `exposed`.

### Decisions

Steven delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **`assert_ne!` strength.** Adopted: `relational_check` / weak, as rule
   1, and weak `whole_object_equality` for a struct literal. Rejected: a
   new `inequality` kind, because that is a schema addition for a reading
   that is already weak.
2. **Observer words.** Adopted: whole segments as rule 5, because names
   such as `events_sent` do observe an effect and keep their medium
   credit. Rejected: drop step 9's observer words entirely and keep only the
   mock call forms.
3. **Rule 7 kind.** Adopted: `exact_value` / strong at a new step 12
   before the bare-`assert!` catch-all, because the admitted check pins
   an exact member value. Rejected: a new membership kind, because that
   is a schema addition for a reading the existing vocabulary already
   covers; and grading the shape `mock_expectation` or
   `whole_object_equality` to borrow effect-family confirmation, because
   the grade must describe the assertion, not smuggle a verdict.
4. **Family strength overrides.** Adopted: reveal's per-family override
   (`probe_relative_oracle_strength`) may lower but never raise a medium, weak
   or smoke classifier strength, and an exact owner pin (the return-value pin
   and #6692's clone-field pin) needs a strong oracle. Without this an
   `assert_ne!` struct literal's weak `whole_object_equality` (rule 1) came
   back as strong and pinned the owner. Today every exact kind the classifier emits is strong, so the cap
   and the gate change nothing until rule 1 emits a weak exact kind; they
   are what keeps rule 1's weakening from being undone downstream. This
   narrows the owner-pin Non-Goal below: the pin's confirmation is
   unchanged, but it no longer accepts a weak exact oracle. The strength
   gate does not apply to a `relational_check`: RIPR-SPEC-0197's bool-owner
   pin (`assert!(owner(..))` or `assert!(!owner(..))` on a `-> bool` owner)
   keeps this spec's weak `relational_check` kind while the pin raises only
   its probe-relative strength, because the assert fixes the owner's whole
   two-valued result. `owner_pin` is the authority for that shape and its
   runtime controls (`bool_owner_assert_pin_matched_static_and_runtime_controls`)
   back it. The same holds for RIPR-SPEC-0197's inline expect-test pin
   (`expect![["text"]].assert_eq(&owner(..))`, #6578): the oracle keeps
   step 6's `snapshot` / medium kind, and only an owner pin that admits that
   exact spelling raises its probe-relative strength; `insta`,
   `expect_file!` and `assert_debug_eq` snapshots never pin
   (`pin_strength_gate_keeps_exact_pins_strong_and_defers_bool_pins_to_owner_pin`).
   Rejected: requiring the classifier to read that shape as
   `exact_value` / strong, which would duplicate the owner-identity gates
   in the oracle extractor. Rejected: a new kind for weak whole-object
   inequality (decision 1).
5. **Asserted subject for observer words.** Adopted: rule 5's subject is
   every identifier except the name of a free function call (`is_present()`);
   a method or getter name (`store.saved()`) still counts. Rejected: parsing
   the receiver chain, which this line-level classifier cannot do reliably.
6. **Unresolved paths.** Adopted: the classifier sees only the assertion
   text, so `Cfg { .. }` and `Only::Value` cannot be shown to name a struct or
   a single-variant enum and keep today's exact reading, as the rules allow.
   Those two forms of examples 24 and 25 stay `not_established` until type
   resolution reaches the oracle classifier (#6737).
7. **A guard that pins a value.** Adopted: a guard pins a value when one of
   its top-level `&&` conjuncts is `a == b` with exactly one of `a` and `b`
   the matched scrutinee or a name the pattern binds; `e == e` and
   `(a, b) if a == b` compare the match with itself and pin nothing. Rules 2 and 3 then leave the
   assertion to the ordinary chain: `_ if value == 2`, `Some(x) if x == 3`
   and `Err(e) if e == E::Bad` keep today's exact reading.
   RIPR-SPEC-0108's runtime-controlled fixtures
   `wildcard_oracle_guarded_original` and `_wrong` show the guarded equality
   catches the wrong value, and reading it as weak turned their `exposed`
   into a false gap. Any other guard still weakens: `!e.is_empty()`,
   `e.len() > 1` (examples 3 and 21), `e.len() == 3` and `flag == true`.
   Rejected: weakening every guard, as rules 2 and 3's "guard or not" read
   literally, and keeping every guard that contains `==`, which keeps
   `Err(e) if e.len() == 3` as a strong variant pin.

## Required Evidence

- Each row of the Problem table, and each overstatement listed after it,
  reads the kind and strength the rules give, in JSON `related_tests[]`.
- `assert_eq!(check(20), Err(e))` with a bound value stays
  `exact_error_variant` / strong.
- Whole-segment observer names (example 10) still earn `mock_expectation`,
  and `is_present()` (example 8) does not.
- No finding moves to a stronger class. Golden drift lists every related
  test whose kind or strength moved, and every finding that moved from
  `static_unknown` back to a named gap.
- Under #5416, a test whose only assertion is `assert_ne!` no longer
  withholds a gap as `static_unknown`.
- Rule 7: acceptance examples 27 to 29 read the kind and strength
  stated there, in JSON `related_tests[]`.
- Rule 7 moves no verdict-corpus row: the full corpus before/after delta
  is empty (283 cases), with zero new `false_exposed` rows, and
  `cargo xtask check-evidence-promotion-honesty` passes. In particular
  `cell-0096-log-info-captured` kept its gap verdict (it abstains as
  `limited` since #7133) while the golden fixture `exact_any_membership_oracle` pins the fixed grade
  (`exact_value` / strong in `related_tests[]`): the grade is fixed,
  and the remaining gap is reveal observation confirmation, owned
  outside this spec.

## Non-Goals

- No new oracle kind or strength value.
- No resolution of custom helper bodies (RIPR-SPEC-0120 owns macro-wrapped
  assertions).
- No change to reveal's token or owner-pin confirmation, except that an
  exact owner pin needs a strong oracle, apart from the bool-owner and
  inline expect-test pins (decision 4).
- No change to TypeScript or Python oracle classification.

## Acceptance Examples

`score(x) = x * 2` (changed from `x * 3`); `check(x)` returns `Err` above 10.

1. `assert_ne!(score(2), 0)`: `relational_check` / weak.
2. `assert_ne!(check(20), Ok(20))`: `relational_check` / weak.
3. `assert!(matches!(check(20), Err(e) if !e.is_empty()))`: `broad_error` /
   weak.
4. `assert!(matches!(check(20), Err(_)))`: `broad_error` / weak (unchanged).
5. `assert_eq!(check(20), Err(e))`: `exact_error_variant` / strong (unchanged).
6. `assert!(matches!(check(5), Ok(5)))`: `exact_value` / strong (unchanged).
7. `assert!(matches!(check(5), Ok(_)))`: `smoke_only` / smoke.
   `assert!(ready && matches!(check(5), Ok(_)))`: `relational_check` /
   weak, as `assert!(ready)` reads (today `exact_value` / strong; #7695).
   `assert!(total == 4 && matches!(check(5), Ok(_)))`: unchanged, because
   `total == 4` pins a value.
8. `assert!(is_present())`: not `mock_expectation`.
9. `assert_eq!(events_sent.len(), 1)`: `exact_value` / strong (unchanged;
   step 5 wins before step 9).
10. `assert!(events_sent.contains(&id))`: `mock_expectation` / medium
    (unchanged).
11. `assert_not_equal(score(2), 0)`: `relational_check` / weak.
12. `assert_json_eq(actual, expected)`: `exact_value` / strong (unchanged).
13. `ensure!(score(2) != 0)`: `relational_check` / weak (today
    `exact_value` / strong). `ensure!(score(2) != 0 || score(3) == 6)` and
    `ensure!(ready && (score(2) == 4 || done))`: `relational_check` / weak
    (both read `exact_value` / strong before #7558);
    `ensure!(score(2) == 4 && (ready || done))`: `exact_value` / strong.
14. `assert_ne!(build(3), Config { retries: 9 })`: `whole_object_equality` /
    weak.
15. `assert!(opt.is_some_and(|v| v > 1))`: `smoke_only` / smoke (unchanged).
16. `assert!(!events.is_empty())`: `mock_expectation` / medium (unchanged).
17. `assert!(matches!(check(20), Err(_e)))`: `broad_error` / weak (today
    `exact_value` / strong).
18. `ensure!(matches!(check(5), Ok(_)))`: `smoke_only` / smoke (today
    `exact_value` / strong).
19. `assert!(!matches!(check(20), Err(E::Bad)))`: `relational_check` / weak
    (today `exact_error_variant` / strong).
20. `assert_ne!(check(20), Err(E::Bad))`: `relational_check` / weak (today
    `exact_value` / strong at step 5).
21. `assert!(matches!(check(20), Err(ref e) if e.len() > 1))`, `Err(mut e)`
    and `Err(e @ _)`: `broad_error` / weak.
22. `assert!(matches!(check(20), Err(e @ E::Bad)))`: `exact_error_variant` /
    strong (unchanged).
23. `assert!(matches!(lookup(1), Some(ref x)))` and `Ok(x @ _)`:
    `smoke_only` / smoke (today `exact_value` / strong);
    `assert!(matches!(lookup(1), Some(x @ 3)))` stays `exact_value`.
24. `assert!(matches!(lookup(1), Some(_) | None))`: `relational_check` / weak
    (today `exact_value` / strong at step 5).
    `assert!(matches!(check(5), Ok(_) | Err(_)))`: `relational_check` /
    weak (today `broad_error` / weak, which RIPR-SPEC-0227 could credit).
    `assert!(matches!(pair(), (_, _)))` and `matches!(cfg(), Cfg { .. })`:
    `relational_check` / weak (today `exact_value` / strong). The
    `Cfg { .. }` form is `not_established` until the classifier can resolve
    a struct path (decision 6, #6737).
25. `assert!(matches!(lookup(1), Some(1..=5)))`: `relational_check` / weak;
    `assert!(matches!(parse(), Some(Ok(_))))` and `&Some(_)`: `smoke_only` /
    smoke; `assert!(matches!(items(), [_, ..]))` and, for
    `enum Only { Value }`, `assert!(matches!(make(), Only::Value))`:
    `relational_check` / weak, because neither pins a value (the
    `Only::Value` form is `not_established` until enum resolution, decision
    6, #6737);
    `assert!(matches!(items(), [1, ..]))` stays `exact_value`; all read
    `exact_value` / strong today. `assert!(matches!(check(20), Ok(_) | Err(E::Bad)))`:
    `relational_check` / weak (today `exact_error_variant` / strong).
    `assert!(matches!(lookup(1), Some(3) | Some(_)))`: `smoke_only` / smoke
    (weakest alternative).
26. `assert!(r.is_ok() || r.is_err())`: `relational_check` / weak, because
    it accepts every value (today `broad_error` / weak, because step 2
    finds `is_err`). `assert!(!matches!(check(5), Ok(_)))`:
    `relational_check` / weak under rule 1, not rule 3's smoke.
    `assert!(matches!(items(), [..]))` and
    `assert!(matches!(items(), [rest @ ..]))`: `relational_check` / weak
    through the widened pre-check (today `exact_value` / strong at step 5).
27. `assert!(lines.iter().any(|l| l == "audited 42"), "{lines:?}")`:
    `exact_value` / strong (today `relational_check` / weak at the
    catch-all; #6991). The twin `assert!(lines.contains(&id))` stays
    `relational_check` / weak.
28. `assert!(xs.iter().any(|x| x == Config::LIMIT))`,
    `assert!(xs.iter().any(|x| x == None))`,
    `assert!(counts.into_iter().any(|n| n == 42))`,
    `debug_assert!(buf.iter_mut().any(|b| *b == 0xFF))`,
    `assert!(vals.iter().any(|v| v == 0x1E))`,
    `assert!(xs.iter().any(|x| x == 1e-3))`,
    `assert!(xs.iter().any(|x| x == 1.5E+6))`,
    `assert!(rows.iter().any(|r| r.total == 33))`,
    `assert!(pairs.iter().any(|pair: &(u32, u32)| pair.0 == 7))`:
    `exact_value` / strong. The literal forms `42`, `-1`, `'a'`,
    `"a>b"`, `"a|b"`, `r#"raw"#` pin the same way. A bare uppercase
    name (`EXPECTED_ID`) never pins: it may be a local.
29. `assert!(xs.iter().any(|x| x > 1))`,
    `assert!(xs.iter().any(|x| x != 1))`,
    `assert!(!xs.iter().any(|x| x == 1))`,
    `assert!(ready && xs.iter().any(|x| x == 1))`,
    `assert!(xs.iter().any(|x| x == 1 || x == 2))`,
    `assert!(xs.iter().any(|x| x == expected()))`,
    `assert!(xs.iter().any(|x| x == other))`,
    `assert!(ready.any(|x| x == 1))`,
    `assert!(xs.any(|x| x == 1))`,
    `assert!(xs.iter().any(|x| x.len() == 1))`,
    `assert!(xs.iter().all(|x| x == 1))`,
    `assert!(xs.iter().any(|x| x == Some(1)))`,
    `assert!(ids.iter().any(|id| id == EXPECTED_ID))`,
    `assert!(xs.iter().any(|x| x == Some))`,
    `assert!(xs.iter().any(|x| x == 1e))`,
    `assert!(pairs.iter().any(|pair: Pair<u32, u32>| pair.0 == 7))`,
    `assert!(xs.iter().any(|x| x == /* budget */ 7))`,
    `assert!(xs.iter().any(|a, b| a == b))`,
    `assert!(xs.iter().any(move |x| x == 1))`,
    `assert!(xs.iter().any(|x| { x == 1 }))`,
    `assert!(x == ".any(|l| l == 1")`: `relational_check` / weak
    (unchanged).
30. `pretty_assertions::assert_eq!(score(2), 4)` and
    `std::assert_eq!(score(2), 4)`: recognized as `assert_eq!`; under
    `if false { .. }` or in any context RIPR-SPEC-0197 refuses, no oracle
    (`assertion_not_credited`). `other::assert_eq!(score(2), 4)`: not an
    assertion. Nor is `pretty_assertions::assert_eq!(score(2), 4)` when the
    name `pretty_assertions` may mean something other than the crate: the
    file declares a module or an `extern crate` rename with that name,
    imports another item under it (`use pretty_assertions;` and
    `use pretty_assertions::{self};` bind the crate and still count), or
    has an item-position macro call that could emit such a binding; or the
    call sits in a promoted function (a `#[test_case]` test, a borrowed
    check helper, a harness trial) whose oracles are read from the function
    text alone. A binding in another file reached through a glob import, and
    a Cargo package rename, are not detected.

31. `assert_eq!(score(2) != 0, true)`, `assert_eq!(check(20) == Err(E::Bad),
    false)`, `ensure!((score(2) == 4) == false)`,
    `assert!((matches!(check(5), Ok(5)) || ready) == true)`,
    `assert_eq!(ready && matches!(check(5), Ok(_)), true)` and
    `assert!(std::ops::Not::not(matches!(check(20), Err(E::Bad))))`:
    `relational_check` / weak (all `exact_value` or `exact_error_variant` /
    strong before #7598). `assert_eq!(check(20) == Err(E::Bad), true)` and
    `assert_eq!(!is_valid(5), true)` keep their exact readings.
32. `assert_eq!(score(2).ne(&0), true)`, `assert_eq!(check(20).eq(&Err(E::Bad)),
    false)`, `assert_eq!(check(20) == Err(E::Bad), false as bool)`,
    `assert_eq!({ check(20) == Err(E::Bad) }, false)` and
    `assert!(matches!(check(20), Err(E::Bad)).then(|| false).unwrap_or(true))`:
    `relational_check` / weak (all `exact_value` or `exact_error_variant` /
    strong before #7931). `assert_eq!(check(20).eq(&Err(E::Bad)), true)`
    keeps `exact_error_variant`.
33. `assert_eq!(score(2) == 4 && check(2) != Err(E::Bad), true)` and
    `assert_eq!(score(2) == 4 && !matches!(check(2), Err(E::Bad)), true)`:
    `exact_value` / strong from `score(2) == 4` (both read
    `exact_error_variant` / strong before #7947, a kind the excluded variant
    never earns).

## Test Mapping

- Existing: `crates/ripr/src/analysis/extract/oracles/classify.rs` unit tests.
- Existing: `classify.rs` unit test for `ensure!(s != X)` changes with
  example 13.
- Existing: `relational_tests.rs::ensure_disjunctions_and_nested_equalities_are_never_exact`
  and the corpus case `spec0231-ensure-disjunction-not-exact` (the
  disjunction forms of example 13, #7558).
- Existing: `relational_tests.rs::a_matches_conjunct_reads_no_stronger_than_its_pattern`
  and the corpus case `spec0231-matches-wildcard-conjunct` (the `&&` chain
  forms of example 7, #7695).
- Existing: `relational_tests.rs::compound_matches_forms_read_no_stronger_than_their_pattern`
  and the corpus case `spec0231-matches-nested-wildcard` (the nested,
  commented and bool-compared forms of example 7, #7715).
- Existing: `relational_tests.rs::bool_valued_operands_read_as_what_they_assert`
  and the corpus cases `spec0231-bool-compared-inequality` (example 31,
  #7598), `spec0231-bool-method-inequality` (example 32's method
  spelling, #7931) and `spec0231-pinning-chain-relation-kind` (example 33,
  #7947).
- Existing: `pattern_admission.rs::tests::spec_0231_rules_2_and_3_read_the_pattern`
  (the pattern forms of examples 3, 4, 7, 17, 18 and 21 to 26, the pinning
  controls 6, 22 and 23, and decision 7's guards).
- Existing: `classify.rs::tests::spec_0231_pattern_readings_reach_the_classifier_and_scanner`
  and `spec_0231_pattern_readings_reach_the_parser_path` (the same readings
  through `classify_assertion`, the line scanner and `syntax/ra.rs`).
- Existing: `classify.rs::tests::spec_0231_rules_4_to_6_match_whole_names`
  (examples 8, 9, 10, 12, 15, 16 and the `is_ok() || is_err()` form of 26).
- Existing: `reveal.rs::tests::probe_relative_oracle_strength_preserves_family_overrides`
  (a family override never raises a weakened strength).
- Existing: `reveal.rs::tests::pin_strength_gate_keeps_exact_pins_strong_and_defers_bool_pins_to_owner_pin`
  (decision 4's owner-pin gate and its RIPR-SPEC-0197 bool-owner exception).
- Existing (example 30): `crates/ripr/src/analysis/syntax/ra/property_macros.rs`
  `drop_in_assertion_crate_paths_keep_assertion_authority`,
  `qualified_drop_in_assertions_classify_unless_their_root_is_bound_locally` and
  `crates/ripr/src/analysis/classify/owner_pin/tests.rs`
  `shared_return_admission_uses_the_outer_invocation_identity`.
- Verdict corpus: 28 cases `spec0231-*` in
  `fixtures/rust-verdict-corpus/cases/` for acceptance examples 1-26
  (two forms each for 21 and 23), each naming its example in its reasoning
  and labeled with runtime mutant truth. The verdict score cannot see an
  oracle-kind error that leaves the line verdict unchanged, so each case's
  `hard_case` records where ripr's related-test kind or strength differs
  from this spec.
- Rule 1 (inequality): `classify.rs`
  `spec_0231_rule_1_inequality_is_never_exact` covers acceptance examples 1,
  2, 11, 13, 14, 19, 20 and the negated half of 26, with equality controls.
  Fixture `fixtures/inequality_assertion_not_exact` shows example 1's related
  test as `relational_check` / weak, and `cli_smoke.rs`
  `repo_exposure_does_not_count_an_inequality_assertion_as_a_strong_grip`
  shows its seam is weakly gripped in the repo audit (strongly gripped
  before). `spec_0231_rule_1_inequality_helper_precedes_exact_kinds` and
  `spec_0231_rule_1_negated_pattern_must_be_the_whole_condition` pin the
  helper and compound-negation readings, and `rust_index.rs`
  `parser_oracles_follow_spec_0231_rule_1_for_helpers_and_compound_negations`
  checks them on both adapters. `reveal.rs`
  `probe_relative_oracle_strength_never_promotes_an_inequality` and fixture
  `fixtures/inequality_struct_assertion_not_exact` show example 14's struct
  inequality stays weak under every probe family.
  `spec_0231_rule_1_negated_matches_operand_in_equality_is_weak`,
  `spec_0231_rule_1_negated_conjunct_respects_or_precedence`,
  `spec_0231_rule_1_inequality_spellings_are_weak` and
  `spec_0231_rule_1_inequality_helper_keeps_observer_readings` pin the
  negated or `false`-compared `matches!` operand, the `||` boundary, the
  helper and `*assert_ne!` spellings, and the observer exemption.
  `spec_0231_rule_1_matches_comparison_polarity_is_structural`,
  `spec_0231_rule_1_negation_spellings_follow_the_polarity_rule` and
  `spec_0231_rule_1_observer_exemption_reads_receiver_and_name_only` pin
  the polarity rule in every operand order and macro form, the
  `prop_assert*`, `assert_not_matches!` and `.not()` spellings, and the
  name-and-receiver-only observer exemption.
- Reveal admission of an excluded sibling (examples 19 and 20):
  `crates/ripr/tests/error_path_sibling_variant.rs::spec_0231_inequality_excluding_a_sibling_reads_relational_weak`
  and its control `spec_0231_sibling_inequality_off_the_owner_call_stays_refused`.
  Corpus: `spec0231-oracle-assert-ne-err-variant` and
  `spec0231-oracle-negated-matches-err-variant` read `relational_check` /
  weak in their `oracle` rows.
- Rule 7: `exact_membership_any_assertion_admits_exact_equality` and
  `exact_membership_any_assertion_rejects_non_exact_shapes` in
  `classify.rs` pin examples 27 to 29 at the classifier; example 27 is
  additionally pinned end to end by golden fixture
  `exact_any_membership_oracle`, whose audit test reads `exact_value` /
  strong in `related_tests[]` (weak on the pre-rule-7 binary) while the
  finding keeps its `weakly_exposed` gap. Verdict-corpus case
  `cell-0096-log-info-captured` exercises the same assertion shape but
  pins only verdicts, not oracle grades.

## Implementation Mapping

- `crates/ripr/src/analysis/extract/oracles/classify.rs`: precedence chain.
- `crates/ripr/src/analysis/extract/oracles/scan.rs`: the RIPR-SPEC-0106
  upgrade, unchanged.
- `crates/ripr/src/analysis/extract/oracles/patterns.rs`: token-level
  matching for rules 1 and 4 to 7.
- `crates/ripr/src/analysis/extract/oracles/pattern_admission.rs`: the
  pattern reading for rules 2 and 3.
- `crates/ripr/src/analysis/classify/reveal.rs`: the per-family strength
  override is a cap that never raises a classifier-weakened strength, and an
  exact owner pin needs a strong oracle (decision 4; `pin_strength_admits`).
  `excludes_only_sibling_variant` admits an excluded sibling as
  `relational_check` / weak.

## Metrics

- `rust_oracle_strength_overstated`: related tests whose reported strength
  exceeds what their assertion pins, on the acceptance set; must be zero.
