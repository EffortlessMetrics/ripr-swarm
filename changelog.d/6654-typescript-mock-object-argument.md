<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0256): a literal `toHaveBeenCalledWith(...)` payload
  now pins a changed `notifier.send(...)` call when the test passes its mock
  inline as `{ send }`, and when the owner has a callback elsewhere
  (`lines.reduce((sum, line) => ...)`) but the changed call is a top-level
  statement. `checkout("ann", [apple], { send })` with
  `expect(send).toHaveBeenCalledWith("ann", "charged 360")` reads `exposed`
  instead of `weakly_exposed`. On the TypeScript verdict corpus false
  verdicts drop by one (#6654).
- TypeScript (RIPR-SPEC-0256 tightens RIPR-SPEC-0247): a changed
  `p.m(...)` call inside a method shorthand or class member
  (`const h = { run(x) { p.m(x) } }`), including one named like a keyword
  (`{ catch(e) { p.m(e) } }`), no longer counts as an owner with no
  nested function, so a literal mock payload stops crediting a call such a
  method may run twice (#6654).
- TypeScript (RIPR-SPEC-0256 tightens RIPR-SPEC-0247): a literal mock
  payload no longer pins the changed call when the test calls the owner in a
  loop, a callback, a method, a getter, a class or a default parameter value
  (`[7, 8].forEach((id) => announce(id, send))`), where another recorded
  call may match the payload (#6654).
