<!-- section: Fixed -->
- TypeScript preview now resolves tsconfig/jsconfig `paths` aliases by default (`[typescript] resolve_tsconfig_paths` defaults to `true`), so tests that import an owner through `@/…` are related instead of reading as a false `no_static_path`. Set `resolve_tsconfig_paths = false` to keep aliased imports unresolved (RIPR-SPEC-0099 amendment, #8346).
