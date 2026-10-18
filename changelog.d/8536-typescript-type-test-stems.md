<!-- section: Fixed -->
- TypeScript preview now treats tsd/vitest type-test files (`*.test-d.ts`,
  `*.spec-d.ts`) as tests, so an edit there is counted as a changed test
  file instead of a false `no_static_path` production finding. Exact stem
  suffixes only: plain `*.d.ts` declarations and production names such as
  `foo.test-data.ts` stay non-test (#8536).
