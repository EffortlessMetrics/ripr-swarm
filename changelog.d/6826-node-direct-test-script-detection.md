<!-- section: Fixed -->
- TypeScript: framework/runner detection now resolves the zero-dependency
  `node:test` shape whose `package.json` test script invokes node directly on
  a test file (`"test": "node test/pricing.test.mjs"`). The old marker set
  matched only `node --test` / `node:test` substrings, capping such
  workspaces at `typescript_framework_hint_unresolved`,
  `typescript_test_runner_unresolved`, a `null` verify command, and
  `incomplete_repair_packet` even though the suite parses and the runtime is
  on PATH. The direct-invocation form is credited only when no other
  framework signal matched, and only when node's entry argument names a test
  file (`.test.`/`.spec.` basename or a `test`/`tests`/`__tests__` path
  segment); non-test entries stay fail-closed, and `doctor` reports
  `typescript: node_test` for these workspaces (#6826).
