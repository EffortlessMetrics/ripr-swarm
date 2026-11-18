<!-- section: Internal / CI -->
- the LSP no-stderr source guard now scans the production region of every LSP
  source file instead of stopping at its first `#[cfg(test)]` declaration, so
  an `eprintln!` in `serve_stdio` or any other production function fails the
  guard; the excluded ranges come from the parser, so a `#[cfg(test)]` module
  holding a char literal such as `.find('{')` no longer hides the production
  code after it, a char literal such as `'"'` no longer flips the quote parity
  of the comment-and-literal blinder and masks a real call, and a file that does
  not parse fails the guard instead of passing it (#14480).
