<!-- section: Fixed -->
- `ripr doctor`: the nested `Cargo.toml` walk visits directories in name order, so a tree larger than its 512-directory cap reports the same manifest on every filesystem; `ripr check` output is now pinned byte-identical across repeat runs, worker counts and file write order for Rust, Python and TypeScript (#14230).
