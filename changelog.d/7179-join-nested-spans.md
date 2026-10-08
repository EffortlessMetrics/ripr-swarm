<!-- section: Fixed -->
- The shared seam-finding join now selects the most-specific span when nested
  same-kind seams in the same function collide on a producer finding's probe line,
  binding the inner seam only rather than crediting an enclosing outer seam or
  refusing both as twins. Competing claimants with equal spans continue to fail
  closed (#7179).
