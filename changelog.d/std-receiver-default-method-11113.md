<!-- section: Fixed -->
- A changed trait default method (`Flags::contains`) is no longer related with
  `direct_owner_call` to a test whose only spelling of the method name is a
  call on a local the test itself types as a standard collection or string
  (`all_named.contains(x)` on a `Vec`). The relation is now name-only, so the
  finding no longer sends the user to harden an unrelated test's assertion. A
  receiver ripr cannot type, a mixed body, a path or UFCS spelling, a rebound
  std type name and an indexed impl of the trait for that type keep the old
  credit (#11113). Crediting `Type::method` passed as a function argument is
  tracked separately.
