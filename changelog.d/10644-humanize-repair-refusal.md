<!-- section: Fixed -->

- `ripr check`, `ripr repair`, and `ripr agent card` no longer print the
  internal readiness code `producer-owned missing discriminator fact: <kind>`
  when a repair route is not ready; the refusal now names the missing evidence
  in plain words (for example, "a test assertion that pins the changed
  boundary value exactly"). Machine-readable readiness fields and JSON output
  keep the stable internal strings. (#10644)
