<!-- section: Fixed -->
- A change whose only calling test observes it through a custom assertion
  macro (`assert_flag!(bulk(100), true)`) no longer reads as an actionable gap
  when another test in the same file holds a weak oracle. ripr names the
  macro as a `rust_macro_wrapped_assertion_unresolved` limit instead (#7089).
