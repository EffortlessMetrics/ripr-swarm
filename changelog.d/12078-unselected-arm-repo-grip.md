<!-- section: Fixed -->
- The repo seam view (`ripr agent brief`, `ripr repair`, repo exposure) no
  longer grips a changed `match` arm strongly when every related test calls
  the owner with an input that selects another arm. It now reads the same
  arm selection `ripr check` uses: activation is weak and the arm's pattern
  is the missing input, so `agent brief` lists the arm instead of hiding it
  as already tested (#12078).
- A changed `match` arm in a trait method (`from`, `fmt`, `eq`) is not
  named as unselected, in either view, since calls such as `.into()` run it
  without naming it (#12078).
