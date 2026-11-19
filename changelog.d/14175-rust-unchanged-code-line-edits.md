<!-- section: Fixed -->
- Rust: a changed line whose code is unchanged no longer gets a finding.
  Adding, removing or rewording a trailing `//` comment, rewriting a file's
  line endings, and adding or removing the final newline used to report the
  touched lines as changed behavior, including an actionable missing-boundary
  finding. A code change on the same line, and a `//` that is string or block
  comment content, keep their findings (#14175).
