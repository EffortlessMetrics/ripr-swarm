<!-- section: Fixed -->
- Rust diff checks: a deleted or added `self.helper(..)` call no longer reads
  `exposed` just because a related test compares some other whole object.
  When ripr can bound the fields `helper` writes (one `&mut self` method of
  the same type, no return value, and only `self.<field>` state reached
  through resolvable self calls), a whole-object `assert_eq!` confirms the
  effect only when it names a written field, calls a method that reads one,
  or compares a value that may hold the receiver. `assert_eq!(inv.history(),
  ..)` and `assert_eq!(receipt, Receipt { .. })` no longer confirm a call that
  only updates `low_stock`; `assert_eq!(inv.low_skus(), ..)` still does, and
  so does any whole-object equality in a test that first calls a `&mut self`
  method reading `low_stock` (such as `inv.reorder()`). When
  the written state cannot be bounded, any whole-object equality still
  confirms, and mock and snapshot observers are unchanged (RIPR-SPEC-0094,
  Part D) (#6628).
