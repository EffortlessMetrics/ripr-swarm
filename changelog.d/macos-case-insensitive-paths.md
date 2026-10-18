<!-- section: Changed -->
- Case-insensitive path identity (LSP URI matching, root containment, named-path
  membership, repository-relative rendering) now follows one shared host
  predicate that covers desktop Windows and macOS hosts only (macOS assumes
  the default case-insensitive APFS volume); Linux stays case-sensitive (#8391).
