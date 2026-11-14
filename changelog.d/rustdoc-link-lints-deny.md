<!-- section: Internal / CI -->
- `cargo doc --workspace --no-deps` is warning-free again (three unresolved or
  private intra-doc links in `ripr-contract`), and rustdoc's default-warn
  lints are now `deny` in `[workspace.lints.rustdoc]`, so a broken doc link
  fails the `check-pr` doc gate instead of printing a warning (#14244).
