<!-- section: Fixed -->
- The TypeScript committed-snapshot call counter also counts `toMatchSnapshot?.()`, `["toMatchSnapshot" ]()`, a block comment before `(`, and `toMatchSnapshot!()`, so those extra calls refuse the strong credit (#8031).
