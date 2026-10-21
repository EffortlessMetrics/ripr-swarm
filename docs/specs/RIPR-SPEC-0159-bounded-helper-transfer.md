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
  as `<package or [lib] name>::entry(..)` with no `use`. The qualifier must
  be exactly the root manifest's package or `[lib]` identifier directly
  before the entry, so a longer path, another crate name, a module or `as`
  rename of that name in the test, a local binding of it, a workspace
  member or nested package, an absolute path, a non-root or non-library
  entry (a `src/main.rs` or `src/bin` module, an inline `mod` item) and a
  shadowed entry keep the existing refusal. The edge carries relation only:
  argument rows and entry pairing still read a direct call site, so a
  qualified call earns no value credit it cannot read.
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
  limitation remains), except a uniqueness-only stop that a test
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

- Accept: `test -> classify(" x") -> is_word_start(input, 0)` — the
  helper-owned probe relates to the test, its oracles connect, and its
  operands evaluate with `input = " x"`.
- Accept: a comparison operand that is a unique helper call evaluates
  through a simple binding-tail return.
- Reject: relating through a non-unique callee, a method or
  path-qualified call site, a partial index, recursion, or multiple
  callers. A computed argument stops the value/row transfer (named
  edge) while the reach relation may still hold — the call chain is
  genuinely reachable; only the exact values stop.

## Test Mapping

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
`crates/ripr/tests/crate_qualified_entry_reach.rs` (own-crate qualified
entry call and its refusals).

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

- `analysis/classify/helper_transfer.rs` — the authority.
- `analysis/classify/related_tests.rs` — the relation branch.
- `analysis/classify/activation.rs` — transferred rows, call operands.
- `analysis/classify/context.rs` — the chain on `ProbeContext`.
- `analysis/classifier/evidence.rs` — the hop-propagation stop
  (`helper_result_not_forwarded`).
- `analysis/classifier.rs` (`withhold_helper_uniqueness_as_unknown`) —
  uniqueness-only class rewrite, reach identity stage, and
  `helper_identity_unresolved` (#7080 / #7272).
- `domain/probe.rs` — `StopReason::HelperIdentityUnresolved`.
- `output/human/explain.rs` — ADR 0019 gloss via `describe()`.

## Metrics

No new metric; helper-owned probes with resolvable chains move from
`no_static_path` to the ordinary reachable classes in existing counts.
