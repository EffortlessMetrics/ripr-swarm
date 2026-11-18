<!-- section: Fixed -->
- Rust: a deleted or added unary `-` on a returned parameter no longer reads
  `reachable_unrevealed` when the test's only owner call may not run or is
  not a call: the right operand of `&&`/`||` (`false && f(5) == 5`), the input
  of a macro that does not evaluate it (`stringify!(f(5))`), or a call after a
  statement that always diverges on a constant (`let Some(x) = None else {
  return; };`, a literal `Err(..)?` or `None?`). Statements after such a
  constant divergence are also no longer credited as oracles (RIPR-SPEC-0231
  rule 8). `let _ = f(5);`, `true && f(5) == 5` and `vec![f(5)]` still read
  `reachable_unrevealed` (#14968).
