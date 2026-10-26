<!-- section: Fixed -->
- A Rust test that passes a local to the changed method as `&mut` and then
  asserts on that same local now counts as observing the method; any
  rebinding, extra write, second pass or assertion before the call still does
  not (#8999).
