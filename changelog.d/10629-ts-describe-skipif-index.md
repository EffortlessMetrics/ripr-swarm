<!-- section: Fixed -->
- TypeScript tests inside `describe.skipIf(cond)` / `describe.runIf(cond)` suites are now indexed for reach under the factory call as their guard, with no assertion credit, instead of being reported as unextracted `typescript_test_extraction_partial` shapes (#10629).
