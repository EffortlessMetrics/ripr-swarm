<!-- section: Fixed -->
- TypeScript preview recognizes `Deno.test(...)` registrations (name and
  function, an options object, a named function, `t.step` bodies; `ignore`
  skips) and the `@std/assert` / `jsr:@std/assert` / `deno.land/std`
  `assertEquals`, `assertStrictEquals`, `assert`, `assertThrows` and
  `assertRejects` assertions, so a tested Deno owner no longer reads
  `no_static_path` (#15622).
