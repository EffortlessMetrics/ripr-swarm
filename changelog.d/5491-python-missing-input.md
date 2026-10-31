<!-- section: Added -->
- Python related-test rows on a weakly exposed predicate finding now carry
  `missing_input` when a strong row's own direct owner calls never reach the
  changed boundary: every compared operand resolves off the boundary, the
  owner has no recursion, helper or decorator, and no implicit setup, conftest
  fixture or hook, helper method or other indirect call could also reach it.
  It is never claimed for an instance method. It reads visible evidence only:
  an autouse fixture imported by name, import side effects, and plugins from
  ini, `addopts` or `sitecustomize` stay invisible. The miss is explanation
  only: class, stages, confidence, ordering and repair guidance are unchanged
  (#5491).
