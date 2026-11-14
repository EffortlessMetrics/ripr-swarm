# Fixture: rust_multi_line_dropped_conjunct

Spec: RIPR-SPEC-0001

## Given

Production code drops a conjunct from a predicate that rustfmt had split
across lines, and moves the surviving comparison's boundary:

```rust
if other > 0
    && 100 <= points
{
```

to:

```rust
if 100 < points {
```

The only test asserts `tier(100, 0) == 0`. That input is false under both
versions (`other > 0` fails before, `100 < 100` fails after), so it separates
neither the dropped operand nor the boundary change.

## When

```bash
cargo xtask fixtures rust_multi_line_dropped_conjunct
```

or:

```bash
ripr check --root fixtures/rust_multi_line_dropped_conjunct/input --diff fixtures/rust_multi_line_dropped_conjunct/diff.patch --mode fast
```

## Then

The changed predicate `100 < points` reads `infection_unknown` and names the
whole old chain `other > 0 && 100 <= points` as the old predicate (#7945).

## Must Not

- Credit the input `points = 100` as boundary evidence for the changed
  predicate: the edit also removes an operand.
- Report the removed line `if other > 0` as a predicate of its own that the
  input `other = 0` exposes.
