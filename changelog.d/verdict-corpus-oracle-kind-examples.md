<!-- section: Added -->
- Verdict corpus: 28 authored cases cover the RIPR-SPEC-0231 oracle-kind
  admission examples. The related test's oracle kind or strength differs
  from the spec on 18 of them. Most read `exact_value` or
  `exact_error_variant` / strong where the spec admits only a weak or smoke
  oracle. The line verdict hides most of these: 22 of 28 still score ideal.
  Measured verdict errors: a length pin reads `exposed` while a mutant
  passes, and 4 discriminated controls read as gaps. Corpus 2026-10-04.8,
  175 cases (#6638; the overclaims are tracked in #6640).
