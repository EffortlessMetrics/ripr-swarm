<!-- section: Changed -->
- A single `ripr check` diff run now computes the external-docs scan, same-file
  test-macro expansion and helper-oracle extraction once per unchanged source
  text instead of once per overlapping index, so a diff over a large workspace
  does less repeated parsing. Results are unchanged; nothing is kept between
  runs (#5363).
