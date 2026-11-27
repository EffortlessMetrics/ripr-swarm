<!-- section: Fixed -->
- In a Rust test file the parser rejects, an expect-test comparison is now
  graded `snapshot` / medium only when the statement parses as
  `assert_eq`, `assert_debug_eq` or `assert_json_eq` on its bracketed
  `expect!`/`expect_file!` receiver outside a closure, so single-bracket and
  spaced spellings, string arguments and a method rustfmt wraps onto the next
  line are read like the canonical form, while a comparison on another
  receiver or one inside a block comment or multi-line string earns no
  snapshot credit (#7878).
