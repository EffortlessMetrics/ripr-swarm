<!-- section: Changed -->
- Evidence value facts intern identifier and literal strings once per test
  file, so tests in the same file share one copy of a let-binding instead of
  allocating it per test (#9494).
