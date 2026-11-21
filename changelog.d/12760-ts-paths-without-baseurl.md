<!-- section: Fixed -->
- TypeScript preview: `compilerOptions.paths` now resolves without a
  `baseUrl`, the create-next-app default
  (`"paths": { "@/*": ["./src/*"] }`). Targets resolve against the config
  that declares `paths`, as `tsc` does, so a test importing `@/pricing` is
  related to `src/pricing.ts` instead of reading `no_static_path`. A target
  without a leading `./` stays unresolved there, as `tsc` rejects it, and the
  alias advice no longer asks for a `baseUrl` (#12760).
