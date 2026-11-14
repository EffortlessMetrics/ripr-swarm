<!-- section: Changed -->
- `ripr check --format json-terse` also codes repeated `related_tests[].oracle`
  and `related_tests[].why` strings through `evidence_legend`. An oracle line
  shares the code of the identical `assertion_texts` value. On the five-gap
  budget fixture the terse document drops from 43,565 to 40,612 bytes, and its
  ceiling is lowered from 45,000 to 42,500 (#13673).
