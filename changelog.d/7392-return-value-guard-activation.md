<!-- section: Fixed -->
- Python preview adapter: a `return_value` change inside a guarding `if` arm
  no longer reads `exposed` off a strong exact-value oracle whose inputs never
  enter the arm. When every strong related call provably fails the guarding
  comparisons, the changed line never executes under an observed input, so the
  verdict fails closed to `weakly_exposed`, names the guarding condition as the
  missing discriminator, and lists the literal inputs the strong calls actually
  bind (#7392).
- The guarded-arm gate engages only where static evidence can read everything
  it needs: the enclosing `if`/`while` conditions, their comparison operands,
  and the literal arguments strong related tests bind. Unguarded changes,
  `else` arms, computed operands, non-literal call rows, and changes without a
  readable guard predicate (for example a pure-expression change with no
  enclosing comparison) keep the existing verdict instead of downgrading
  (#7392).
