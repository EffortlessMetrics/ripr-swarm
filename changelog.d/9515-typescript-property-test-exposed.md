<!-- section: Fixed -->
- TypeScript findings no longer read `exposed` when the only exact assertions
  cannot tell the changed return value apart: a loop that passes the
  operator's identity (`scaled(1, u)` against `u` for `unit * qty`), or an
  idempotence property (`normalize(normalize(t))` against `normalize(t)`) that
  a constant return value satisfies. Both now read `weakly_exposed` (#9515,
  RIPR-SPEC-0248 rules 2 and 3).
