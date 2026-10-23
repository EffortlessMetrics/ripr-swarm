<!-- section: Fixed -->
- TypeScript and JavaScript preview analysis retains the supported assertion
  credit of promises implicitly returned by expression-bodied test arrows,
  while discarded callback promises keep their existing limits. Inline
  receiver shadows cannot supply that credit; bindings in independent nested
  scopes do not suppress a supported assertion (#6798).
