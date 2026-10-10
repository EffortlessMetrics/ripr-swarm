# RIPR-SPEC-0159: Bounded helper-call transfer

Status: proposed

Issue: #3296 (parent #3215; builds on #3294 / RIPR-SPEC-0157 and
#3295 / RIPR-SPEC-0158). Classification of a uniqueness-only refusal:
#7080.

## Problem

After #3295, a changed binding inside a helper that related tests
reach only through a short call chain (`test -> classify ->
is_word_start`) still reports `no_static_path`: the relation stage
requires a direct call to the owner, the exact input literals never
cross the helper edge, and the transitive-reach walk (RIPR-SPEC-0114)
can only hint that a caller "may lead here" without changing anything.

## Behavior

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
  A uniqueness stop that no test enters — including a unique wrapper of a
  non-unique helper (`outer` wrapping a second `inner`), or a receiver
  call that only shares the bare name (`req.parse()`) even when that
  line also contains the string or comment text `parse(` — stays
  `no_static_path`. Spaced and turbofish calls the extractor records
  (`parse (...)`, `parse::<T>(...)`) still enter. A path-qualified call
  enters only when some workspace function of that name could be the `T`
  in `T::name(` (`VersionReq::from_str` yes; `serde_json::from_str` no).
  A wrapper-local or test-local `fn`/`let` of the refused name, including
  on a lexical-fallback file, is not an entry.
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
- Every unsupported edge is named, never silently dropped: non-unique
  callee, incomplete workspace index, recursion, multiple callers,
  multiple call sites, computed (non-literal, non-parameter)
  arguments, hop-bound exhaustion, and unsupported return bodies.

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
  owner.
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
`given_helper_chain_refused_for_non_unique_entry_when_tests_only_call_a_receiver_then_no_static_path`);
`analysis/classify/related_tests.rs` (the `HelperOwnerCall` relation
branch); `analysis/classify/activation.rs` (transferred rows and the
call operand); fixtures `helper_chain_{one_hop,multi_hop,controls}`;
`crates/ripr/tests/helper_wrapper_reach.rs` (relation precedence, hop
propagation stop, rebinding stop).

## Non-Goals

- No scanner-state or loop evaluation, no recursive-positive support,
  no trait/dyn/function-pointer/macro/FFI transfer (named V1 stops;
  scanner transitions remain a #3296 remainder).
- No cross-function predicate transfer: a helper-owned probe still
  names its own body's predicates; the caller's predicate on the
  helper call evaluates only when it is the probe's comparison
  operand.
- No propagation claim beyond the existing owner-local machinery.

## Implementation Mapping

- `analysis/classify/helper_transfer.rs` — the authority.
- `analysis/classify/related_tests.rs` — the relation branch.
- `analysis/classify/activation.rs` — transferred rows, call operands.
- `analysis/classify/context.rs` — the chain on `ProbeContext`.
- `analysis/classifier/evidence.rs` — the hop-propagation stop
  (`helper_result_not_forwarded`).

## Metrics

No new metric; helper-owned probes with resolvable chains move from
`no_static_path` to the ordinary reachable classes in existing counts.
