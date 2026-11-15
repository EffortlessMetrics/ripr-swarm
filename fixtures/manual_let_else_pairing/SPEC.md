# Fixture: manual_let_else_pairing

Spec: RIPR-SPEC-0001

## Given

A Rust diff is the mechanical, behavior-preserving `manual_let_else` rewrite
from dtolnay/semver `d913bcd`: six `let x = match e { Some(x) => x, None =>
return R, }` blocks become `let Some(x) = e else { return R; }`. The scrutinee
and the diverging statement are the same on both sides.

The input keeps only the four changed files, the types they use, and the two
test files that reach them; the other semver modules do not change the
classification.

## When

```bash
cargo xtask fixtures manual_let_else_pairing
```

or:

```bash
ripr check --root fixtures/manual_let_else_pairing/input --diff fixtures/manual_let_else_pairing/diff.patch --mode fast
```

## Then

`ripr` pairs the removed `None => return R,` arm with the added `return R;`
when the scrutinee pairs, so the rewrite yields no repairable Start-here gap
and no `return_value` row whose text equals a removed line modulo the trailing
`,`/`;`.

## Must Not

- Pair a changed scrutinee or a changed return value: both keep their probes.
- Pair a multi-line `None => { .. }` arm or a repeated diverging statement.
- Claim that a behavior discriminator is missing for the paired statement.
