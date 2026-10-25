<!-- section: Fixed -->
- TypeScript test extraction no longer scales quadratically with the number of identifier writes in a test file. A 5 MB single-line file under `tests/` took about 187 s per `ripr check` or LSP refresh in a debug build and now takes about 20 s; extraction output is unchanged (#10017).
