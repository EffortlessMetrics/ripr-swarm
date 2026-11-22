<!-- section: Fixed -->
- TypeScript preview: a solution-style `tsconfig.json` (`"files": []` plus
  `references`, the Vite template) no longer disables alias resolution. The
  `paths` of each referenced config in the same directory, such as
  `tsconfig.app.json`, are read as that project's own, so a test importing
  `@/pricing` is related to `src/pricing.ts`. When two referenced projects
  map one alias to different files, or a reference cannot be read, the import
  stays unresolved and disclosed (#16003).
