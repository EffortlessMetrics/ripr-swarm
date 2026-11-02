<!-- section: Fixed -->
- Array `return_value` findings demote to `weakly_exposed` when constructing
  the owner's single-array return may exit before the test reaches its
  assertion, matching the indexed sibling exit refusal (#8084).
