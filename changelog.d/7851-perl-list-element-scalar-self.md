<!-- section: Fixed -->
- Perl fact-packet classification no longer treats equal sink text as
  observation of a changed returned-list element when the only aligned
  strong oracle is a Test::More `is(scalar ...)` (last-element only) or a
  self-comparison. A literal-list `is_deeply` still aligns. Packets whose
  sink text itself names `scalar` or `wantarray` keep the existing
  wantarray-exact credit. RIPR still consumes packets only; it does not
  parse Perl (#7851).
