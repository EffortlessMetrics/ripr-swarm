<!-- section: Fixed -->
- Rust analysis no longer reads `Type::new_*`, `Type::from_*` or `Type::with_*`
  as a value of `Type` when matching a receiver to a changed method: the
  prefix is a naming convention, not a return type, so `Site::with_cache()`
  returning a `Cache` cannot credit `Site::build` (#8782).
