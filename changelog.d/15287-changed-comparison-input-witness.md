<!-- section: Fixed -->
- A Python `!=` comparison whose operands change (`n != 11` to `n != 10`, or a
  slice bound and its literal together, `tag[:4] != "musl"` to
  `tag[:3] != "mus"`) reads `exposed` only when a related test call gives the
  old and the new comparison different results. Inputs that both versions
  treat alike, and inputs or slices ripr cannot read, read `weakly_exposed`
  (#15287).
