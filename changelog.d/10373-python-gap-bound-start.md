<!-- section: Added -->
- A Python finding with a complete repair packet now gets the same gap-bound
  repair start TypeScript has: `ripr check` prints
  `ripr agent repair --gap-id <gap> --base <base> --phase before`, the before
  phase accepts it, and `ripr agent repair --phase after` closes the gap and
  writes the receipt as it does for TypeScript. `ripr repair` and `agent repair --seam-id` on a
  Python gap still explain that seams are Rust-only and now point at the
  gap-bound start, and `ripr agent status` after a pilot names it (#10373).
