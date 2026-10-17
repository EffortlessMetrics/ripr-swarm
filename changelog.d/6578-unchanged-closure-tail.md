<!-- section: Fixed -->
- A changed operand beside an unchanged closure in an owner's returned tail,
  `lines.iter().map(|l| l.cents).sum::<u64>() + 50` → `.. + 25 * 2`, is now
  exposed by a test that pins the owner's return. Only the part that differs
  from the old line is held to the owner-pin return gate and the propagation
  witness; a change inside the closure, a value a closure captures, or an
  operand a combinator may skip (`map_or(51, ..)`) stays unrevealed (#6578).
