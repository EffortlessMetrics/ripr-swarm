<!-- section: Fixed -->
- MCP `ripr_refresh` over a committed diff withholds repair cards when a
  gitignored non-generated Rust file (including a vendored Rust tree) exists,
  because the findings producer has no captured observation of it; the earlier
  note that gitignored Rust files are not claimed was wrong. Gitignored non-Rust
  files still keep the card. The remedy is `git add -f` plus a commit, or
  removing the file (#7357).
