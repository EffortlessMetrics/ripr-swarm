<!-- section: Fixed -->
- TypeScript/JavaScript preview: a changed tool configuration script
  (`jest.config.ts`, `vitest.config.*`, `eslint.config.js`, `.eslintrc.cjs`,
  `karma.conf.js` and the like) is no longer reported as product code with no
  test path. A runner or bundler loads these files and no test imports them;
  on vercel/ms one `jest.config.ts` edit produced nine such findings. Diff and
  repo mode refuse them as finding subjects, repo mode counts them as skipped,
  and near-miss names such as `src/config.ts` and `app.config.ts` stay
  ordinary source. A config that carries tested logic opts back in with
  `[languages.typescript] handwritten_files` (#15947).
