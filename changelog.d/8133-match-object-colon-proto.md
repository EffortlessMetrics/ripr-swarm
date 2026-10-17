<!-- section: Fixed -->
- TypeScript `toMatchObject({ __proto__: v })` no longer pins a returned
  `__proto__: v` field: the colon form sets the prototype rather than an
  own property, so that key is skipped on both the matcher argument and
  the owner literal (#8133).
