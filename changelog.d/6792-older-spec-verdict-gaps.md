<!-- section: Fixed -->
- A builder default that the test overrides through a setter before asserting
  no longer credits the constructor's `return_value` as `exposed` (#6613).
- An exact pin reached only through a wrapper whose body passes the owner to a
  macro now reads `propagation_unknown` instead of an actionable gap (#6614).
- A trailing `_` match arm pinned by an exact string input that matches no
  sibling literal is now credited as observed (#6616).
- A boundary predicate whose test inputs come from call expressions now reads
  `infection_unknown` instead of an actionable gap (#6615).
