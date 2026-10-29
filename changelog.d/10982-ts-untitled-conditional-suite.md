<!-- section: Fixed -->
- An untitled `describe.skipIf(cond)(() => { ... })` / `describe.runIf(cond)(() => { ... })` suite is now indexed for reach under the factory call as its guard, with no assertion credit, instead of being reported as an unextracted `typescript_test_extraction_partial` shape (#10982).
