<!-- section: Fixed -->
- The TypeScript preview adapter now reads a changed field update through a
  literal numeric table loop (`for (const [calls, want] of rows)`) and through
  a same-file one-assertion helper (`checkWrites(stats, 2)`) as the assertion
  they hold, so the existing exact, pinned, nonzero rules apply to them; a
  table with no calling nonzero row, a multi-statement helper, a computed
  argument or a shadowed free name still read as before (#9534).
