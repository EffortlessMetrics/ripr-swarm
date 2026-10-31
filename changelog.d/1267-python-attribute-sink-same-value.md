<!-- section: Fixed -->
- Python changed-sink credit no longer treats an assertion on a different
  receiver as observing the changed attribute sink when only the asserted
  value and attribute name coincide, and nested Python functions now get
  qualified owners instead of attributing their changes to the outer
  function (#1267).
