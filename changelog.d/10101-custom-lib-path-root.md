<!-- section: Fixed -->
- A dependent crate importing a library whose manifest moves it with `[lib] path`
  is now related to a change in that library file, so a test that pins the changed
  value exposes it instead of reporting a false gap (RIPR-SPEC-0171 example 5, #10101).
