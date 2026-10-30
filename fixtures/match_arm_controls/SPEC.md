# Fixture: match_arm_controls

Spec: RIPR-SPEC-0164

## Given

Four caller-side equality boundaries over match helpers: a computed arm
value (`"word" => pick(kind)`), a guard arm (`"word" if kind.len() > 2`),
a bare-identifier pattern (`word => "alpha"`), and a char scrutinee
(`match c { 'a' => ... }`). Each caller binds the helper return into
`final_label` and compares it at an equality boundary; the diff
changes each comparison constant (`"alpha"` -> `"beta"`). The tests
call each classify with one exact literal, and that row returns the
constant the edit replaced (`"alpha"`).

## When

```bash
cargo xtask fixtures match_arm_controls
```

## Then

All four probes read `exposed`. The edit is a one-sided equality swap
(RIPR-SPEC-0032): a row that sits on the replaced constant (`"alpha"`)
takes the `if` branch before the edit and the `else` branch after it,
so the single pinned row flips whichever side the new constant is on.
Runtime mutation of each helper confirms every test fails under the
edit. The bare identifier is read as a catch-all only because the file
declares no item of that name (RIPR-SPEC-0164 Example 4).

The negative shapes (a row on neither constant, a compound predicate
with the gate off, a weak oracle) are the `eq-swap-*` cases of the
Rust verdict corpus; they stay gaps.

## Must Not

- Report a gap or an unknown class on any of the four probes: each
  pinned row is discriminated by the runtime mutant.
- Resolve a helper through argument binding instead of the match
  grammar (the arguments stay exact literals or the owner's bound
  parameter).
