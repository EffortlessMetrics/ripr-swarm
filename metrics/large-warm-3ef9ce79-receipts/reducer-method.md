The driver below parses Rust attributes with the repository dependency ra_ap_syntax 0.0.349. It inserts cfg(any()) only before real #[test] attributes, excluding the entire selected indirect_owner.rs module. It does not match text inside strings/comments. The preparation caller records each insertion offset in reducer-manifest.json, reverses all insertions byte-for-byte, and compares the selected module with published 3ef9ce79. Result: 18,713 unrelated test attributes across 977 files disabled; selected module identical. This is a reduced test harness, not full-library qualification.

```rust
use ra_ap_syntax::{AstNode, Edition, SourceFile, ast};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let mut source = std::fs::read_to_string(&path)?;
        let parsed = SourceFile::parse(&source, Edition::Edition2024);
        let mut positions = parsed.tree().syntax().descendants()
            .filter_map(ast::Attr::cast)
            .filter(|attr| attr.syntax().text().to_string() == "#[test]")
            .map(|attr| u32::from(attr.syntax().text_range().start()) as usize)
            .collect::<Vec<_>>();
        positions.sort_unstable();
        let count = positions.len();
        let offsets = positions.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
        for position in positions.into_iter().rev() {
            source.insert_str(position, "#[cfg(any())]\n");
        }
        if count > 0 {
            std::fs::write(&path, source)?;
            println!("{count}\t{path}\t{offsets}");
        }
    }
    Ok(())
}
```

Build: rustc --edition=2024 reducer-driver.rs -L dependency=$RELEASE_DEPS --extern ra_ap_syntax=$EXACT_RA_RLIB -o reducer-driver

Selected suite: CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=1 cargo test --locked -p ripr --lib indirect_owner -- --nocapture --test-threads=2

Old-loop challenge: replace only judged_for_owner with exact b51 implementation plus the identical test-only pre-mask counter; keep all tests and harness gates. Run same command filtered to indirect_owner_body_admission. Both controls fail at 204 versus 1. Restore selected module and compare its bytes with candidate.
