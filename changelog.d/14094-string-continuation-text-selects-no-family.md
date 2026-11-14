<!-- section: Fixed -->
- Rust checks: text on a continuation line of a multi-line string literal that
  reads like code (`if x > 3 { return Err(e) }` in a help message) no longer
  seeds predicate, error-path, return-value, call or field probes (#14094).
