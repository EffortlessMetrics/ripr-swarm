<!-- section: Fixed -->
- Repo predicate-boundary seams whose `if` arms only run unit-returning calls
  now bind to the side-effect sink instead of crediting unrelated return-value
  assertions as `strongly_gripped` (#5384).
