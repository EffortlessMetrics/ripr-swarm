<!-- section: Fixed -->
- A Rust gap whose related `assert_eq!` was refused for a shape that can keep
  it from running (an `#[ignore]`d or cfg-off test, an `if` branch, an
  uncalled closure, a rebound `assert_eq`) no longer suggests the refusal
  might be a static limit; its next step says to make the check run on every
  default test run as the standard macro (#6903).
- When related tests pass only char or byte literals next to a numeric
  boundary (or the reverse), the infection note now names that mismatch
  instead of blaming strings, computed values or fixtures (#6902).
