<!-- section: Fixed -->
- Python nested functions are reached through a call to their enclosing
  function only when that function's own body calls the nested def, so a
  test of a function that merely defines a nested helper, calls it only from
  an unused sibling def, or rebinds its name no longer credits the helper's
  changed body (#12141).
