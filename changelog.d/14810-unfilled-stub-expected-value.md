<!-- section: Fixed -->
- An assertion compared with a local the test body binds only to an unfilled
  placeholder macro (`let expected: i32 = todo!("ripr: write the value ...")`,
  the `ripr agent stub --write` template) no longer earns strong exact-value
  credit, so `ripr check` after writing a stub reports the gap instead of
  "no test gap found"; a filled or computed expected value keeps its credit
  (#14810).
