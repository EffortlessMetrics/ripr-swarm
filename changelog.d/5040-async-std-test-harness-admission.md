<!-- section: Fixed -->
- Owner-return pins now admit `assert_eq!` and related exact-value assertions in `#[async_std::test]` async bodies when `async_std` is the extern crate in the test module, the attribute carries no runtime-changing arguments, and the assertion reads the owner through one `.await` (#5040).
