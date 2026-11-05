<!-- section: Added -->
- Rust index now records parser-backed `include_str!` / `include_bytes!` resource
  dependency edges (exact or opaque) as an internal producer slice; no public
  output changes (#5171, advances; known limit #13046).
