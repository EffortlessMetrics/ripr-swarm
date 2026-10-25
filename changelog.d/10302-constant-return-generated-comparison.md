<!-- section: Fixed -->
- A `#[test_case(x => y)]` test that calls the changed owner but returns a
  literal (`let _ = weight(x); 12`) no longer carries
  `rust_macro_wrapped_assertion_unresolved`: its generated comparison cannot
  notice the owner, so the missing discriminator stays a gap instead of
  reading as an unreadable comparison (#10302). A return that may carry the
  owner's value, or any `return`, `?`, macro or closure in the body, keeps the
  limit.
