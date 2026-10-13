<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0255): a literal `toHaveBeenCalledWith(...)` payload
  now pins a changed `notifier.send(...)` call when the test passes its mock
  inline as `{ send }`, and when the owner has a callback elsewhere
  (`lines.reduce((sum, line) => ...)`) but the changed call is a top-level
  statement. `checkout("ann", [apple], { send })` with
  `expect(send).toHaveBeenCalledWith("ann", "charged 360")` reads `exposed`
  instead of `weakly_exposed`. On the TypeScript verdict corpus false
  verdicts drop by one (#6654).
- TypeScript (RIPR-SPEC-0255 tightens RIPR-SPEC-0247): a changed
  `p.m(...)` call inside a method shorthand or class member
  (`const h = { run(x) { p.m(x) } }`) no longer counts as an owner with no
  nested function, so a literal mock payload stops crediting a call such a
  method may run twice (#6654).
