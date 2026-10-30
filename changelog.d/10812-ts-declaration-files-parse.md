<!-- section: Fixed -->
- TypeScript analysis now parses `*.d.ts`, `*.d.mts` and `*.d.cts` as declaration files, so valid ambient declarations such as `export const x: T;` no longer raise a parse error that marks the whole run partial (#10812).
