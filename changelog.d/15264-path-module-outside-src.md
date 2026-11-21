<!-- section: Fixed -->
- `ripr check` now analyzes a change in a module the crate root mounts through `#[path]` from outside `src/` (for example `#[path = "../impl/price.rs"] pub mod price;`), instead of reporting that no changed line is behavior ripr checks (#15264)
