<!-- section: Changed -->
- `ripr check --format json-terse` omits each finding's top-level `observed_values` and `missing_discriminators` when they repeat its `activation` block, and lists them in `dropped_aliases`; full `json` is unchanged (#13673).
