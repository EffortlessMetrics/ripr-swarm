# RIPR-SPEC-0228: Rust field writes are probed and observed, not unknown

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

- None yet

Linked PRs:

- None yet

Support-tier impact:

- No tier change. A changed Rust field assignment (`recv.field = expr`,
  `recv.field op= expr`) gets a typed probe instead of `static_unknown`, and a
  test that calls the mutating owner and then reads that field exactly can
  confirm it. Python and TypeScript previews already probe field assignments
  (RIPR-SPEC-0027, RIPR-SPEC-0028); this closes the Rust asymmetry. Claim
  boundaries remain governed by [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. Two proposed `static_limit_kind` values are
  additive and would be documented in `docs/STATIC_LIMITS.md` and
  `docs/OUTPUT_SCHEMA.md` by the implementing PR.

## Problem

State changes through `&mut self` are a core Rust shape:

```rust
impl Counter {
    pub fn bump(&mut self) { self.count += 1; }   // changed from `+= 2`
}

#[test]
fn bumps() {
    let mut c = Counter::default();
    c.bump();
    assert_eq!(c.count, 1);
}
```

The parser emits no shape for assignments or compound assignments: a binary
expression counts only for predicate operators (`analysis/syntax/ra.rs`). The
lexical fallback needs a `:` without `::` to call a line a field
(`analysis/probes/lexical.rs`, `has_field_shape`). It masks strings and
comments on the changed line's own text first; masking is per line, so a
line inside a multi-line string or block comment, or one that starts
inside a string, is still read as code (#8013). So the line becomes a
`static_unknown` probe and the finding reads `static_unknown` with stop
reason `static_probe_unknown` and no `static_limit_kind`
(`analysis/classify/decision.rs`).

Measured on origin/main with `ripr check --diff`, a direct field assertion, a
getter assertion and a test that reads nothing all produce the same
`static_unknown`. ripr cannot tell an observed write from an unobserved one.
`self.state = State::Done` and `*p = v` fall the same way. The "Fill
spec-defined corpus cases" ledger crate reproduces it: `self.balance +=
amount` abstains although an `assert_eq!` catches every mutant.

Neighbouring shapes are already typed: `self.label = compute(x)` reads as a
`call_deletion`, and `self.items.push(x)` reads as `side_effect` with sink
`state_write`. RIPR-SPEC-0094's direct collection observer covers a bare
receiver (`items.push`) and leaves field-path receivers on its Part C.
RIPR-SPEC-0001 says labels "should be available for state writes" without
defining them for assignments.

A bare local accumulator update follows the call-free rule even when its
right-hand side calls something. `count += line.split_whitespace().count();`
and `count = count + f(x);` consume the call's value (#6677), so deleting the
call is not a mutant of the line; the lexical fallback no longer reads them as
`call_deletion` and the line reads `static_unknown`, like `sum += w;`
(`analysis/probes/lexical.rs`, `is_local_accumulator_assignment`; corpus row
`grid-loop-table`). Only a line holding one statement qualifies
(`count += 1; notify(x);` keeps its `call_deletion`). Diff pairing treats a
removed `call_deletion` line rewritten as such an update (`total =
compute(x);` to `total += compute(x);`) as the same edit when the update
consumes the removed line's outermost call, receiver included, or when the
removed call appends to the same accumulator (`push_str`, `push`, `extend`,
`extend_from_slice`, `append`) and the update adds the same argument calls
(`out.push_str(&format!(..));` to `out += &format!(..);`). The removed line
then mints no base-side `call_deletion` (`analysis/probes/diff.rs`,
`has_matching_added_line`). A different outer callee (`notify(total);` or
`notify(compute(x));` to `total += compute(x);`) or receiver (`a.f(x)` to
`b.f(x)`) is a real deletion of the removed call and keeps its
`call_deletion`. Closing braces after the one statement (`total += f(x); }`)
do not count as a second statement. Field, deref and index targets
(`self.total += f(x)`, `*out += f(x)`, `v[i] += f(x)`) and a
non-self-referential `total = compute(x)` keep their current families; the
latter also consumes the call's value, but this rule covers only updates that
accumulate into their target and leaves plain assignments to the existing
`call_deletion` handling. Crediting an accumulator observed
through an exact owner-return assertion needs loop-count reasoning
RIPR-SPEC-0197 leaves out, and stays future work.

## Behavior

### Probe

A changed Rust statement `place = expr` or `place op= expr`, where `place` is a
field path (`self.f`, `recv.f`, `recv.a.b`), produces a `field_construction`
probe whose field identity is the full projection path after the receiver
(`count` for `self.count`, `child.count` for `self.child.count`) and whose
expression is the assignment's right-hand side and operator. Every rule below
matches that full path: a read of `r.count` never observes a write to
`r.child.count`. It does not produce a
`static_unknown` probe for the same span.

### Observation

The probe is confirmed by a related test that:

In this section `f` is the full projection path (`r.child.count`, never
`r.count`, for a write to `self.child.count`).

1. calls the owner method on a local binding `r` (the receiver), and the
   changed statement is not inside a loop or closure, and after it, on any
   path to the owner's return, the body uses `self` only in plain reads of
   fields whose values are copied, or compared with a built-in operator on
   a primitive type (`let n = self.limit;`, `self.count > 3`). A comparison
   that dispatches to a workspace `PartialEq` or `PartialOrd` impl counts as
   a call, because that impl could write interior-mutable state. Any other later use refuses credit, written or
   implicit: an assignment to `f` or a prefix (`self.child = ..`,
   `*self = ..`), a method call on `self` or on any field path
   (`self.reset()`, `self.child.reset()`, `self.count.set(0)` through a
   `Cell`), `self` or a field passed to a call (`zero(self)`,
   `mem::take(&mut self.child)`), a closure capturing `self`, a macro call,
   or a use of any binding that borrows `self` or one of its fields
   (`let p = &mut self.count;` before or after the change). So
   `self.count += 1; self.count = 0` and `self.count += 2; self.reset()`
   both give no credit, and
2. after that call, holds an admitted exact oracle that reads `r.f`
   (`assert_eq!(r.f, v)`, or a whole-value comparison of `r` that names `f`
   under RIPR-SPEC-0225); and
3. between the owner call and that read, nothing else can write `r.f`: no
   method call on `r` taking `&mut self`, no assignment to `r` or `r.f`, no
   `&mut r` passed to a call, and no method call on `r` at all when `f` is a
   `Cell`, `RefCell`, `Mutex`, `RwLock` or atomic (interior mutability).
   Otherwise the read does not confirm
   (`c.bump(); c.reset(); assert_eq!(c.count, 0)` hides the mutant).

A read of a sibling field keeps a `FieldValue` missing discriminator for `f`,
as for constructed fields (RIPR-SPEC-0005). A test that reaches the owner and
reads nothing after the call follows the constructed-field classes: no oracle
on the receiver reads `reachable_unrevealed`, a weak or sibling read reads
`weakly_exposed`. An enum-variant write (`self.state = State::Done`) applies
RIPR-SPEC-0094 Part B variant scoping to the read.

Collection writes on a field path (`self.items.push(x)`,
`self.map.insert(k, v)`) extend the direct-collection observer (#4575, today
limited to `push` on a bare identifier in
`analysis/classify/propagation_witness.rs`) to field-path receivers and to
`insert`, so `c.items` in the test is the observed subject for `self.items`
in the owner. This is new behavior, not reuse. The getter rule under Limits applies to
these reads too (Decision 2): `c.history()` observes `self.log` only when
`history` resolves to a body returning `self.log`, and a shared name alone
never confirms a field-path collection write (examples 12 and 13).

A read of only element zero of the written collection (`c.items.first()`,
`c.items.get(0)`, `c.items[0]`, and the bare-receiver forms) does not
confirm a `push` (#8203): `push` appends, so element zero is the pushed
element only when the collection started empty. Such a test stays
`weakly_exposed`, pinned by `fixtures/rust_preseeded_first_element` in
the RIPR-SPEC-0108 honesty corpus. A read of the pushed element
(`last()`, a non-zero index, or the whole collection) still confirms,
as `fixtures/rust_pushed_element_last` pins. The one exception is a
bare receiver whose test body proves an empty start (a narrow empty
construction such as `Vec::new()` with no test-side seeding of that
receiver): there element zero is the pushed element, and the read
still confirms. Field-path first-element reads stay demoted, because
this observer does not establish the field collection's start state.

### Limits

- A read through a method (`r.count()`) confirms only when the method
  resolves to an inherent method of the receiver's own type (not a trait
  method, a `Deref` target or a same-name method on another type) whose body
  returns `self.f` directly (or a reference, copy or clone of it). Otherwise, when the test holds a strong oracle on the
  method result, the finding reads `static_unknown` with the proposed
  `static_limit_kind` `rust_state_read_path_unresolved`.
- A write through a dereferenced `&mut` parameter (`*p = v`) or through a
  `&mut` argument (`fill(&mut v, 7)` writing `vec.push(..)` in the owner)
  needs parameter-to-argument aliasing. Until that exists the finding reads
  `static_unknown` with the proposed `static_limit_kind`
  `rust_mut_reference_alias_unresolved`.

### Decisions

Steven delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Family.** Adopted: `field_construction`, matching the Python and
   TypeScript previews and reusing the `FieldValue` discriminator.
   Rejected: `side_effect` with sink `state_write`.
2. **Getter credit.** Adopted: resolved-body rule above, fail closed as
   `static_unknown`. Rejected: credit on a name match (`count()` for
   `count`), which a getter returning another field would fool.
3. **Unread writes.** Adopted: `reachable_unrevealed` when no oracle reads
   the receiver after the call, matching constructed fields. Rejected:
   keep them non-actionable until the read set is established another way.

## Required Evidence

- The reproduction reads `exposed` with a `field_construction` finding and no
  `static_unknown` finding on the line.
- A sibling-field control (`assert_eq!(c.label, "x")` only) reads
  `weakly_exposed` with a `FieldValue` missing discriminator.
- A no-read control (`c.bump();` and nothing after it) reads
  `reachable_unrevealed`.
- A getter control returning `self.count` reads `exposed`; a getter returning
  another field does not.
- A `*p = v` control reads `static_unknown` with
  `rust_mut_reference_alias_unresolved`.
- Golden drift: every finding that leaves `static_unknown` is listed with its
  new class; none reads `exposed` without a same-test read of the field.

## Non-Goals

- No interprocedural aliasing.
- No credit through `Debug`/`Display` output or logs.
- No change to `call_deletion` for `self.f = call(..)`.
- No new probe family.

## Acceptance Examples

`Counter { count: u32, label: String }`, owner `bump(&mut self)`, the diff
changes `self.count += 2` to `self.count += 1`.

1. `c.bump(); assert_eq!(c.count, 1);`: `exposed`.
2. `c.bump(); assert_eq!(c.count(), 1);` with `fn count(&self) -> u32 { self.count }`:
   `exposed`.
3. Same as 2 with `fn count(&self) -> u32 { self.other }`: not `exposed`.
4. `c.bump(); assert_eq!(c.label, "x");`: `weakly_exposed`, `FieldValue`
   missing discriminator for `count`.
5. `c.bump();` and no assertion: `reachable_unrevealed`.
6. `self.state = State::Done` (changed from `State::Idle`), test
   `c.finish(); assert_eq!(c.state, State::Done)`: `exposed`; a test
   asserting only `State::Idle` on another field: not `exposed`.
7. `fn set(p: &mut u32, v: u32) { *p = v; }` (changed from `v + 1`), test
   `set(&mut x, 5); assert_eq!(x, 5)`: `static_unknown`,
   `rust_mut_reference_alias_unresolved`.
8. `self.items.push(x)` with argument changed, test
   `c.add(4); assert_eq!(c.items, vec![4])`: `exposed` (measured on main with
   `ripr check --diff`, sink `state_write`).
9. `c.bump(); c.reset(); assert_eq!(c.count, 0)` with `reset` taking
   `&mut self`: not `exposed`.
10. `bump` writes `self.count += 1; self.count = 0;` (first statement
    changed from `+= 2`), test `c.bump(); assert_eq!(c.count, 0)`: not
    `exposed`.
11. `bump` writes `self.count += 2; self.reset();` (first statement
    changed from `+= 3`), with `reset` setting `count = 0`, test
    `c.bump(); assert_eq!(c.count, 0)`: not `exposed`.
12. `self.lines.push(qty)` with argument changed, test
    `o.add(4); assert_eq!(o.entries(), &[4])` with
    `fn entries(&self) -> &[u32] { &self.lines }`: `exposed`. The getter
    shares no name with the field; its body resolves the read.
13. `self.held.push(qty)` with argument changed, test
    `o.hold(3); assert_eq!(o.held(), &[] as &[u32])` with
    `fn held(&self) -> &[u32] { &self.lines }`: not `exposed`. A method named
    after the field is not a read of it.
14. `self.lines.push(Line { sku: sku.to_uppercase(), qty })` with the `sku`
    initializer changed, test `o.add("bolt", 2); assert_eq!(o.lines(),
    &[Line { sku: "BOLT".to_string(), qty: 2 }])` with
    `fn lines(&self) -> &[Line] { &self.lines }`: `exposed`. The pushed
    literal is the whole `push` argument and the assertion compares the whole
    collection, so it compares every field of the pushed element.
15. `self.tags.push(Tag { label: label.to_uppercase(), weight })` with the
    `label` initializer changed, test `o.tag("rush", 1);
    assert_eq!(o.tags().len(), 1)`: not `exposed`. A projection of the
    collection does not read the pushed field.
16. `self.stock.insert(sku.to_string(), qty)` in `restock(&mut self, sku:
    &str, qty: u32)` with the key spelling changed, test
    `s.restock("BOLT", 5); assert_eq!(s.count("BOLT"), 5)` with
    `fn count(&self, sku: &str) -> u32 { self.stock.get(sku).copied().unwrap_or(0) }`:
    `exposed`. The getter looks its argument up in the written map, and the
    test passes the same key to the owner and the getter.
17. Same write in `deliver`, test `s.deliver("BOLT", 5);
    assert_eq!(s.count("NUT"), 0)`: not `exposed`. A read of another key is
    not a read of the inserted entry.
18. Same write in `place`, which returns `Err` before the insert when
    `qty == 0`, test `s.restock("BOLT", 2 + 3); s.place("BOLT", 0).unwrap_err();
    assert_eq!(s.count("BOLT"), 5)`: not `exposed`. A refused owner call
    never reaches the insert. Only an owner call whose result `.unwrap()`,
    `.expect(..)` or `?` consumes (then optionally bound to one name), or a bare call to an owner that returns nothing,
    runs the insert for certain; `.is_err()`, `.is_ok()`, `let _ = ..`, an
    unconsumed `let r = ..` and a bare `s.place("BOLT", 0);` do not count.
    The earlier `restock` also stores the expected value through a computed
    argument, which fails closed on its own, so the refused-call rule is
    isolated only by unit tests.
19. `self.stock.insert(sku.to_string(), qty)` in `receive`, test
    `l.receive("BOLT", 5); assert_eq!(l.held("BOLT"), 0)` with
    `fn held(&self, sku: &str) -> u32 { self.held.get(sku).copied().unwrap_or(0) }`:
    not `exposed`. The same key looked up in another field map is not a read
    of the written entry.
20. `self.stock.insert(sku.to_string(), qty)` in `fill`, test
    `b.fill("BOLT", 3); b.set("BOLT", 5); assert_eq!(b.count("BOLT"), 5)`:
    not `exposed`. `set` rewrites the entry between the owner call and the
    read (rule 3), so the read never sees what `fill` stored. A write on
    the read's own line before the assertion (`b.set("BOLT", 5);
    assert_eq!(..)`), an earlier call that may already store the expected
    value (`b.set("BOLT", 5); b.fill("BOLT", 5)`), a constructor that
    seeds the map, and an owner that writes the map again after the insert
    (`self.stock.insert(..)` twice, `self.bump(sku)`) count the same way.

## Test Mapping

- Existing: `fixtures/python_field_assignment_shape` (parity reference),
  `fixtures/observation_verified_side_effect`,
  `fixtures/observation_unverified_side_effect`.
- Verdict corpus: 19 cases `spec0228-*` in
  `fixtures/rust-verdict-corpus/cases/` for acceptance examples 1-9 and
  12-20,
  each naming its example in its reasoning and labeled with runtime mutant
  truth. Examples 10 and 11 are dead writes: every mutant of the write is
  equivalent, so they have no runtime-labeled case.
- Planned: probe extraction unit tests for `=` and `op=` on field paths.
- Probe boundary (#6676, #6675): `lexical.rs`
  `let_binding_type_annotations_are_not_field_construction` and
  `struct_literal_fields_keep_field_construction_beside_let_bindings` keep a
  `let` binding's `: Type` annotation out of `field_construction` while real
  struct-literal fields keep it;
  `colons_in_strings_and_comments_are_not_field_construction` (#6578) keeps
  a colon inside a string or comment (`format!("{:<8}{:>6}\n", ..)`,
  `println!("total: {}", t)`) out of `field_construction`, so a spurious
  field probe cannot turn a credited format-call line into a gap
  (verdict corpus `lib-pretty-render-shadowed-assert-eq`); `diff.rs`
  `reordered_replacement_does_not_repeat_the_removed_static_unknown` drops the
  removed side's `static_unknown` for a pure operand reorder only when the
  added side's probe carries the removed text as its `before`.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/ra.rs`: emit an assignment shape for field
  paths.
- `crates/ripr/src/analysis/probes/diff.rs` and `lexical.rs`: map it to
  `field_construction`; stop the `static_unknown` fallback for that span.
- `crates/ripr/src/analysis/classify/activation.rs`: receiver read after the
  owner call; `FieldValue` for sibling reads.
- `crates/ripr/src/analysis/classify/propagation_witness.rs`: field-path
  receivers for collection writes.

## Implementation Status

- Collection writes on a field path (`self.f.push(..)`, `self.f.insert(..)`)
  are implemented for side-effect and call-deletion probes (#7131 lane):
  `field_collection_write_path` and `assertion_field_collection_reads` in
  `propagation_witness.rs`, `field_collection_observer_confirms` in
  `reveal.rs`, `getter_plainly_reads` in `builder_override.rs`. One side of
  the assertion (either operand of `assert_eq!`) must read the field from
  the binding the test called the owner on, on a later line, with no
  `let` shadow or plain `binding = ..` assignment of that name after the
  call (on the call's own line too); comments and strings never count as
  the call, and neither does a call inside a nested block, branch, loop or
  closure of the test. The other operand of the `assert_eq!` must not name
  that binding (`assert_eq!(o.entries(), o.entries())` cannot fail). A count
  (`.len()`, `.is_empty()`) confirms only a deleted write
  (`call_deletion`), since a changed argument leaves the count alone. A getter confirms only when it is an inherent method whose body
  is `self.f`, `&self.f`, `&self.f[..]`, `self.f.clone()` or
  `self.f.as_slice()`.
- An empty-collection expectation is implemented (#8051, #8141 review):
  `assertion_expects_empty_collection` in `propagation_witness.rs` and the
  `empty_after_skipped_push` cap in `reveal.rs` recognize
  `assert_eq!(p.log, vec![])` (either operand, `vec![]`, `Vec::new()`,
  `Vec::<T>::new()` or `[]`), `assert_eq!(p.log.len(), 0)` and
  `assert!(p.log.is_empty())` (the receiver must be a plain path without
  operators). When the owner may leave before the changed push
  (`owner_may_exit_before_line`, sharing `may_exit_early`, including a
  guard on the changed line before the expression), such an assertion
  neither confirms observation nor counts as a strong oracle for
  `side_effect` or `call_deletion` probes on a field collection write
  (`field_collection_write_path`, `direct_collection_mutation_receiver`):
  a test that passes with the collection empty took that exit, so it
  cannot tell a deleted or changed push from the original. Without an
  earlier exit the push ran, and a test that empties the collection again
  (`add(1); remove(1)`) can still catch a changed argument, so the
  assertion keeps its strength. Known limit: the early-exit flag is
  owner-wide, so a test that provably bypasses the exit and then removes
  the pushed value is capped with the tests that took the exit; the cap
  cannot read which path one test took. The shared `may_exit_early` also
  counts any `else`, since a `let .. else` block may exit through a
  never-typed call no token catches, so an ordinary `if/else` completed
  before the push widens the same conservative cap.
- Keyed reads of a field-map insert are implemented (examples 16-20):
  `field_collection_insert_key` and `assertion_keyed_getter_reads` in
  `propagation_witness.rs`, `keyed_field_read_confirms` and
  `owner_call_passes_key` in `reveal.rs`, `getter_plainly_reads_key` in
  `builder_override.rs`. The insert key must be an owner parameter or an
  owned copy of it (`sku`, `sku.to_string()`, `sku.to_owned()`,
  `sku.clone()`, `sku.into()`, `String::from(sku)`). One `assert_eq!`
  operand must be exactly `binding.getter(key)`, and `getter` must be an
  inherent method of the owner type taking `&self` and one parameter `k`
  whose body is `self.f.get(k)` or `self.f.get(&k)`, optionally followed by
  `.copied()` or `.cloned()` and then `.unwrap_or_default()` or
  `.unwrap_or(<literal>)`. The other operand must be a literal the getter
  cannot return for a missing key (`Some(..)` against a bare `get`, a
  literal other than the `unwrap_or` default, a non-zero, non-empty or
  `true` literal against `unwrap_or_default`): otherwise the assertion
  passes whether or not the insert ran. `assert!` and `assert_ne!` reads
  do not count. The assertion must open its line. The test binding must
  be declared in the test by a constructor of the owner type that builds
  the map field empty (`constructor_starts_empty` in
  `builder_override.rs`): `let s = Shelf::ctor(..)`, or
  `let s: Shelf = Default::default()`, where the constructor body is one
  struct literal setting the field to an empty std map or set
  (`BTreeMap::new()`, `HashMap::default()`, `Default::default()`) or
  leaving it to a `..Self::default()` or `..Default::default()` spread, or
  a delegation such as `Self::default()` to a constructor that does; the
  field must be declared as a std map or set (`BTreeMap`, `HashMap`,
  `BTreeSet`, `HashSet`), since a custom map's `Default` may seed it.
  `Shelf::default()` resolves to an inherent `default`, then a manual
  `impl Default`, then a derived `Default`; `Default::default()` always
  runs the trait implementation, so it skips the inherent method.
  A struct literal in the test, a seeding
  constructor (`Shelf::with("K", 5)`) or any other initializer fails
  closed, and so does a type name declared by more than one `struct` in
  the workspace, since methods resolve by bare type name. The binding must
  be declared once, at the test's top level (no shadowing `let`, none in a
  nested block). Methods resolve to the one inherent definition outside
  tests; a test-only twin (`#[cfg(test)] impl Shelf { fn count .. }`)
  fails closed. An accepted owner call before the read must pass the same key in the insert
  key's argument position: a literal, or an identifier the test does not
  name again between the call and the read. The call must run to
  completion: a bare statement when the owner returns nothing, otherwise
  ending in `.unwrap()`, `.expect(..)` or `?`, then optionally bound to one
  name the test uses again before the read only as a whole first or last
  operand of an assertion (`assert_eq!(receipt, ..)`): any other use of a
  returned `&mut Self` could rewrite the entry. No `return` may come anywhere in the test
  before the read (a platform skip passes without reading). Rule 3 holds for keyed reads: between that call and the read the
  test may name the binding only as `binding.getter(..)` through the
  verified getter. Between the declaration and the owner call, every
  statement that names the binding must be a plain call
  `binding.m(<literals>)`, optionally ending in `.unwrap()`, `.expect(..)`
  or `?`, none of whose literal arguments equals the expected value under
  any spelling (`7`, `7u32`, `7_u32`), to an inherent method whose body,
  and every `self.m(..)` helper it calls, spells no literal equal to the
  expected value, hands neither `self` nor `&mut self.f` to other code, and
  otherwise only reads it (`.get`, `.contains_key`, `.len`, `.is_empty`) or
  writes it as `self.<path>.insert(k, v)` or
  `*self.<path>.entry(k).or_insert(0) += v` (or `.or_default()`) with `v` a
  parameter that is neither `mut` nor rebound in the body, used as is or
  through `.clone()` / `.to_owned()` (a conversion such as `.into()` turns
  `true` into 1); a helper is
  followed only when every argument it receives is a parameter or a literal;
  the method and its helpers write the field at most once, and no body
  holds a macro call, loop or closure (`method_cannot_store` in
  `builder_override.rs`). An accumulating call (`+=`) must be the only
  earlier call that writes the field, since it adds to whatever another
  call stored (`s.add("K", 2); s.add("K", 3)` reaches 5). A constant, another
  field or a function result in value position (`SEED`, `self.base`,
  `five()`, `self.<path> = Self::defaults()`, `let qty = SEED;`,
  `self.put(k, SEED)`) or a nonzero `or_insert` start fails closed: `s.set("K", 5)`,
  `s.set("K", v)`, `seed(&mut s)` or `s.load_defaults()` storing `("K", 5)`
  before `s.put("K", 5)` may already have stored the expected value. The insert must be a top-level statement of the owner
  (`self.<path>.insert(..)` outside any branch, loop, block or closure),
  and elsewhere the owner may touch `self.<path>` only through
  `.get(..)`, `.contains_key(..)`, `.len()` or `.is_empty()`, directly or
  through the `self.m(..)` helpers it calls (`owner_touches_field_only_at`
  in `builder_override.rs`); a second insert, an `entry(..)` update, a
  bare `self` or an unresolved helper fails closed. The key parameter must
  not be rebound in the owner body, and a `#[should_panic]` test never
  confirms. A field's collection type counts as std only when spelled
  `std::collections::X`, or imported bare from `std::collections` without
  `as` in the struct's own module while no other import, item, alias or
  generic parameter in the file reuses the name (a local `BTreeMap` with a seeded `Default` fails closed). A
  stored value computed from several parameters (`insert(k, a + b)`) is
  not a parameter and fails closed (#7988).
- Not yet implemented for argument-free field reads: rule 3 (no
  intervening write between the owner call and the read). This one does not
  fail closed: a later write through the binding can still read `exposed`
  (#7976). Keyed reads enforce it. Also not yet implemented, all failing
  closed: the `static_unknown` fallback with
  `rust_state_read_path_unresolved` for a strong oracle on an unresolved
  getter, which reads `weakly_exposed` instead (example 13); reads that are
  not a compared side (`let items = &c.items;` then an assertion on
  `items`, `assert!(!c.items.is_empty())`, `c.items.iter().any(..)`), an
  owner called as `Basket::add(&mut c, 4)` or through a helper, and
  `prop_assert_eq!`/`debug_assert_eq!`. A whole-value comparison of the
  receiver keeps the effect-observer path (RIPR-SPEC-0094 Part D).
- A field of a struct or enum literal pushed onto a field collection is
  credited (#7131): `pushed_literal_collection_path` and
  `assertion_whole_collection_equality` in `propagation_witness.rs`,
  `pushed_collection_compared_whole` in `activation.rs`. The literal must be
  the whole argument of `self.<path>.push(..)`, the push a top-level
  statement of the owner body (not in a branch, loop, block or closure),
  and the changed initializer must occur once on its line. Nothing later in
  the owner body may write the field: a use of `self.<path>` other than
  another `push`, a use of a parent field of `<path>` (`self.state` for
  `state.log`), a bare `self` (`*self = ..`, `reset(self)`), or a call
  `self.m(..)` whose inherent body, followed through further `self` calls,
  may do so; an unresolved method counts as a write. The assertion must be
  an `assert_eq!` whose one operand is the whole collection (`inv.log`,
  `inv.history()`, optionally borrowed) read after the owner call as above,
  and whose other operand does not name that binding. Either operand may
  be the read. The test must call the owner on nothing but that binding (no
  `other.add(..)`, `Order::add(&mut other, ..)` or chained
  `Order::new().add(..)`): a value built through the owner may carry the
  changed field into the other operand. From the first owner call to the
  assertion the test may neither write through nor copy the binding
  (`binding_untouched_after_owner_call` in `reveal.rs`): every owner call
  is a bare statement (optionally ending in `.unwrap()`, `.expect(..)` or
  `?`, never bound or chained), the assertion starts its line, and any
  other statement between may name the binding only as a plain field read
  that is a whole argument of `assert!`, `assert_eq!` or `assert_ne!`, with
  no block, closure or nested macro other than `vec!` or `format!`. Projections
  (`.len()`, `[0].qty`, `.iter().map(..)`), `assert_ne!`, literals built in
  a local and pushed later, `insert`, and a push preceded by another
  statement on its line are not credited and keep the missing field-value
  fact. A custom `PartialEq` on the element that ignores the changed field
  is not detected: whole-value equality is taken to compare every field, as
  for RIPR-SPEC-0225 whole-value credit.
- Field assignments (`self.f = ..`, `self.f op= ..`, every compound
  operator) on a field path rooted at `self` are probed as
  `field_construction` in diff scope (`field_assignment_path` in `ra.rs`)
  when the whole assignment sits on one line, every field name is a plain
  ASCII identifier (not `self.0`, `self.r#type` or a non-ASCII name), and
  the right-hand side contains, at any depth, no call, method call, macro,
  closure, `?` or `.await` (those keep `call_deletion` and their other
  probes, per the Non-Goals) and no choice between values (`if`, `match`, a
  comparison, `&&`, `||`; those keep their predicate probe). Other
  assignments read as before. Repo-scope seam inventory skips these shapes
  (`build_seam_from_shape` in `seam_inventory.rs`): its grading does not
  apply the read rule below. So a `reachable_unrevealed` finding on one has
  no repo seam, and `ripr agent stub --at` cannot target it yet.
- One is credited through the same read rule as a pushed literal
  (`assigned_field_of` and `pushed_collection_compared_whole` in
  `activation.rs`): an `assert_eq!` whose one operand is the whole field,
  read directly or through a getter whose body returns it, from the binding
  the test called the owner on, after that call. The assignment must occur
  once on its line and start a top-level statement of the owner body, and
  nothing later in the body may write the field (`assigned_field_path` in
  `propagation_witness.rs`). That later-write check is `may_write_field`
  above, which is narrower than rule 1: it allows later reads of other
  fields, macro calls and closures that do not name the field, and `self`
  calls whose bodies do not write it. Nothing earlier in the body may leave
  before the assignment (`return`, `?`, `let .. else`, `break`,
  `continue`, any macro call including ones that cannot exit such as
  `vec!` or `println!`, `assert`, `.unwrap(`, `.expect(`), the
  right-hand side may not branch, and the asserted value may not be one a
  field usually holds before any write (`0`, `0 as T`, `T::MIN`,
  `T::ZERO`, `false`, `None`, `""`, an empty `String` or `Vec`, a
  `default()`), since deleting the assignment would leave it in place.
- When no related test holds any assertion, the finding reads
  `reachable_unrevealed` (Decision 3). Every weaker reading (`assert!(c.f
  == ..)`, `assert_ne!`, projections, helper assertions, whole-value or
  `Debug` comparisons of the receiver, a sibling-field read, a later read
  through another method) reads `static_unknown` with
  `static_probe_unknown` instead of the `weakly_exposed` that Behavior
  describes (`ClassifiedProbeEvidence::class` in `classifier/evidence.rs`),
  failing closed until those reads are established. Writes through `*p`, a
  local binding or a non-`self` receiver are not probed.
- Known limits shared with other probe families (#8051, #7638): a test
  whose only oracle is a manual `if .. { panic!(..) }`, a
  `debug_assert_eq!` or an unrecognized assertion macro (`assert_that!`,
  `check!`, `prop_assert_eq!`), or whose owner asserts on the field itself,
  reads `reachable_unrevealed`; an assertion that only runs in a dead
  branch, an empty loop or a `#[should_panic]` test still credits; and a
  non-default pre-call value equal to the asserted one
  (`self.count = self.count.max(1)` with `count` already 5) still credits.

## Metrics

- `rust_field_write_static_unknown`: changed field assignments that read
  `static_unknown` without a named limit; must be zero.
- `rust_field_write_direct_read_credit`: field writes confirmed by a same-test
  exact read of the field.
