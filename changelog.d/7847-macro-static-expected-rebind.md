<!-- section: Fixed -->
- An owner-return pin no longer credits an expected value that a statement macro binds or that the test writes through a static (`EXPECTED.store(f(4))`), which compared the owner with itself (#7847).
- The same guard now also refuses an expected value the test writes through a field, tuple field or index (`expected.v = f(4)`, `expected[i] = f(4)`), a wider set of mutating methods (`fill`, `sort`, `drain`, `retain`, `*_assign`, ...), and a mutator call in a struct-literal field value; the `let` type-annotation skip is scoped to a `let` pattern (#7847).
