<!-- section: Fixed -->
- `ripr agent stub` on a changed line inside a `Display`, `Debug` or other
  `std::fmt` trait's `fmt` method formats the value
  (`std::format!("{subject}")`, `std::format!("{subject:?}")`, also in
  `#![no_std]` crates) and compares the text, instead of asking for a
  `Formatter` no stable test can build; a user trait that shares a `std::fmt`
  trait's name keeps the direct call. Return-value fill-ins name the inputs
  the test chooses and quote the changed code ("should return for `subject`
  (the changed code is `...`)") instead of the broken "should return when
  `<changed code>`" (#8361).
