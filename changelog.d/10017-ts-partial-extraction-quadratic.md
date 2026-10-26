<!-- section: Fixed -->
- A TypeScript test file with many tests no longer takes quadratic time in the partial-extraction disclosure: a 5 MB test file added about 127 s to `ripr check` and now adds about 22 s, growing linearly with size (#10017).
