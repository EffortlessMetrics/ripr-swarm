<!-- section: Fixed -->
- TypeScript monorepos: a test resolves `paths` aliases through its own package's `tsconfig.json` instead of the workspace-root map, so package-level aliases credit the right owner and no longer read as an unresolved alias (#12754).
