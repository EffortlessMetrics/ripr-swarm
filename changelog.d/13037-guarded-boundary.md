<!-- section: Fixed -->
- Read `amount >= LIMIT && !flag` as a boundary on `amount` guarded by
  `flag`, instead of treating `LIMIT && !flag` as the boundary operand. A test
  that pins a call on the boundary with the guard held now counts; boundary
  rows that miss the guard stay unclear and the reason names the guard. Such
  a row is no longer credited through the compound-predicate reading (#9637)
  either: a guarded shape ripr cannot read this way (for example a field
  compared against the boundary, or a moved threshold) now reads unclear
  instead of exposed (#13037).
