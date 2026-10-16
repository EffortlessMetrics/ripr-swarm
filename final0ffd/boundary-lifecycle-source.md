# Internal exact-module lifecycle experiment

```rust
#[path = "/workspace/review-8201-final/crates/ripr/src/analysis/input_boundary.rs"]
mod input_boundary;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new("/workspace/8201-evidence/final0ffd/lifecycle-fixture");
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir_all(&first)?;
    std::fs::create_dir_all(&second)?;
    let one = input_boundary::Boundary::new(&first)?;
    one.verify()?;
    std::fs::remove_dir(&first)?;
    input_boundary::observe_module_read(&first, &first.join("lib.rs"), false);
    assert!(one.verify().is_err());
    drop(one);
    let two = input_boundary::Boundary::new(&second)?;
    let result = two.verify();
    println!("fresh stable boundary after completed failed run: {result:?}");
    assert!(result.is_err(), "control must expose sticky process-wide failure state");
    Ok(())
}

```
