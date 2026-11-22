<!-- section: Fixed -->
- Rust: a test that passes identically on the changed code and its baseline no
  longer reads `exposed` in three more shapes: a method called on a record
  literal whose fields hide the change (`Acc { base: 0 }.double()` after
  `self.base * 2` became `* 3`), equal arguments to swapped `if` arms
  (`largest(2, 2)`), and a tuple both versions build alike
  (`pair(5) == (5, true)` after `a > 0` became `a >= 0`). Each reads
  `weakly_exposed` and names the agreeing call (#15272)
