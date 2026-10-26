# Fixture: match_arm_controls

Spec: RIPR-SPEC-0164

## Given

Four caller-side equality boundaries over match helpers, two of which
break a rule of the strict grammar and two of which now resolve: a computed arm value
(`"word" => pick(kind)`), a guard arm (`"word" if kind.len() > 2`),
a bare-identifier pattern (`word => "alpha"`), and a char scrutinee
(`match c { 'a' => ... }`). Each caller binds the helper return into
`final_label` and compares it at an equality boundary; the diff
changes each comparison constant (`"alpha"` -> `"beta"`). The tests
call each classify with an exact literal.

## When

```bash
cargo xtask fixtures match_arm_controls
```

## Then

The guard and bare-identifier probes stay at their fail-closed class,
`infection_unknown` with `Changed boundary input is unresolved` naming
`final_label` (RIPR-SPEC-0001, #6674): a guard or unresolved pattern
could match anything, and a bare identifier may be a binding — the
token-coincidence family.

The computed-value probe (`pick` returns one literal) and the char
probe (`'a'` arm against a char scrutinee) resolve their operand, since
the match evaluator takes a literal-tail callee and a char scrutinee
with char arms. Each test pins one input only, so both sides of the
changed boundary are not exercised: those probes read `weakly_exposed`,
not `exposed` (a single-row test cannot discriminate the constant).

## Must Not

- Produce any `exposed` finding on any of the four probes: the tests
  pin one row each.
- Produce a hop-provenance string on the guard or bare-identifier
  probe.
- Skip the helper authority entirely: the guard and bare variants must
  be refused by the match grammar, not by argument binding (the
  arguments stay exact literals or the owner's bound parameter).
