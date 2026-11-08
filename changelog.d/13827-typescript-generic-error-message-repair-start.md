<!-- section: Fixed -->
- `ripr agent repair` now starts a TypeScript repair for a bare `toThrow()`
  test on a changed `throw new Error('<message>')`: the packet asks to pin the
  literal message (`toThrow('<message>')`), since `toThrow(Error)` could not
  tell the change apart. Messages that would need escaping still get no
  packet. The nightly TypeScript repair-start floor rises to 8 (#13827).
