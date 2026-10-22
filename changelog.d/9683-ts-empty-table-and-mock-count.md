<!-- section: Fixed -->
- TypeScript: a loop over an empty literal table no longer counts as an assertion, and a mock call count (`mock.callCount()`, `mock.calls.length`) no longer reads as an exact value of the arguments a changed side-effect call sends, so both stop crediting changes they cannot notice (RIPR-SPEC-0243 decision 19, #9683, #9684).
