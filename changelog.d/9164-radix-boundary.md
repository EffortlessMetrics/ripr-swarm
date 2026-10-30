<!-- section: Fixed -->
- Hex, binary and octal integer literals (`0xFF`, `0b1010`, `0o17`) are read as
  one input value instead of `0`, so a test input far from a `0xFF` boundary no
  longer reads as the boundary and the finding is `weakly_exposed` rather than
  `exposed` (#9164).
