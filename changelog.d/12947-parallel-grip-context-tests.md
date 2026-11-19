<!-- section: Changed -->
- Repository evidence builds each test's related-test context facts in
  parallel chunks, then fills the shared lookup tables in test order, so a
  cold check of a large workspace spends less time preparing evidence with
  identical output (#12947).
