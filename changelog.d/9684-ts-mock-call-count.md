<!-- section: Fixed -->
- TypeScript: a mock call count (`mock.callCount()`, `mock.calls.length`) no longer reads as an exact value of the arguments a changed side-effect call sends, so it stops crediting a change it cannot notice (RIPR-SPEC-0243 decision 21, #9684).
