<!-- section: Fixed -->

- A one-field integer tuple struct owner (`Self(other.0 + self.0)`) is no longer credited `exposed` when the test's literal operands (`Money(0).add(Money(0))`) make the old and new tail return the same value; it reads `weakly_exposed` (#12808).
