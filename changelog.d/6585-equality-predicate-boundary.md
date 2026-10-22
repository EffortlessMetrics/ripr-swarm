<!-- section: Fixed -->
- A Python equality predicate such as `if n == 10:` mutated from `n == 11` no
  longer reads `exposed` when every strong related call sits off both compared
  literals (`assert eq(20) == False`). The boundary gate handles `==` -> `==`
  only; `!=` and `is` keep today's verdict (#6585).
