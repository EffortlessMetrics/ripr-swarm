<!-- section: Fixed -->
- `ripr check` no longer drops every Rust finding ("static limit
  adapter_panicked") on a file with a same-line attribute before an inline
  module whose name has a non-ASCII char, such as
  `#[cfg(test)] mod prüfung {`; the module brace offset was measured from the
  text after the attribute and used to slice the whole line (#14838).
