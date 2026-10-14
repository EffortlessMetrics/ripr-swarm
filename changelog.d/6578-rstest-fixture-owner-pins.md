<!-- section: Fixed -->
- Owner-return pins now credit an `assert_eq!` in an `#[rstest]` test whose
  receiver or argument comes from an injected `#[fixture]`
  (`fn fee(account: Account) { assert_eq!(account.monthly_fee(), 5); }`).
  A fixture receiver is typed by its parameter annotation. A fixture never
  supplies the expected value, directly or through a check helper, since
  the fixture body may call the owner;
  generic tests and parameters with `#[from]`, `#[with]`, `#[future]`,
  `#[values]` or other attributes still refuse (#6578).
