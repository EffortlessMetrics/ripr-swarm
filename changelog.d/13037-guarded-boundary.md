<!-- section: Fixed -->
- Read `amount >= LIMIT && !flag` as a boundary on `amount` guarded by
  `flag`, instead of treating `LIMIT && !flag` as the boundary operand. A test
  that pins a call on the boundary with the guard held now counts; boundary
  rows that miss the guard stay unclear and the reason names the guard (#13037).
