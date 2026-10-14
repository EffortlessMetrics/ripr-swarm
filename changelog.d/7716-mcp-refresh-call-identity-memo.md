<!-- section: Changed -->
- The same-named call identity is now established once per analysis run and
  shared by every consumer (the related-test relation, the owner-return pin
  and the dependent-scope pin-scope check) instead of being rebuilt per
  probe, per pin and per scope check — each rebuild repeating a whole-index
  crate-layout scan and re-parsing every candidate test. On the pinned
  20-package `mid` corpus the related-test share of an MCP `ripr_refresh`
  cold refresh drops from ~28 ms per probe pair to near zero, with
  identical classification answers: the shared identity is immutable after
  construction and the index's sources are fixed once built (#7716).
