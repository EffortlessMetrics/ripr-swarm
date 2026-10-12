<!-- section: Changed -->
- TypeScript predicate-boundary activation is now computed per related test:
  witnessed, reached without a discriminator, a definite miss of a
  module-pinned boundary, unresolved, or not applicable. The finding-wide
  boundary witness reduces those rows, so one test's boundary hit can no
  longer stand in for another test that misses it. Findings, missing text,
  actionability and rendered output are unchanged (#5527).
