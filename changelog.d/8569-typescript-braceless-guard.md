<!-- section: Fixed -->
- A TypeScript changed line that is the one-statement body of a braceless `if (c)` / `else if (c)` arm now reads `weakly_exposed` when every related test call provably leaves the guard untaken, instead of `exposed`. Loop heads, nested braceless `if`s, continued conditions and any unreadable guard keep the existing verdict (#8569).
