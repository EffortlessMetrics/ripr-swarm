<!-- section: Fixed -->
- `ripr check` no longer aborts when the user's git config sets a dangling
  `diff.orderFile`, and a live orderfile no longer reorders the diff ripr
  parses. The diff invocation pins `diff.orderFile=/dev/null`; an empty
  value is not an off switch (git fails reading orderfile `''`), so the
  null device is used on every platform (#5325).
