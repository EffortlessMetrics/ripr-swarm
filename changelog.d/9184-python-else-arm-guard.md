<!-- section: Fixed -->
- Python return-value changes inside the `else:` arm of an `if`/`elif` chain no
  longer read `exposed` when every strong related test call satisfies a chain
  condition and so never enters the arm. The finding reports `weakly_exposed`
  naming the missing discriminator `not (X)`; calls that enter the arm,
  unreadable conditions and `for`/`while`/`try` `else` clauses keep the
  existing verdict (#9184).
