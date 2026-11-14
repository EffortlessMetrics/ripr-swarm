<!-- section: Changed -->
- TypeScript checks: each import specifier is resolved once per importing file per run instead of once per changed owner; a 23-file check on `colinhacks/zod` uses about 18% less CPU with identical findings (#14364).
