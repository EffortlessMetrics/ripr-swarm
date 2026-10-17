<!-- section: Fixed -->
- A `matches!` guard that equates a field, deref, or tuple projection of a
  pattern binding with a value (`Some(v) if v.id == 3`) now reads as an
  exact oracle. Self-comparisons (`v.id == v.id`, `v.a == v.b`), disjuncts,
  method calls (`v.len() == 3`), and projections of a name the pattern does
  not bind stay weak (#7726).
