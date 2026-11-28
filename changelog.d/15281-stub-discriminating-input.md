<!-- section: Fixed -->
- `ripr agent stub` fill-in guidance now names the inputs that tell the changed predicate from its neighbor. For `a && b` or `a || b` it names "exactly one of `a` and `b` is true". For a boundary over receiver state, such as `self.len > MAX`, it names `subject.len == MAX`. Following the stub literally no longer produces a test that passes under both spellings (#15281).
