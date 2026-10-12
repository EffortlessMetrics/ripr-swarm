<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0255): a literal `toHaveBeenCalledWith(...)` payload
  now pins a changed `notifier.send(...)` call when the test passes its mock
  inline as `{ send }`, and when the owner has a callback elsewhere
  (`lines.reduce((sum, line) => ...)`) but the changed call is a top-level
  statement. `checkout("ann", [apple], { send })` with
  `expect(send).toHaveBeenCalledWith("ann", "charged 360")` reads `exposed`
  instead of `weakly_exposed`. On the TypeScript verdict corpus false
  verdicts drop by one (#6654).
