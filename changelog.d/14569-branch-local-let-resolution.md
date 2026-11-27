<!-- section: Fixed -->
- Repo seam grip: the branch-local `return_value` gate now reads the `let`
  binding the assertion actually pins. A rebound local
  (`let got = f(1); let got = f(2); assert_eq!(got, ..)`) resolves to the
  second call and is judged by that call's branch, instead of leaving the
  operand unresolved and keeping credit; a binding in a nested test block that
  closed before the assertion is no longer read. An owner `let` in a block that
  closes before the seam's `if` no longer counts as a parameter rebind, so it
  cannot keep credit for a call that takes the other branch (#14569).
