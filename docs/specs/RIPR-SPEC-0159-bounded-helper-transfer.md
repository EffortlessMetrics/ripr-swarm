# RIPR-SPEC-0159: Bounded helper-call transfer

Status: proposed

Issue: #3296 (parent #3215; builds on #3294 / RIPR-SPEC-0157 and
#3295 / RIPR-SPEC-0158). Classification of a uniqueness-only refusal:
#7080. Raw-identifier receiver/path not an entry: #7270. Reach and
stop presentation of that rewrite: #7272. Qualified
sibling of the uniqueness-stop hop is not an entry: #7268.

## Problem

After #3295, a changed binding inside a helper that related tests
reach only through a short call chain (`test -> classify ->
is_word_start`) still reports `no_static_path`: the relation stage
requires a direct call to the owner, the exact input literals never
cross the helper edge, and the transitive-reach walk (RIPR-SPEC-0114)
can only hint that a caller "may lead here" without changing anything.

## Behavior

- Identity-only prerequisite under #3215: the same helper-transfer producer
  may retain a selected root test declaration, qualified call expression,
  inherent callee SymbolId, root receiver struct declaration and direct helper
  call expression as source-bound spans. This separate receipt does not alter
  singleton-chain resolution, relatedness or exact-input rows. It requires a
  unique same-file plain inherent receiver, one top-level let initializer call
  at each selected edge (optionally followed by `?` without payload credit),
  complete parser-backed inventory and synchronous
  nongeneric declarations. Imports (including glob imports), aliases,
  competing names, traits, deferred code, macro calls/declarations, transforming or
  cfg attributes and multiple selected paths/occurrences refuse. A competing
  path on another root struct is noncompeting only when its member binds to one
  plain inherent function declaration. Associated constants, trait items and
  unresolved or ambiguous member declarations refuse; nominal type identity
  alone does not establish callable identity. Selected test/caller bodies form
  a closed direct-call scope: every nonselected direct call and qualified value
  reference must bind to a plain root function or inherent function declaration,
  regardless of its name. Unknown constants, trait members, external paths,
  computed callees and instance-method calls refuse. This deliberately narrows
  acceptance; checks may live in declared root functions, but their invocation,
  value flow and oracle ownership are not established here. Receipts may
  accompany a Partial returned-ledger witness; they add no propagation edge,
  activation, collection element/index, Config/Result payload or oracle credit.
  Wrapper-local collection/field propagation remains refused.

- One helper-transfer authority (`analysis/classify/helper_transfer.rs`)
  resolves the chain above the owner once; the relation stage
  (`find_related_tests`), the activation rows, and the call-operand
  evaluation all consume the same resolution.
