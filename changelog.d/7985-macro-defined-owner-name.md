<!-- section: Fixed -->
- A bare call of the owner's name in a Rust test file no longer earns match-arm
  selection credit when a `macro_rules!` invocation in that file may define an
  item of the same name; the call reads as ambiguous, as it does for a foreign
  import. Standard expression macros and macro bodies that only call the owner
  do not shadow it (#7985).
