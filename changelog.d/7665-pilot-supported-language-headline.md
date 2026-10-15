<!-- section: Changed -->
- `ripr pilot` on a repo with no Rust seams but a supported language now
  headlines that language's `ripr check` route (`analyze the Python diff:
  ripr check --root <root>`) instead of `Top recommendation: none`. The
  Rust-scan explanation stays secondary. JSON `language_routes` is unchanged
  (#7665).
