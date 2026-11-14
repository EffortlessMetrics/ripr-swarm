<!-- section: Changed -->
- Human and editor text says "check" where it said "discriminator". The
  boundary reason now reads `Related tests contain input values, but the
  equality-boundary check is missing`, the fallback reads `static evidence
  names no concrete missing check`, and missing-value entries read `Missing
  check value: <value>` instead of `Missing discriminator value: <value>`.
  The same strings appear in `ripr check --format json` and in LSP hover, so
  a script or test that matches the old wording needs the new one. Field
  names such as `missing_discriminators` are unchanged (#12963, #12992).
