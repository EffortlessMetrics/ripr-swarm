<!-- section: Internal / CI -->
- the LSP no-stderr source guard now scans the production region of every LSP
  source file instead of stopping at its first `#[cfg(test)]` declaration, so
  an `eprintln!` in `serve_stdio` or any other production function fails the
  guard; only `#[cfg(test)]`-gated items are excluded (#14480).
