<!-- section: Fixed -->
- Rust: when a change swaps the `Err` variant a function returns, a test whose
  only assertion pins the `Ok` arm (`assert_eq!(f(x), Ok(..))` or
  `f(x).unwrap()`) is now reported as not observing the change instead of
  "no assertion pins the variant". The agent `tK=` row and the context fix site
  now name the test that does reach the error, such as an
  `assert!(f(x).is_err())` test to tighten, rather than the strong `Ok` test
  (#14454).
