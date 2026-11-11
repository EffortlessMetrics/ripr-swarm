<!-- section: Added -->
- `ripr repair` names the exact native command that runs the suggested Rust
  test (`Run the new test: cargo test --manifest-path Cargo.toml --test
  pricing NAME`), resolved the way `ripr agent stub` resolves its run command;
  no line is printed when the target cannot be named exactly, and `--json`
  output is unchanged (#14320).
