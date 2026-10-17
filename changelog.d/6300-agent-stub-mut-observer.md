<!-- section: Fixed -->
- `ripr agent stub` no longer emits a vacuous `assert_eq!(actual, Ok(()))` for owners that return `()` or `Result<(), _>`: it asserts the `&mut` input (or `&mut self`) after the call, and refuses with `ObserverRequired` when the owner offers none (https://github.com/EffortlessMetrics/ripr/issues/6300).
