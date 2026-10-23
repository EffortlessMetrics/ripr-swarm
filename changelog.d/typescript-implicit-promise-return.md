<!-- section: Fixed -->
- TypeScript and JavaScript preview analysis retains the supported assertion
  credit of promises implicitly returned by expression-bodied test arrows,
  while discarded callback promises keep their existing limits. Inline
  receiver shadows cannot supply that credit; bindings in independent nested
  scopes do not suppress a supported assertion. Function-scoped `var` shadows
  are refused even when declared inside a nested callback block (#6798).
