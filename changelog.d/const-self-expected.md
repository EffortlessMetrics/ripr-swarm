<!-- section: Fixed -->
- Rust owner-return pins no longer credit an equality whose expected value is
  a `const` or `static` initialized through the changed function, directly or
  through another constant (`const EXPECTED: u32 = quota(2);
  assert_eq!(quota(2), EXPECTED)`); the assertion reads as weak, as a `let`
  binding of the same call already did (#PR).
