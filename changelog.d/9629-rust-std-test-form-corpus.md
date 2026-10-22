<!-- section: Changed -->
- The Rust verdict corpus gains `authored-idiom-kit` with 21 runtime-labeled
  cases for standard-library test forms (`#[should_panic(expected)]`,
  `Result`-returning tests, `catch_unwind`, hand-written mocks and recorders,
  seeded property loops, table-driven loops, assertion helpers and macros,
  `Display` string pins, doctests, `tests/` integration tests, out-of-line test
  modules and fixture builders) and a new `std_test_form` coverage axis with a
  floor of 21. Four cases read as false actionable and are listed in the
  `[[known_false]]` ledger with owning issues (#7089, #9608, #9609, #9610).
