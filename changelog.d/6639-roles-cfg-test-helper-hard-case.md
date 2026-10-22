<!-- section: Internal / CI -->
- The Rust verdict corpus `roles-cfg-test-helper-input` hard-case text now agrees
  with its label: `credited` stays the ideal, and `silent` is acceptable
  because ripr seeds no probe in a `#[cfg(test)]` helper (#6639).
