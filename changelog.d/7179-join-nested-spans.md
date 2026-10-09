<!-- section: Fixed -->
- The shared seam-finding join now selects the most-specific span when nested
  same-kind seams in the same function collide on a producer finding's probe line,
  binding the inner seam only rather than crediting an enclosing outer seam or
  refusing both as twins. Competing claimants with equal spans continue to fail
  closed (#7179).
- Most-specific selection now fails closed on incomplete candidate sets: a
  repo-seam-limit-truncated `ripr agent card` inventory refuses with the new
  typed `incomplete_inventory` refusal, and the LSP repair card binds only
  through the complete raw seam inventory instead of the class-filtered
  diagnostic projection, omitting the card when that inventory was deferred,
  disabled, or truncated (#7179).
