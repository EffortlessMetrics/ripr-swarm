<!-- section: Fixed -->
- A TypeScript test registered through a renamed runner (`import { test as t } from 'vitest'`) no longer reads `exposed` when none of its calls can enter the changed `if` arm. It now reads `reachable_unrevealed`, like the same test written with `test(...)`. A nested call through the alias inside the test still keeps the earlier verdict (#16435).
