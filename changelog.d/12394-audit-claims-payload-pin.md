<!-- section: Fixed -->
- The repo-wide audit now reads `assert_ok_eq!(call, literal)` / `assert_some_eq!(call, literal)`
  through the same owner-pin decision `ripr check` uses, so the owner's `Ok(..)` / `Some(..)`
  return-value seam reads `strongly_gripped` instead of `reachable_unrevealed`. A refused
  import, direction, expected operand, owner call or a literal the owner does not return stays
  uncredited (#12394).
