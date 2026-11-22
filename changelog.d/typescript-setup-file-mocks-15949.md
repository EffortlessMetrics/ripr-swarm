<!-- section: Fixed -->
- TypeScript and JavaScript: an owner module mocked from a runner setup file (`setupFiles` or `setupFilesAfterEnv` in a Vitest or Jest config, including the `jest` key of `package.json`) no longer reads `exposed`. The setup file's mocks now count for every test under the config, like a mock written in the test file (#15949).
