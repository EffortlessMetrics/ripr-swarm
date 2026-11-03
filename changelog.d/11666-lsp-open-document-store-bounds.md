<!-- section: Fixed -->
- `ripr lsp --stdio` now bounds the open-document store: more than 4,096 open URIs or more than 512 MiB of retained buffer text is refused through the existing invalid-incremental-change path (the overflow URI is kept with empty text so later `didChange`/`didClose` still find it) instead of pinning unbounded editor memory (#11666).
