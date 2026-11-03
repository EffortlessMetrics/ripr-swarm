<!-- section: Fixed -->
- TypeScript root `tsconfig.json` / `jsconfig.json` path aliases no longer resolve for an importer
  whose nearest config declares its own `compilerOptions.paths` (with or without `baseUrl`):
  the root map is withheld there instead of crediting the wrong owner, and the existing
  `typescript_path_alias_unresolved` disclosure covers the miss. Root fallback is unchanged when
  no nearer config declares `paths` (#12754).
