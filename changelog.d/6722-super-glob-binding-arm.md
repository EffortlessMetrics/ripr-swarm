<!-- section: Fixed -->
- Inline test modules' `use super::*` no longer make ripr treat every bare lowercase match binding as a possible constant import for the whole file: only a file-root `use super::*` (which may import another module's items) still demotes binding-arm selection (#6722).
