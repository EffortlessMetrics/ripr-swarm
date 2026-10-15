<!-- section: Fixed -->
- Rust owner-return pins no longer credit an equality whose expected value is
  a `const` or `static` (including `thread_local!` and `lazy_static!` items)
  whose initializer calls the changed function, directly or through another
  constant (`const EXPECTED: u32 = quota(2);
  assert_eq!(quota(2), EXPECTED)`); the assertion reads as weak, as a `let`
  binding of the same call already did. The reveal side applies the same
  rule to `let` bindings and constants, so a changed token shared by the
  owner call's arguments no longer restores a strong oracle (#8077).
