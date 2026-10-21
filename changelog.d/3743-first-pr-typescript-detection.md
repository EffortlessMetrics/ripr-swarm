<!-- section: Changed -->
- First-use `first-pr` guidance now detects a TypeScript project with the same rules as the no-config adapter default (root `tsconfig.json`/`jsconfig.json` or detectable TypeScript source below `src`/`tests`; `package.json` alone, generated names and declaration files no longer count), so its route agrees with what `ripr check` enables (#3743).
