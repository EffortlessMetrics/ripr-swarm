<!-- section: Fixed -->
- TypeScript: a loose `assert.equal(f(x), 1)` (or `0`, or `deepEqual`) now reads as an exact assertion when `f` declares its return type as `number` and the assertion checks the whole result, because `==` between two numbers is strict. It was reported as weakly exposed although the test discriminates the change (#6654).
