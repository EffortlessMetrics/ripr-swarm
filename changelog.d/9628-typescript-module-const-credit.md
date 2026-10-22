<!-- section: Fixed -->
- TypeScript findings credit a test that compares an edited module-level scalar
  constant (`export const LIMIT = 11;`), including a non-ASCII constant name
  such as `bannière`. Both previously read `no_static_path` / `limited`
  although the test fails on the edit (#9628, RIPR-SPEC-0027).
