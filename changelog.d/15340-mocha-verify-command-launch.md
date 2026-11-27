<!-- section: Fixed -->
- The TypeScript repair packet's `verify_command` for a Mocha project is now
  `<launch> mocha <file>` (for example `npx --no-install mocha test/age.spec.js`)
  instead of `npm test -- <file>`, and the `npm test`, `pnpm test` and
  `yarn test` fallbacks are no longer emitted when `scripts.test` is the
  `npm init` placeholder `echo "Error: no test specified" && exit 1`, which
  always failed (#15340).
