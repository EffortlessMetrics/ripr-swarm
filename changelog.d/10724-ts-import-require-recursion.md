<!-- section: Fixed -->
- TypeScript: `ripr check` no longer overflows its stack on a file that binds its own `require` through an ES import (`import require from "./b"`); the import scan is bounded (#10724).