- V1 transfers only: direct same-workspace calls with a **unique
  callee identity** (the #2971 workspace-complete uniqueness rule),
  **positional argument-to-parameter binding** where every bound
  argument is a literal or the caller's own parameter (resolved
  through that caller's rows), at most **3 hops**, **acyclic**, with a
  single caller and a single call site per hop.
- On resolution, a test that calls any resolved hop's caller relates
  to the helper-owned probe (`RelationReason::HelperOwnerCall`): the
  reach, observation, and oracle stages see the same related tests a
  direct call would produce. The owner's exact input rows bind down
  the chain, so #3295's boundary machinery evaluates the helper's
  operands with the tests' literals.
- Method hops (#5352): a hop may also bind at a method call site when
  identity is established by the impl, never by the bare name. A
  `self.<callee>(..)` site binds only when the caller and the callee
  are methods of the same impl self type; a call on a value
  constructed from the callee's type (`Parser { .. }.parse()`,
  `Parser::new(..).parse()`) binds only when the constructed type is
  the callee's impl type. A variable receiver (`parser.parse()`),
  another type's same-named method, and any other receiver fail
  closed. The callee name must still be unique in the workspace, and a
  constructed receiver whose type name the caller's file imports does
  not bind (the import may name another module's or crate's type).
  Forwarding accepts a caller tail that is exactly the identity-bound
  hop call, optionally wrapped in one constructor
  (`Some(Parser { .. }.parse())`), with nothing before the receiver
  (`0 * self.parse()` forwards nothing); the tail's receiver must itself
  bind to the same impl type as the recorded hop site, so a tail call
  on a different owner (`Other { .. }.parse()`) forwards nothing; a
  caller body containing `?` or an
  early `return` still refuses forwarding. An exact assertion on the
  entry's result pins a `return_value` owner through the chain under
  the same conditions as forwarding to the observed hops, with
  exactly one entry call in the assertion, that call being a whole
  operand of the equality (a projection such as `entry(1) % 2`, a
  path-qualified `other::entry(1)`, or an expectation computed through
  the entry, directly or through a test binding, is not credited). A
  `match_arm` owner whose
  scrutinee selection through the entry is not established (for
  example a `self.<field>` set through a `from_str` match behind `?`)
  keeps its fail-closed class; only its reach is established.
- Method entry (#9638): a test may enter the chain through a receiver call
  of a hop caller that is a method, `pool.grow(4)` for `fn grow(&mut self, ..)
  { self.try_grow(..) }`, the form the free-call entry test cannot see. The
  test relates as `HelperOwnerCall` only when the entry is an inherent
  (not trait-impl) method whose first parameter is `self`, `&self` or
  `&mut self`, in production source (no test or evidence role, no
  `#[cfg(test)]` item or impl), no hop between the owner and the entry goes
  through a macro argument, the test's only mentions of the name are
  receiver calls (a path call `Pool::grow(&mut pool, 4)` or a function value
  never counts), no test-local binding, nested `fn` or foreign import
  shadows the name, and every receiver is established as a value of the entry's own
  impl type: a `let` the test binds only to that type's constructor
  (`Pool::default()`, a `Self`-returning associated fn) or struct literal,
  or an inline constructor, read by the owner-pin receiver reader, with no
  foreign or aliased import of the type and exactly one workspace type
  (struct, enum, union or alias, in any module or file) of that name, so a
  same-named sibling-module type never stands for the forwarder's own. A receiver of std or external type
  (`vec.len()`, `HashSet::insert`, `String::is_empty`, an iterator's `next`),
  a parameter or a rebound name is not established and never relates the test.
  The entry being the one workspace function of its name and the identity of
  each hop come from chain construction, and the in-package condition from
  `find_related_tests`, not from this rule: a test in another package is
  refused end to end, though no single guard of that is mutation-pinned. It relates the test only:
  reach, infection and propagation still follow the rules above, so an
  entry that drops or transforms the owner's result keeps
  `propagation_unknown`, and an owner name with two candidates never forms
  the chain (`helper_transfer::test_calls_method_entry`). A forwarder that
  unwraps the owner's result with `unwrap_or_else` or `?` is a transform and
  still abstains, except an `unwrap_or_else` whose handler is a diverging
  workspace function (the diverging unwrap tail below); `mut self` (by value)
  is not admitted.
- Free-call entry pin (#9638): a return-value owner reached through free
  direct calls is pinned by an exact assertion on the entry, as a method hop
  is, when every observed hop is a plain bare-name call the workspace
  resolves to exactly one item: the callee is the one workspace function of
  its name, declared in the caller's own file and enclosing block scopes; it
  is not imported (a `use` of the name, a glob import or a
  `macro_rules!` of it refuses), rebound by the caller through any pattern,
  `let`, `const`, `static`, block-level `use` or nested `fn`, or a parameter
  (plain or pattern) of the caller; and no hop is a method call, a macro
  argument or a path-qualified call (`a::inner(x)`, `crate::inner(x)`). The
  entry is called by the test as a bare name, or (#9638) through the
  package's own library path with no `use` (`pkg::entry(..)`,
  `pkg::module::entry(..)`) when the relation's own-crate authority
  (`related_tests::own_crate_qualified_entry_path`) establishes that spelling: the
  test is a `tests/` crate root of the root package, the entry is an item of
  its library at that module path, and nothing in the test file or body can
  rebind the crate name (a local module, import, rename, glob import, item
  macro, `include!` or `let` of it refuses). The pin reads the established path as
  the entry's operand and adds no resolver of its own. The assertion keeps the
  same one-call,
  whole-operand, exact-value form the method pin requires. A hop
  tail forwards the call unchanged, or shifts it by a decimal integer
  literal (`inner(x) + 1`, `1 + inner(x)`, `inner(x) - 1`) when the owner and
  every observed hop caller declare a builtin integer return type that no
  workspace type or `use` item (an import or rename of the name, or a glob import other
  than a `super::*` / `self::*` glob in a scope that does not enclose the
  function, such as an inline `mod tests`) shadows: adding or subtracting a constant is injective on an
  integer type, so the exact value at the entry moves with the owner's.
  Multiplication, division, remainder, bit operations, negation, `1 - call`,
  a second operand that is not a literal, `as` casts, literal suffixes,
  method projections (`.min(..)`), a branch around the call, a swallowed
  result and a float, string or custom type (`impl Add<i64>`) keep the
  abstention (`propagation_unknown`). Cross-file and cross-crate owners,
  re-exports, aliases and module-qualified hop calls are not admitted: the
  bare name is only trusted where one scope declares it. Only the test's call
  of the entry may be crate-qualified.
- Diverging unwrap tail (#9638): a hop caller whose tail is exactly
  `<hop call>.unwrap_or_else(<handler>)` forwards the owner's result when
  `<handler>` is a path to a workspace function that only diverges, so the
  owner's `Ok` value passes through unchanged and the error path never
  returns. The handler is `name` (a free function) or `Type::name` (an
  inherent associated function of that type, or `Self::name`); it must be the
  one workspace function of its name, outside test or evidence source, with
  no local binding, `use .. as` rename or foreign import of the name (or of
  the type) in the caller's file, no other mention of the name in the caller
  (a parameter, nested `fn`/`const`/`static`/`let` or a glob import in its
  body), defined in the caller's own file, inline module and enclosing
  function scope (read by whole-word item parsing, so a function whose
  signature or name spells `impl` still opens its own scope), with a `Type::` qualifier that is the workspace type and
  not a generic parameter, local item or import of the same name, called on a
  hop whose callee declares `Result<..>` or `Option<..>` with no workspace
  type of that name and no `use` in the callee's file binding that name to a non-std item or glob-importing a non-`super`/`self` module, and its whole body must be one
  unconditional `panic!`, `unreachable!`, `unimplemented!`, `todo!` (none
  rebound by a user macro or import) or `std::process::abort()` /
  `std::process::exit(..)` (with no item, import or `use` binding named `std`
  other than the standard crate anywhere in the workspace). Everything else keeps the abstention: a
  closure (even one that panics), a substituting handler (`unwrap_or(..)`,
  `unwrap_or_default()`, `|_| false`), a handler that sometimes returns, a
  module-qualified, generic or external path, `.ok()`, `.is_ok()`,
  `.unwrap()`, `.expect(..)`, and any further operation on the unwrapped
  value. A handler declared `-> !` is not read: it does not type-check as a
  handler for a value-returning forwarder. The forwarder resolved this way
  must still return a value an exact assertion can pin. A unit-returning
  forwarder (`fn grow(&mut self, n) { self.try_grow(n).unwrap_or_else(..) }`)
  stays out of scope: its effect is observed through state
  (`!v.spilled()`), which needs a state-observation rule this slice does not
  add (`helper_transfer::diverging_handlers`).
- The chain relation outranks any file-path, test-name or token
  proximity reason (#6694, #6672): a test that calls a resolved hop's
  caller relates as `HelperOwnerCall`, not `same_test_file`,
  `owner_named_test` or `weak_token_substring`. A test that calls no hop
  caller keeps its proximity reason.
- A hop-caller call that a test-local binding shadows (a nested
  `fn <name>` anywhere in the test body, or a `let` binding naming it at
  or before the call, such as `let order_discount = |_: u32| 5;`) is not a
  call of the hop caller: it earns no `HelperOwnerCall` and binds no row
  down the chain. The shadow authority is the seam-call one: parser body
  facts on parser-backed files, the masked-body lexical scanners
  otherwise. The call is also treated as shadowed (fail closed) when the
  hop caller's name is not unique among workspace functions (a same-named
  `fn` in `mod tests` or a test file), or when the test's file imports
  another item under that name: a `use .. as <name>` rename, or a `use`
  of `<name>` from a foreign crate. Known gap (#3727): the parser path
  collects only `let` statements, so `if let`/`while let`, match-arm,
  closure-parameter, `for` and macro-introduced bindings do not shadow on
  parser-backed files. That uniqueness refusal stays fail-closed for
  relation and row transfer.   When no other relation remains *and* a test
  calls the ambiguous name as a free function or path-qualified
  associated function (or a non-unique caller of it, such as
  `from_str` wrapping a non-unique `parse`), the finding is
  `static_unknown` naming that function, not `no_static_path` (#7080).
  That rewrite sets reach to `unknown` with an identity summary (not
  `no`) and emits `helper_identity_unresolved` rather than
  `static_probe_unknown` (#7272). Human stop text uses the domain
  `describe()` gloss, which names callee-identity, not a missing probe
  shape (ADR 0019).
  A uniqueness stop that no test enters — including a unique wrapper of a
  non-unique helper (`outer` wrapping a second `inner`), or a receiver
  call that only shares the bare name (`req.parse()`, `req.r#parse()`)
  even when that line also contains the string or comment text `parse(` —
  stays `no_static_path`. A foreign-qualified raw identifier
  (`foreign::r#parse()`) is not a workspace free-function entry. Bare
  `r#parse(...)` is the same identifier as `parse(...)` and still enters
  (#7270). Spaced and turbofish calls the extractor records
  (`parse (...)`, `parse::<T>(...)`) still enter. A path-qualified call
  enters only when some workspace function of that name could be the `T`
  in `T::name(` (`VersionReq::from_str` yes; `serde_json::from_str` no).
  When the uniqueness-stop name is also a resolved hop (`A::parse`
  calling unique `op`), a type-qualified sibling (`B::parse`) does not
  enter that hop; bind the hop like extras already bind wrappers
  (`tests_call_function`). Bare unresolved `parse(...)` still enters
  (#7268 / #7080).
  A wrapper-local or test-local `fn`/`let` of the refused name, including
  on a lexical-fallback file, is not an entry. An `fn` item on the call
  line (`fn parse() { ... }`) is not a free-function entry.
- Hop propagation (#6780): for probe families observed through the
  owner's returned value (every family except `side_effect` and
  `call_deletion`), when the related tests reach the owner only
  through the chain (some `HelperOwnerCall`, no `DirectOwnerCall`), the
  owner's result must be forwarded unchanged through every hop up to the
  highest hop whose caller a `HelperOwnerCall` test calls directly (the
  maximum across those tests); hops above it are not checked, because no
  related test observes their result. When any such test's directly
  called hop cannot be found, the whole chain is checked (fail closed).
  A hop forwards when its caller has no `return` or `?`, does not rebind
  or assign a parameter it forwards, and its tail is the hop call itself
  or `if <call> { L1 } else { L2 }` / `if !<call> ..` with literals `L1`,
  `L2` of distinct value. Otherwise the propagation stage is `unknown`
  (`helper_result_not_forwarded`), so the finding abstains instead of
  crediting the entry's oracle or reporting a gap. Arithmetic or
  let-bound use of the result is not followed in V1.
- Weak oracle behind a non-forwarding hop (#7549): the abstention above
  does not apply when every observed hop's caller is production code;
  every observed call hop carries its callee's value as data (allowlist:
  the callee declares a std string or scalar return type (`&str`,
  `&'static str`, `String`, `char`, an integer or a float), since a user
  type's `Display`, `Clone`, `From` or `Add` impl may decide on the value;
  the caller parses cleanly, does not return `bool`, and its body is one
  tail expression naming the callee, so no call is evaluated and
  discarded; it calls only its
  callee, `Some`/`Ok`/`Err`/`String::from`/`Box::new` and the
  argument-free methods `to_string`/`to_owned`/`clone`; its only
  operator is `+` with a string literal operand; its only macros are
  `format!` and `concat!`, whose tokens name only the callee, the caller's
  parameters and those methods; anything else, including a branch, loop,
  early exit, `?`, cast, closure, index, any other call such as
  `check(title())`, `port_str().parse()` or `TABLE.get(key())`, and any
  other operator, fails the check); no `HelperOwnerCall` test uses `?`,
  `unwrap` or `expect`; and those tests hold at least one assertion, each
  a relational, broad-error or smoke oracle written as a plain `assert!`
  whose condition, under any `!` or parentheses, is an ordering or `!=`
  comparison whose operands are each a non-`bool` literal, a plain call or
  a plain call's `.len()` (`assert!(banner().len() > 0)`), or an
  argument-free `is_empty`/`is_some`/`is_none`/`is_ok`/`is_err` directly on
  a plain call (`assert!(!banner().is_empty())`). A plain call is a path
  call whose arguments are non-`bool` literals, paths or references to
  them; a local binding, a cast or any other method in between
  (`banner().strip_prefix("ripr!").is_some()`) may hold an exact decision. The caller
  then hands the owner's value on as data and the oracle cannot
  discriminate that data exactly, so the existing evidence (the weak
  oracle's gap) stands without pairing. A caller that decides on the
  owner's value, itself or through anything it calls (`if title() ==
  "ripr" { Ok(()) } else { Err(()) }` or `check(title())` under
  `assert!(parse().is_ok())`), can turn a loose check into an exact pin, so
  it keeps the abstention, as do any exact, snapshot, mock or unknown
  oracle, a bare `assert!(caller(..))`, an `==` comparison, any other
  projection and a test-local hop caller (fail closed). Effect families
  take this only when every observed hop is a local-macro argument hop
  (RIPR-SPEC-0117 example 7), whose unexpanded body is not checked for
  data carrying; a plain fn hop above it keeps the target-passing rule.
- Exact oracle behind a data-carrying hop (#11248): for a return-value
  probe, an exact value at the entry pins the owner's value through
  a hop when the owner and every observed hop caller declare a std string
  or scalar return type that no workspace type or import shadows, and the
  caller is one tail expression the weak-oracle rule above accepts as
  carrying its callee as data. Only an injective carry counts: every
  string literal in the caller uses plain `{}` placeholders, so a width,
  precision, radix or `{:?}` spec (which can drop or re-render the value),
  a positional or named argument, a bitwise `&` on the value
  (`format!("{}", base() & 1)`), the callee nested in its own arguments, a
  parameter that shares a method's name and a user `Display`/`From` impl keep
  the abstention. The free-call chain identity rule and the owner-call entry
  pin still apply. A whole-body bare parenthesized `concat!(...)` with one or more readable
  string literals, totaling at most 512 decoded bytes, is evaluated only if the workspace cannot rebind the macro; its exact
  return receives a direct witness. Nested macros, nonliteral and non-string
  operands, empty, bracketed or braced calls, qualified calls and ambiguous
  bindings stay opaque.
  Corpus: `spec0117-external-std-macro`, `format-forward-to-string-credits`
  and `old-title-through-format-macro` credit;
  `format-forward-precision-hides-value` and
  `format-forward-bitand-hides-value` keep abstaining.
- Effect propagation (#6780): a `side_effect` or `call_deletion` probe
  acts on state rather than on a returned value, so it is not judged by
  result forwarding. Under the same chain-only reach and the same
  observed-hop bound, it keeps its owner-local effect-sink propagation
  only when the probe expression names at least one owner parameter (the
  effect target, `out` in `out.push(10)`) and every observed hop passes
  each such parameter through as one of its own parameters, by name and
  not rebound or assigned (`wrapper(out) { record(out) }`). Any other
  argument at that position — a fresh temporary (`record(&mut
  Vec::new())`), a wrapper-local (`let mut v = ..; record(&mut v)`), a
  field, a static or an expression — and a probe that names no owner
  parameter make propagation `unknown` (`helper_result_not_forwarded`):
  the caller's test cannot see that state, so the finding abstains.
- Returned mutable-ledger identity (#3215, ub-review #1363): a constructed
  field directly inside a record passed to an owner parameter's `push` can
  retain the first helper hop's mutable argument and returned local binding in
  `PropagationWitnessV1`. The source field must match uniquely by owner, line
  and normalized field expression: probe locations do not retain occurrence
  offsets, so identical same-line fields refuse identity. The call must be the
  caller's final statement, and its
  exact `&mut local` argument must match the resolved hop. Mutability comes
  from parsed `RefType`/`RefExpr` tokens, never a canonicalized text prefix;
  shared `&mutLedger` types and `&mut_errors` arguments refuse, as do raw
  pointers. The local must have
  one mutable binding and no assignment, and the tail must return that local.
  Caller and owner must share source/type scope and the declared return type
  must match the parameter's pointee, without generic substitutions. Both
  declarations must bind uniquely to the indexed source, be synchronous and
  unattributed direct items in the same lexical source-file/module scope, with
  unique direct declaration names. Const/static/block-local declarations,
  associated functions, attributed enclosing modules (which may rewrite their
  declarations), conditional declarations and different module scopes
  refuse. Attribute checks reuse `attrs_including_inner`, covering function
  bodies and module item lists as well as outer attributes. Source-file inner
  attributes also refuse, except inert lint/doc attributes; no cfg evaluator or
  expansion is introduced. SymbolId prefixes or a free-function fact alone are insufficient.
  Fact body fragments alone omit qualifiers and cannot establish execution. Caller
  imports, callee-named signature/body bindings (including raw names and
  destructuring patterns), value/item declarations, and opaque item/statement macro
  expansions refuse the unqualified helper edge. The owner parameter must be
  its sole matching binding. The same parser-owned opaque-binding boundary
  refuses imports (direct, glob, alias and grouped) and item/statement macro
  expansions in both owner and caller bodies. The returned local also requires
  a sole matching full-function binding. Scoped expression tails introduce no
  binding visible to a subsequent call/push and remain admissible. Rebound,
  shadowed, discarded, sibling-field, transformed-return and later-write cases
  refuse this transfer. This is **partial identity evidence**, not an exception
  to the complete propagation refusal above. A pushed element remains a
  candidate, and the first observer boundary is named
  `returned_mutable_ledger_observer_unresolved`; Config field assignment, index
  selection, preceding appends, branch activation, and exact field observation
  remain unestablished. No oracle, score or exposure class is strengthened.
  This observer limitation replaces the ordinary forwarding refusal only when
  tests directly observe the proved first-hop caller. Higher observed wrappers
  retain `helper_result_not_forwarded`; their identity is not established here.
  The bounded identity endpoints use these authorities consistently:

  | Endpoint | Named-binding authority | Opaque-binding refusal | Other constraints |
  |---|---|---|---|
  | Owner push receiver | Exactly one ordinary/raw full-function binding, the parameter | Owner-body imports and statement/item expansions | Exact field/receiver and mutable parameter; no rebind; synchronous source owner |
  | Caller helper callee | No ordinary/raw full-function binding shadows the resolved owner | Caller-body imports and statement/item expansions | Same file/scope; final direct call matches resolved arguments |
  | Caller returned local | Exactly one ordinary/raw full-function binding, the mutable local | Same caller-body refusal | Direct local declaration, no assignment, exact mutable argument and tail return |

- Boundary pairing through the entry (RIPR-SPEC-0186) reads only rows
  recomputed from the asserting test, and requires the assertion's
  subject to hold exactly one entry call whose arguments are each a whole
  literal, identifier or path; a scalar buried in a compound argument
  (`entry(std::cmp::max(10, 50))`, `entry(10 * 2)`) does not pair.
- A hop argument that is a caller parameter the caller rebinds or assigns
  (`let qty = qty * 2;`, `qty += 1`, a `let`/`for`/closure/match pattern
  naming it) stops the row transfer like a computed argument.
- A comparison operand that is a **direct call to a unique helper**
  (`is_word_start(input, 0) == want`) evaluates through the helper's
  return when the body is simple single-line `let` statements plus a
  bare-binding tail expression evaluated through the #3295 families.
  Anything else fails closed.
- The test-to-entry edge is a direct free-function call. One qualified
  spelling also counts (#8999): an integration test under `tests/` of the
  root package calling a crate-root item of that package's `src/lib.rs`
  as `<lib>::entry(..)` with no `use`, where `<lib>` is the root manifest's
  `[lib] name`, else its package name with hyphens folded to underscores
  (the package name is not a second spelling when `[lib] name` differs; a
  moved `[lib] path` admits nothing). The entry may also sit in a
  `src/` file module or inline module composed under that library, spelled
  with its exact module path (`<lib>::inner::entry(..)`; a segment that is
  not the entry's module path, `crate::`/`self::`/bare `inner::` forms and
  a `use <lib>::inner;` module import are refused), only when no file of
  the package carries a `#[path]` attribute and no binary root also
  declares the module. A module-qualified entry is also refused when any
  `src/` file of the package declares a module segment other than exactly
  once, puts `cfg`/`cfg_attr` on that declaration, an enclosing inline
  module or the entry's own file (read from parsed module items, so a
  comment or string cannot hide the attribute), defines a macro, invokes a
  macro in item position, or does not parse; and for any qualified spelling
  when the test crate invokes an `include`-family macro however its tokens
  are spaced or commented. Imports of the entry (`use <lib>::inner::{entry, ..}`)
  with a bare call were already related; that bare-call route is now
  refused too when the test file imports a path starting with the library's
  name and the file aliases, imports, globs or declares that name, or
  invokes an `include`-family macro, defines a macro or invokes any macro in
  item position (an expansion may declare the name; the prefix then may not
  name the library), imports the entry's name through any `self::`/`crate::`/`super::` path
  (only `crate::` from a unit test under `src/` and the resolved relative
  imports below are credited), or when any
  `const`/`static` of that name appears anywhere in the test file.

  The imported suffix is resolved, not only the prefix: an import of the
  entry's name through the library must spell the entry's definition module
  path exactly (`<lib>::<module path>::<entry>`), or be the crate-root
  `<lib>::<entry>` of a plain unaliased top-level crate-root `pub use` of that same
  path (a `use` inside another module, a non-`pub` use, a glob or macro at
  the root, or any other root item named like the entry refuses). A path reaching the name through another module (a re-export such as
  `pub mod foreign { pub use std::cmp::max; }`), an aliased tail, or an entry
  whose definition path is unknown refuses.

  The library must also bind that module path to the entry's file: every
  file-module segment is declared once by an unconditional, un-remapped
  out-of-line `mod <segment>;` at the top level of the previous file, with no
  other module of that name beside it, and the chain from `src/lib.rs` must
  end at the indexed definition's file (a `#[path]` that compiles the file
  under another name, a same-named inline module, or an orphan declared only
  inside another module refuses). A `#[path]` on an unrelated declaration and a
  binary root that also declares the module do not refuse.

  Known limits (a proper path resolver is a follow-up to #8999): the bare-call
  route credits only a direct `use <lib>::..` import (list imports included)
  where nothing in the test file can shadow the library name. Valid but
  refused routes: an extern-crate alias, a re-export through a test-crate
  module (`pub use ::<lib>::..`, including beside a same-named local module),
  `super::public_api::..` re-export routes and `self::` re-exports through a
  test-crate module; the const/static
  veto is file-wide, so a same-named value inside an unrelated module also
  refuses. The package-wide macro-definition and item-position-macro scan on
  the library side is deliberately coarse and fail-closed. A bare call is read from the masked file at
  its own line, so a spelling only in a string or comment is not a call.
  The qualified spelling is refused for:
  - a path longer or shorter than the entry's own, or another crate name;
  - a test file that declares any item, `let`, closure or typed parameter
    named like the crate, declares a module (`mod <lib>;` or inline) under
    the library's name, aliases anything to that name (`use x as <lib>`), or
    imports a path ending in it (raw `r#` spellings included);
  - a raw `r#` module segment, a glob import, an `include`-family macro
    however its tokens are spaced or commented, a macro definition, or any
    macro invoked in item position in the test file (an expansion may
    declare a module named like the library);
  - a unit test inside `src/`, a test module child (only `tests/<name>.rs`
    and `tests/<name>/main.rs` roots qualify), a workspace member's entry or
    a `src/main.rs` entry;
  - a call spelled only in a string or comment, including a multiline raw
    string or block comment that ends on a line holding a real call (the
    call is read from the masked file at its own line);
  - an entry whose module path is not placed under the library (a
    binary-shared module, a `#[path]` package, a module not declared
    exactly once, or declared under `cfg`/`cfg_attr`).

  A refused case may still render `limitation_last_established_edge: test -> entry`
  from the pre-existing name-only hint path (SPEC-0114/0115); that is a
  projection wording limit, not a reach promotion (tracked in #10026).

  Binders inside macro token trees are not seen by the shadow veto and still
  link (for example `assert_eq!({ let max = other as fn(u32, u32) -> u32; max(10, 5) }, 10)`,
  a closure parameter `|max| max(10, 5)` inside `assert!`, a `match` arm
  binding, or a block-local `fn` inside a macro argument); this is a named
  limit tracked in #9971.

  A relative `use super::<entry>;`, `use super::super::..`, `use super::<module>::<entry>;`
  or `use self::<module>::<entry>;` (#9971) in a unit test under the root
  package's `src/` is resolved against the test file's own module (read from
  the layout) to an absolute module path, and is credited only when that path
  equals the entry's definition module path and both sides are bound: the
  entry by the library declaration chain above, the test file by an
  unconditional, un-remapped chain from `src/lib.rs` whose only `cfg` is a bare
  `#[cfg(test)]` on the test module's own out-of-line declaration and which
  passes through no inline module. Inside the test file each `use` item that
  spells the import is resolved from its own position (#10279): the inline
  modules around it extend the file's module before `self`/`super` apply, so
  the default in-file layout `#[cfg(test)] mod tests { use super::<entry>; }`
  beside the entry resolves to the entry's module, while `super` from
  `flat::tests` names `flat`. Every such item in the file must resolve to the
  entry's module, and one must sit in the same inline module as the test
  function: an import in a sibling module does not bind the test's bare call. It refuses (verdict-corpus rows and the
  honesty corpus pin each): any `#[path]` in the package, a test file in a
  workspace member or under `tests/`, a spelling whose inline-module scope
  resolves elsewhere or past the crate root, a glob import,
  `cfg`-gated `use`, inner `cfg`, macro definition or item-position macro in the
  test file, a renamed or function-local import, a path through `self`/`super`/
  `crate` again or past the crate root, a path that resolves to another module
  or only re-exports the entry, a `cfg`/`cfg_attr`-gated, duplicated or
  `#[path]`-remapped declaration on the chain, and every nearer-binding veto
  above (a body `let`, closure parameter, `fn`, `const`, `static` or tuple
  struct of the entry's name). Verdict-corpus row
  `spec0236-seam-private-tier-src-test-chain` (`use super::checkout;` in
  `src/checkout_tests.rs`, declared `#[cfg(test)] mod checkout_tests;`) is the
  recovered positive; fixture `module_reach_super_import_in_file_tests` is the
  in-file positive and `module_reach_super_import_in_file_wrong_module` its
  wrong-scope counterexample. An in-file import that resolves to another module
  only re-exporting the entry (`mod flat { pub use crate::<entry>; mod tests {
  use super::<entry>; } }`) is an accepted conservative loss, as is a test function nested inside another
  function. Still refused, tracked in #9971: a test module on a
  declaration chain that passes through an inline module (the library index
  does not place such a file in a module context either), a test module under
  a `cfg(test)`-gated parent, and the other routes listed above.

  Accepted conservative losses (tracked in #9971): the fully qualified
  spelling of a crate-root re-export of an inline-module entry
  (`qual_reach::entry(..)` for `mod inner { pub fn entry }` plus
  `pub use inner::entry;`) is refused because the spelled path is shorter than
  the entry's module path (the bare `use qual_reach::entry;` import links); and
  module visibility is not checked, so a spelling through a private module
  (which would not compile) is credited like a public one; and
  the library-binding proof reparses every non-test source file once per module
  segment, a coarse cost boundary left unmemoized.

  Fail-open shapes kept as known limits (tracked in #9971): the bare-call
  prefix proof returns the older name-only result when the package has no
  resolvable `[lib]` name (a moved `[lib] path`), when the entry sits in a
  workspace member, and when the test file has no own-crate import at all
  (for example `mod foo { pub use std::cmp; } use self::foo::cmp::max;` in a
  member crate). A body-local value item of the entry's name (`fn`, `const`, `static`,
  tuple or unit `struct`) whose block holds the call refuses; type-namespace
  items (`type`, `trait`, `enum`, `union`, `mod`, braced `struct`) do not
  shadow a value call.

  The edge
  carries relation only: argument rows and entry pairing still read a
  direct call site, so the qualified spelling adds no new value-credit
  path of its own (the argument reader may still read a qualified
  literal on its own terms). (A path-form guard for absolute paths exists in code as defense in
  depth; no test exercises it.)
- Every unsupported edge is named, never silently dropped: non-unique
  callee, incomplete workspace index, recursion, multiple callers,
  multiple call sites, computed (non-literal, non-parameter)
  arguments, hop-bound exhaustion, and unsupported return bodies.

## One-hop primitive field observation

A field-construction value probe may carry its existing complete local witness
through one production free-function wrapper when that wrapper's sole tail is
`owner(parameters...).changed_field`. Both functions are unique, same-file,
unattributed root functions with immutable `usize` parameters. The owner solely
returns a local record literal whose fields are declared builtin `usize`, with
initializers restricted to decimal literals, parameters, parentheses and `+`/`-`
within a depth budget of 16. The workspace must satisfy the existing closed-field
premise and contain no impl blocks or procedural-macro declarations.

The observer must be an admitted, executing plain `#[test]` assertion in the
same file: `assert_eq!(wrapper(decimal_literals...), decimal_literal)` (either
operand order). The existing propagation witness carries the exact field,
owner, wrapper, source and assertion identities with semantic and validation
digests. Reach and an unrelated strong equality cannot be pooled into a path.
Wrong-sink, discarded-result and sibling-field assertions keep the existing
`helper_result_not_forwarded` refusal.

This exception does not support indexing, collections, method receivers, nested
helpers, let-bound or transformed projections, aliases, custom impls, generated
code, test-helper wrappers, or general interprocedural evaluation. It does not
change punctuation extraction. Existing direct-field observation remains valid.

## A public inherent method entry and a module-qualified hop (#8999)

A changed private helper (`matches_caret`) that integration tests reach only
through a public method (`req.matches(&version)`) read "no test is seen
calling the helper": the chain entry is a method, not a free function, and the
hop into the helper layer is spelled `eval::matches_req(..)`, a path-qualified
call that name identity alone does not bind (#3296 review B1). Two additions
close that gap, both reach-only and both fail-closed:

- **Module-qualified hop.** A hop whose caller spells the call
  `<qualifier>::<callee>(..)` binds when the callee is the one function of its
  name, a module-level `fn` with a body, caller and callee both items of the
  root package's library (composing under `src/lib.rs`, no `#[path]` remap,
  every module segment declared once), the caller mentions the name exactly
  once, and the qualifier (`crate::`, `self::`, `super::` or a child module of
  the caller's module; no import, crate alias, type alias, generic parameter,
  local declaration or item expansion can rebind a bare first segment) resolves to the callee's module path. Anything else keeps the
  "not statically splittable" stop. The bound hop carries the same positional
  arguments as a direct call.
- **Inherent method entry.** A test relates to the helper as
  `helper_owner_call` when it calls a hop caller that is a plain inherent
  method taking `self` (not a `fmt` owner, which keeps its own route) on a
  receiver the established receiver rules bind to the impl's self type (a
  struct literal or a `let` annotated with the type). Constructor calls and
  `Type::method(..)` paths remain unbound on this route. The
  method's name must be defined exactly once in the index, every hop from the
  helper up through that method must be an identity-bound non-macro call (a macro-argument hop
  carries reach only), the test must be in the owner's package, and the test
  must not rebind the type (a same-named type in its module scope or an
  out-of-line parent module, or a `use .. as Type` rename).

The receiver name must resolve through the shared call-identity authority to
one nongeneric declared struct, enum or union in the method impl's own module. The
root-package library declaration path and the test's import must be established;
foreign unrenamed imports, sibling namesakes, aliases, generic self types and
unreadable or generated bindings decline. Qualified type references and
constructor calls in the test decline even beside a genuine unused import.
Method sites must use a plain ASCII receiver name bound by a distinct top-level
`let` with a direct record literal or a whole bare type annotation, taking the
value rather than a reference pattern. The inherent method must take `&self`;
the shared owner-pin prelude-method veto also applies. Visible imports must name
unique unconditional data declarations in the owner's module, without competing
declarations or imports. Source attributes must satisfy the shared source-item
preservation rule (with the built-in `#[test]` admitted); unsupported attributes,
including derives, remain outside this narrow route. Foreign, renamed (including
`as _`), glob and unresolved imports, and local trait declarations, decline.
Local item bindings, field/path suffixes, inline/parenthesized receivers, non-ASCII identifiers, nested,
closure, destructuring and later shadow bindings remain unestablished. The receiver route permits only unrebound bare standard assertion and `vec!`
macros; pattern macros and stringified tokens establish no reach. Qualified-hop
callers with any macro invocation decline. These checks share the helper-transfer
macro binding and invocation authority, with a narrower entry allowlist.
This bounded route intentionally does not infer factory return types or re-export
chains. Non-ASCII qualified helper paths decline without slicing inside UTF-8.

A chain prefix containing a module-qualified hop uses only these stricter
method-entry rules. Ordinary direct/self-method forwarders retain the separate
rule in examples 10 and 11; that route cannot restore a refused qualified
entry. A qualified hop above a tested lower entry does not change that lower
entry's authority. The qualified-hop provenance also refuses result forwarding and effect-target
propagation; the existing bare-call effect rule cannot restore either.

This is reach only. The helper's result is not shown to be forwarded to the
test's assertion, so findings remain below `exposed`; the version-requirement
example reads `infection_unknown` with `helper_result_not_forwarded`. Named limit: the route does not
look at how a hop uses the callee; a call inside a loop, a branch or a `match`
arm still reads as reached because the chain records one call site and no
control-flow condition. That is acceptable only because the route credits
reach and nothing else.

## Relation to RIPR-SPEC-0114

The 0114 transitive-reach walk is **lexical** (name-match only) and
stays fail-closed: it names a candidate path and never changes the
classification. The 0159 transfer is a different, stronger relation —
typed callee identity, workspace-complete uniqueness, and exact
argument binding — and only that relation relates tests and carries
values. A chain the typed relation cannot fully resolve keeps the 0114
limitation unchanged, except the #7080 uniqueness-only case: when a
  test calls the ambiguous entry, classification becomes `static_unknown`
instead of leaving a false `no_static_path` for 0114 to annotate.
Reach on that rewritten finding is `unknown` and the stop reason is
`helper_identity_unresolved` (#7272).

## Required Evidence

- The reproduction flip (helper-owned probe: `reach no` + limitation
  hint -> `reach yes` with both exact oracles related through the
  chain) and the removal experiment: with the relation branch disabled
  the reproduction regresses to the limitation-hint state.
- Authority unit tests: one-hop resolution with arguments, non-unique
  callee stop, incomplete-workspace stop, recursion stop, multi-caller
  stop, entry-hop test reach, helper return evaluation (identity tail
  evaluates; non-identity closure fails closed).
- `rust_constructor_field_wrong_field_observer` re-blessed and its
  corpus expectation graduated (`no_static_path` ->
  `propagation_unknown`): its chain is genuinely resolvable under the
  typed conditions, so the test legitimately relates while
  `must_not_promote` and every repair guard stay intact.
  `rust_transitive_reach_positive` keeps its SPEC-0114 pin exactly: a
  second same-name `inner` makes the callee non-unique, the typed
  transfer refuses, tests call the unique wrapper `outer` rather than
  `inner`, and the lexical-walk limitation stays the pinned
  `no_static_path` outcome. A test that instead calls the non-unique
  entry reads `static_unknown` naming that entry (#7080).
- The fixture corpus: one-hop positive/negative with exact tests,
  bounded multi-hop, the fail-closed controls (same-name helper in
  another module, computed argument, wrong-sink assertion).
- Golden blast radius measured; `cargo xtask goldens check` and
  `cargo xtask dogfood` green otherwise.

## Required guards

- Call reach alone never establishes propagation or discrimination:
  the oracle still has to observe the sink through the ordinary
  evidence stages.
- A chain that stops keeps the pre-#3296 output exactly (the 0114
  limitation remains), except a non-predicate probe whose chain-edge
  argument cannot bind, which abstains as `infection_unknown` (example 9,
  #9889), and except a uniqueness-only stop that a test
  actually enters, which abstains as `static_unknown` naming the
  ambiguous function (#7080) rather than claiming no test reaches the
  owner. That finding's reach stage is `unknown` and its stop reason
  names callee-identity (`helper_identity_unresolved`), not a missing
  probe shape (#7272).
- The workspace-completeness gate is mandatory: a partial index never
  transfers (a same-named function in an unindexed file would make the
  name falsely unique).
- No cross-crate, trait/dyn/function-pointer, macro, or loop transfer
  in V1; those edges stop by name.

## Acceptance Examples

1. Accept: `test -> classify(" x") -> is_word_start(input, 0)` — the
   helper-owned probe relates to the test, its oracles connect, and its
   operands evaluate with `input = " x"`.
2. Accept: a comparison operand that is a unique helper call evaluates
   through a simple binding-tail return.
3. Reject: relating through a non-unique callee.
4. Reject: relating through a method call site (`gauge.span_of(input)`)
   in a hop's callee position; a test's receiver-call entry into a method
   forwarder (`pool.grow(4)`) is the separate rule in examples 10 and 11.
5. Reject: relating through a path-qualified call site whose qualifier
   is not shown to name the callee's own module (`meter::reach_of(input)`
   where `meter` is imported, or names another module). A qualifier that
   resolves to the one callee's module binds (#8999).
6. Reject: relating through a partial index.
7. Reject: relating through recursion.
8. Reject: relating through multiple callers.
9. A computed argument stops the value/row transfer (named edge) while
   the reach relation may still hold — the call chain is genuinely
   reachable; only the exact values stop.
   A non-predicate probe (return value, match arm, field) reached only
   through the chain then reads `infection_unknown`, not a gap, when a
   chain-edge argument cannot bind: anything but a strict literal, the
   caller's own parameter text, or `self.<tuple index>`. That covers
   method calls (`input.trim()`), locals, named constants, negative
   literals and prefix forms (`&input`, `*input`), and abstains wholesale
   (#9889). The trade-off is precision: `&input`, `.clone()` and
   constants abstain although the value is knowable.
10. Accept: a method forwarder that hands the owner's result back unchanged,
    called with a receiver by an integration test that pins the boundary row
    (`forwarder-hop-forwarded-hit`: `pool.grow(4)` asserted `Ok(false)` for
    `grow` forwarding `self.try_grow(wanted)`). The verdict is `exposed`.
11. Reject: a forwarder that drops the owner's result
    (`forwarder-hop-swallowed-result`, `propagation_unknown`), a forwarder
    name shared with another type's method whose test calls the other one
    (`forwarder-hop-name-collision`), and an owner name with two candidates
    where the test calls the forwarder of the unchanged one
    (`forwarder-hop-multi-candidate`): none is credited. A forwarder named
    like a std method (`len`, `insert`, `is_empty`, `next`) is entered only by
    a receiver built as the owner's own type, never by the std receiver
    (`helper_wrapper_reach`, `forwarder_method_named_like_a_std_method_*`).
12. Accept: a forwarder `self.try_dock(wanted).unwrap_or_else(die)` where `die`
    is the one workspace function whose body is `panic!`, with a test pinning
    `dock(4)` (`forwarder-hop-unwrap-diverging-handler`). The verdict is
    `exposed`.
13. Reject: a forwarder that collapses the result with `.is_ok()`
    (`forwarder-hop-unwrap-is-ok-tail`), a substituting or closure handler, a
    non-diverging, conditional, ambiguous, module-qualified, shadowed or
    external handler: none is credited
    (`helper_wrapper_reach`, `unwrap_tail_*`).
14. Accept: an entry `plus_one(x)` returning `plus_one_inner(x) + 1`, with
    an integer owner, called by an integration test that pins `plus_one(5)`
    to `11` (`free-call-offset-plus`, `free-call-offset-minus`). The verdict
    is `exposed`.
15. Reject: an entry that clamps the owner's result (`.min(10)`), swallows it
    (`let _ = inner(x); 10`) or takes the callee as a parameter
    (`fn passed_in(x, shift: fn(i64) -> i64)`): none is credited
    (`free-call-clamped-tail`, `free-call-swallowed-owner`,
    `free-call-parameter-shadow`; `free_call_return_pin`).
16. Accept: an entry `plus_one(x)` returning `plus_one_inner(x) + 1`, or an
    entry `tiers::bumped(x)` returning `bumped_inner(x) + 2` in a child
    module, called by an integration test that pins `crate_pin::plus_one(5)`
    or `crate_pin::tiers::bumped(3)` with no `use` (`qualified-pin-root-entry`,
    `qualified-pin-module-entry`). The verdict is `exposed`.
17. Reject: the same qualified pin when the entry swallows the owner's result,
    the test declares, imports, globs or macro-defines a module of the crate's
    name, or two modules declare the entry's name
    (`qualified-pin-swallowed-owner`, `qualified-pin-local-module-shadow`,
    `qualified-pin-glob-shadow`, `qualified-pin-macro-shadow`,
    `qualified-pin-twin-entry`; `crate_qualified_free_call_pin`): none is
    credited.

## Test Mapping

`crates/ripr/tests/concat_literal_return.rs` covers direct and forwarded
literal returns and opaque or shadowed macro refusals.

`analysis/classify/helper_transfer.rs` `tests`;
`analysis/classifier.rs` (`given_helper_chain_refused_for_non_unique_entry_when_tests_call_it_then_static_unknown`,
`given_helper_chain_refused_for_non_unique_callee_when_tests_call_a_unique_wrapper_then_no_static_path`,
`given_helper_chain_refused_for_non_unique_entry_when_tests_only_call_a_receiver_then_no_static_path`,
`assert_uniqueness_unknown_identity_stage`);
`output/human/explain.rs` (`helper_identity_unresolved_explain_uses_domain_describe`);
`analysis/classify/related_tests.rs` (the `HelperOwnerCall` relation
branch); `analysis/classify/activation.rs` (transferred rows and the
call operand); fixtures `helper_chain_{one_hop,multi_hop,controls}`;
`crates/ripr/tests/helper_wrapper_reach.rs` (relation precedence, hop
propagation stop, rebinding stop);
`crates/ripr/tests/method_helper_reach.rs` (#5352 method hops: public
entry credit, smoke/wrong-owner/variable-receiver controls, the
wrong-receiver tail-forwarding control, the #8633 review controls
(transformed tail, self-computed expectation, projected entry value,
foreign-qualified entry, nested comparison, imported constructor), and the
unselected `match_arm` control);
`crates/ripr/tests/helper_wrapper_reach.rs` (`forwarder_method_*`: the method
entry credit and its swallowed, transformed, shared-name, two-candidate,
shadowed, path-call, unwrap-tail (including shadowed-name, other-module, other-file and custom-type controls), std-receiver, unestablished-receiver, same-named-type, root-package-member, external-trait-impl,
`#[cfg(test)]`, no-self, macro-hop and other-package controls);
`crates/ripr/tests/free_call_return_pin.rs` (the free-call entry pin and its
offset, type, name-binding, scope, test-form and trait-entry controls);
`crates/ripr/tests/crate_qualified_free_call_pin.rs` (the qualified free-call
entry pin: crate-root and child-module positives, and the swallowed,
reshaped, non-pinning-test, rebound-crate-name, duplicated-name, wrong-module,
trait-entry and cfg-variant controls);
`crates/ripr/tests/crate_qualified_entry_reach.rs` (own-crate qualified
entry call, `[lib] name` and hyphen spellings, and each refusal above).
`crates/ripr/tests/helper_wrapper_reach.rs` (qualified effect refusal and lower
forwarder preservation), `helper_transfer.rs::tests::method_entry_checks_only_the_prefix_the_test_enters`;
fixture `rust_qualified_helper_effect_reach` and its honesty-corpus rows;
`crates/ripr/tests/pub_api_method_entry_reach.rs` (#8999 public inherent
method entry through a module-qualified hop: typed-receiver positives, and the
untyped/constructor-factory, `str::matches`, second-method, foreign/renamed
imports, foreign UFCS, sibling namesakes, crate/type aliases and Unicode
qualifier refusals; the class stays below `exposed`);
`analysis/classify/helper_transfer/qualified_hop.rs` `tests` (the qualifier
resolution and each decline); corpus cases `spec-b-0159-path-qualified-call-site` (accept shape of
example 5) and `spec-b-0159-path-qualified-imported-module` (reject shape).

## Non-Goals

- No scanner-state or loop evaluation, no recursive-positive support,
  no trait/dyn/function-pointer/macro/FFI transfer (named V1 stops;
  scanner transitions remain a #3296 remainder).
- No cross-function predicate transfer: a helper-owned probe still
  names its own body's predicates; the caller's predicate on the
  helper call evaluates only when it is the probe's comparison
  operand.
- No propagation claim beyond the existing owner-local machinery and the exact
  one-hop primitive field observation described above.

## Implementation Mapping

- `analysis/classify/propagation_witness.rs` — bounded literal `concat!`
  return evaluation and direct witness; `analysis/classifier/finding.rs`
  removes the macro opacity reason only for the admitted return.

- `analysis/classify/helper_transfer.rs` — the authority.
- `analysis/classify/related_tests.rs` — the relation branch.
- `analysis/classify/helper_transfer/qualified_hop.rs` — the module-qualified
  hop (#8999); `analysis/classify/related_tests.rs` `method_entry_hops` and
  `calls_helper_method_entry` — the inherent method entry (#8999).
- `analysis/classify/activation.rs` — transferred rows, call operands.
- `analysis/classify/context.rs` — the chain on `ProbeContext`.
- `analysis/classifier/evidence.rs` — the hop-propagation stop
  (`helper_result_not_forwarded`) and the chain-edge infection abstention,
  with `analysis/classify/activation.rs` (`chain_row_transfer_stop`).
- `analysis/classifier.rs` (`withhold_helper_uniqueness_as_unknown`) —
  uniqueness-only class rewrite, reach identity stage, and
  `helper_identity_unresolved` (#7080 / #7272).
- `domain/probe.rs` — `StopReason::HelperIdentityUnresolved`.
- `output/human/explain.rs` — ADR 0019 gloss via `describe()`.

## Metrics

No new metric; helper-owned probes with resolvable chains move from
`no_static_path` to the ordinary reachable classes in existing counts.
