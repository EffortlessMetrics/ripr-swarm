<!-- section: Fixed -->
- Resolve bounded literal `strip_prefix` option chains and local string lengths
  through the actual comparison and asserted call, retaining opaque-input and
  wrong-call refusals (#3295).
- Refuse stale initializer observations after typed or multiline shadows, using
  parsed binding patterns across Rust whitespace and comments (#3295).
