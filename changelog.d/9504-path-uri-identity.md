<!-- section: Fixed -->
- LSP path identity now has one shared owner (`output::path::path_identity_key`).
  On Unix a file named `a\b.rs` (URI `a%5Cb.rs`) no longer aliases `a/b.rs`,
  distinct non-UTF-8 paths no longer collapse to one lossy file URI, and the
  URI, hover and gap-artifact comparators share one ASCII case-fold rule.
  Windows separator behavior is unchanged (#9504).
