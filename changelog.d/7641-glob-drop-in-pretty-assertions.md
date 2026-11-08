<!-- section: Fixed -->
- Qualified drop-in assertion macros such as `pretty_assertions::assert_eq!` no longer earn parser oracle credit in files with a glob import that may bind the root from another module without a crate dependency (#7641).
