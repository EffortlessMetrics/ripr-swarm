<!-- section: Internal / CI -->
- RIPR-SPEC-0197's pinned-literal rule for `matches!` (#12716) now has its own integration rows: a literal `matches!` pattern equal to the owner's old or new value stays credited, one equal to neither (single-line, multi-line, guard exit, `Ok(..)` and `Some(..)`) is withheld, and an expression argument keeps today's reading. No product behavior changes.
