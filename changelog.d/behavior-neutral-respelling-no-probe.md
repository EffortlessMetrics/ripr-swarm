<!-- section: Changed -->
- A one-for-one line replacement that only writes out an elided lifetime
  (`Chunk` to `Chunk<'_>`) or moves a primitive integer constant from its
  deprecated module (`core::i32::MAX` to `i32::MAX`) no longer seeds a
  probe, so lint-cleanup commits stop naming a refactor as the top "Start
  here" finding (#8747). The same holds when the respelled statement is
  rewrapped across a different number of lines, and a respelling inside a
  multi-line string literal, or inside an invocation of a macro that renders
  its argument as text (`stringify!`, `dbg!`, `assert!`, `assert_matches!`, including
  continuation lines of a multi-line call), keeps its probe.
