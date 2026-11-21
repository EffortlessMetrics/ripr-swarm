<!-- section: Fixed -->
- Rust return edits behind a tuple-field guard read `weakly_exposed` when every
  related test passes a literal newtype that selects the other branch, including
  through a directly resolved addition on let-bound operands (#15770).
