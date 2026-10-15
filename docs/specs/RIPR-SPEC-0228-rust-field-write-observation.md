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

## Test Mapping

- Existing: `fixtures/python_field_assignment_shape` (parity reference),
  `fixtures/observation_verified_side_effect`,
  `fixtures/observation_unverified_side_effect`.
- Verdict corpus: 14 cases `spec0228-*` in
  `fixtures/rust-verdict-corpus/cases/` for acceptance examples 1-9 and
  12-15,
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
- Not yet implemented for these writes, all failing closed: rule 3 (no
  intervening write between the owner call and the read), as before for
  bare-receiver pushes; the `static_unknown` fallback with
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
- Field assignments (`self.f = ..`, `self.f op= ..`) are not implemented:
  their corpus cases read `static_unknown`.

## Metrics

- `rust_field_write_static_unknown`: changed field assignments that read
  `static_unknown` without a named limit; must be zero.
- `rust_field_write_direct_read_credit`: field writes confirmed by a same-test
  exact read of the field.
