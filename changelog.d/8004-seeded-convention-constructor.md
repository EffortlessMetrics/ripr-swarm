<!-- section: Changed -->
- Rust receiver matching also reads `Type::seeded()` as a value of `Type`:
  the state-participle constructor joins `new`/`default`/`from` in the
  bounded admission, so a test typing its receiver with `let mut ledger =
  Ledger::seeded()` credits the `Ledger` impl's changed method (the
  RIPR-SPEC-0228 fixtures keep their pinned `exposed` credit). Other names,
  including the `new_*`/`from_*`/`with_*` factories removed in #8782, still
  fail closed (#8004).
