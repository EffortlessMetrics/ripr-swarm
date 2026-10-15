<!-- section: Fixed -->
- A test carried as an integer through a `From`/`Into` path call
  (`From::from(r == Err(E::Bad))`, `<u8 as From<bool>>::from(..)`,
  `Into::<u8>::into(..)`, `std::primitive::u8::from(..)`) compared with `0`,
  `1` or a variable no longer earns `exact_value`/strong credit; a turbofish
  call over a `matches!` (`foo::<T>(matches!(..)).is_ok()`) and a comparison
  inside a call's arguments under `|` or `&`
  (`flags | compute(|v| v == 1)`) now read as their plain spellings do
  (RIPR-SPEC-0231 rule 1; #8101).
