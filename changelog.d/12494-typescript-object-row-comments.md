<!-- section: Fixed -->
- TypeScript object-row `test.each` callbacks: line and block comments inside an
  otherwise lone `expect(..).matcher(..)` body no longer block row literal
  substitution, so pinned boundary rows stay `exposed` instead of
  `weakly_exposed` (#12494).
