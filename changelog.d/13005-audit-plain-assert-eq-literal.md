<!-- section: Fixed -->
- The repo-wide audit no longer credits a plain `assert_eq!(owner(..), literal)` whose literal the
  owner provably does not return for those arguments (`Ok(9)` against `Ok(a / b)` at `(17, 5)`).
  It reads the current owner through the same owner pin `ripr check` uses and withholds only what
  that literal check alone refuses; every other reading keeps its credit (#13005, #12394).
