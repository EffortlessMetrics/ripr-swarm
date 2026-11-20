<!-- section: Fixed -->
- `ripr check` no longer names an `if` around a changed side effect as never
  entered when the owner is a trait method (`from`, `fmt`, `eq`): calls such
  as `.into()` run it without naming it, so a test the reading cannot see
  may enter the guard (#15010).
