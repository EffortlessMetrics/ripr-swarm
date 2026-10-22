<!-- section: Fixed -->
- TypeScript: under `node:test`, a bare `p.then((v) => assert...)` statement now counts as an assertion on the owner call, because `node:test` fails the run on the unhandled rejection; Jest and Vitest, and chains with an earlier `then`/`catch`/`finally`, are unchanged (RIPR-SPEC-0243 decision 21, #9686).
