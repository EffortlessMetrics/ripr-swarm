<!-- section: Fixed -->
- An option typed before the command (`ripr -q check`) is no longer answered
  with a `ripr check -q ...` example that `check` rejects. The example appears
  only for a flag `check` accepts; a near spelling names the accepted flag
  (`ripr check --quiet`), and anything else says `ripr check` has no such
  option (#14125).
