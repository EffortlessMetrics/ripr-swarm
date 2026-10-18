<!-- section: Fixed -->
- TypeScript committed-snapshot call counting also counts `toMatchSnapshot<T>(..)`, bracket-access and `.call`/`.apply`/`.bind` spellings, so an uncounted counter-advancing call can no longer leave a body credited as one snapshot (#8031).
