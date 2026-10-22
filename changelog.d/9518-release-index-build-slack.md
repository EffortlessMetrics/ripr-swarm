<!-- section: Changed -->
- A finished Rust fact index gives back the spare capacity its builders left on
  fact vectors and strings. On this repository's cold index the live heap
  measured 427.6 MB before and 374.3 MB after (counting allocator, one build);
  peak process memory of a cold pilot was not measurably different in one run
  (#5363).
