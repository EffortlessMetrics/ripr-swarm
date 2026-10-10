<!-- section: Fixed -->
- A change whose only test is written through a same-file `macro_rules!`
  generator (a macro whose body emits `#[test]`, as in httparse's `req!`
  tables) no longer reads as an actionable gap. ripr names the generated
  test and the generator as a `rust_macro_reach_unresolved` witness
  instead of reporting a missing test (#6649).
