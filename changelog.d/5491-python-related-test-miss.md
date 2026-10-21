<!-- section: Added -->
- Python related-test rows on a weakly exposed finding now carry a typed `miss`
  and `why` in JSON, human, LSP and explain output: `weak_assertion` for a
  weak or smoke assertion and `observation_unconfirmed` when a strong
  assertion is not credited as observing the changed sink. `no_call_path` and
  `missing_input` follow separately. `no_assertion`,
  `assertion_not_observing`, `assertion_not_credited` and
  `missing_exact_assertion` stay unset for Python until their evidence is
  row-owned. The miss is explanation only: class, stages, confidence, ordering
  and repair guidance are unchanged (#5491).
