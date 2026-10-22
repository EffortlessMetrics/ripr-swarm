<!-- section: Fixed -->
- LSP path identity now has one shared owner (`output::path::path_identity_key`).
  On Unix a file named `a\b.rs` (URI `a%5Cb.rs`) no longer aliases `a/b.rs`;
  distinct non-UTF-8 paths no longer collapse to one lossy file URI and now
  round-trip through `file_uri_for_path` / `path_from_file_uri`; the LSP input
  fingerprint, unavailable-adapter path membership, root-relative components,
  URI matching, hover and gap-artifact root comparison all use the same rule
  (#9504).
- Behavior changes to note: the gap-artifact root fold is now ASCII-only (the
  Windows-only Unicode fold is dropped) and applies on macOS through
  `host_paths_case_insensitive`; hover no longer rewrites `\` on Unix and uses
  the same ASCII fold instead of Unicode `to_lowercase`; a Windows-produced
  report root spelled `sub\proj` no longer equals `<root>/sub/proj` on Unix
  (it fails closed as a wrong root). Windows separator handling is unchanged.
  The edit-cage case-collision key is ASCII-only, so non-ASCII case variants (for example `É`/`é`) are no longer flagged ambiguous, including on macOS. Baseline keys
  (`output/baseline*.rs`) are intentionally unchanged (stored-string
  compatibility; tracked in a follow-up issue) (#9504).
