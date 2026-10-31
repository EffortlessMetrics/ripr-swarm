<!-- section: Fixed -->
- A changed equality (`n == 5` to `n == 6`) no longer reads as exposed when an earlier branch, `else if` chain or early return in the same function can take the test's row away before it reaches the changed line; the boundary now reads as unresolved instead of crediting a row on either constant (#10856).
