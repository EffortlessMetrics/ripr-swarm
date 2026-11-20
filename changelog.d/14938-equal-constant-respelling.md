<!-- section: Fixed -->
- A returned constant respelled as the same value no longer reads
  `exposed`. Examples: `65` as `0x41`, `-128` as `-0x80`, `"A"` as `"\x41"`
  or `r#"A"#`, `'A'` as `'\u{41}'`, `1.0` as `1e0`, `[1, 2]` as `[0x1, 0b10]`.
  No test can tell the two spellings apart, so the line now seeds no
  finding on either side of the diff. The value comparison uses the same
  constant folder as returned-constant changes. A changed value, a changed
  literal suffix (`1` to `1i8`, which can change the inferred type), a
  different literal kind (`b'A'` to `65`) and anything the folder does not
  read (`vec!`, names, calls, a line with a trailing comment) keep their
  finding (#14938).
