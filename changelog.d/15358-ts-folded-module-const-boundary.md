<!-- section: Changed -->
- TypeScript preview: a changed comparison against a module `const` derived
  from literal integer arithmetic (`const m = 1000 * 60; const h = m * 60;`)
  now resolves the boundary value, so an exact test input at that value
  (`assert.equal(formatShort(3_600_000), "1h")`) witnesses `msAbs >= h` instead
  of leaving it `weakly_exposed` with `observation_unconfirmed`. The fold takes
  only decimal integer literals, other such module constants, parentheses and
  `*`, `+`, `-`, with every step a non-negative safe integer; `let`/`var`,
  reassigned, shadowed, function-local, overflowing, non-integer or cyclic
  bindings keep the operand unresolved (#15358).
- TypeScript preview: a changed comparison on a derived local (`const msAbs =
  Math.abs(ms)`) now credits an exact test input only when the local is an
  identity alias (`p` or `Math.abs(p)`) of exactly one never-written parameter
  and the literal sits in that parameter's argument position. A literal in
  another parameter's position, or a local like `Math.abs(ms) / 2`, `-ms` or
  `ms + other`, stays `weakly_exposed` instead of `exposed` (#15358).
