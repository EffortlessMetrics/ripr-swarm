<!-- section: Fixed -->
- `ripr check` skips the parse and token walk of a file that never mentions a macro-generator name when it looks for the first `name!` call (imported-macro scan), so a workspace-wide index build does less parsing. On this repository's `crates/ripr` diff the warm check went from 132 s to 100 s on one machine, with byte-identical cold and warm reports (#10809).
