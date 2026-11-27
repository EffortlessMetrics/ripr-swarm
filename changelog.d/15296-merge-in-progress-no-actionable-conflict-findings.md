<!-- section: Fixed -->
- A merge or rebase in progress no longer yields findings with repair next steps
  from a conflicted Rust file: the conflict region stays quarantined when the
  live zero-context diff splits it across hunks, so `ripr check` lists 0
  findings there (exit 2, as before) like Python, and `ripr doctor` warns about the
  interrupted operation with the finish-or-abort next step (#15296).
