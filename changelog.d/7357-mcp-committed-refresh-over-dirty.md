<!-- section: Changed -->
- MCP `ripr_refresh` over a committed diff already withheld repair cards when a
  gitignored non-generated Rust file (including a vendored Rust tree) exists,
  because the findings producer has no captured observation of it. The docs
  claimed gitignored Rust files were not claimed; they now say so and a test
  pins the refusal. Gitignored non-Rust files still keep the card. The remedy
  text now offers `git add -f` plus a commit, or removing the file (#7357).
