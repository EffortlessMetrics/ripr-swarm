<!-- section: Added -->
- Python related-test rows on a weakly exposed finding now carry a typed `miss`
  and `why` in JSON, human, LSP and explain output: `weak_assertion` for a
  weak or smoke assertion, `missing_input` when a strong row's own calls never
  reach the changed predicate boundary (direct calls only; withheld whenever implicit setup, a conftest fixture or hook, or a decorated owner could also reach the owner), and `observation_unconfirmed` when a
  strong assertion is not credited as observing the changed sink. `no_call_path`
  follows separately. `no_assertion`,
  `assertion_not_observing`, `assertion_not_credited` and
  `missing_exact_assertion` stay unset for Python until their evidence is
  row-owned. `missing_input` reads visible evidence only: an autouse fixture imported by name, import side effects, and plugins from ini, `addopts` or `sitecustomize` stay invisible. The miss is explanation only: class, stages, confidence, ordering
  and repair guidance are unchanged (#5491).
