<!-- section: Fixed -->
- Draft and Fast diff indexes now treat target-root placement as unavailable when an unindexed workspace member may compile other crates (`include!` or `#[path]`), so the generated-mount negative stays withheld while indexed members across packages stay credited (#10149).
