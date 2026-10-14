<!-- section: Fixed -->
- TypeScript: a changed line with more than one `throw` or `Promise.reject` site no longer reads a message from the first site, so a `toThrow` payload cannot falsely separate versions that a later site still throws (#7950).
