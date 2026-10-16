# Independent review experiment

```rust
#[path = "/workspace/review-8201-repair/crates/ripr/src/analysis/input_boundary.rs"]
mod input_boundary;
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let root = std::path::Path::new("/workspace/8201-evidence/repair490/lifecycle-fixture");
 let broken = root.join("broken"); let stable = root.join("stable");
 std::fs::create_dir_all(&broken)?; std::fs::create_dir_all(&stable)?;
 let first = input_boundary::Boundary::new(&broken)?;
 let concurrent = input_boundary::Boundary::new(&stable)?;
 std::fs::remove_dir(&broken)?;
 input_boundary::observe_module_read(&broken, &broken.join("absent.rs"), false);
 assert!(first.verify().is_err()); concurrent.verify()?; drop(first);
 let fresh = input_boundary::Boundary::new(&stable)?; fresh.verify()?;
 println!("failed root refused; concurrent unrelated and later fresh boundaries accepted");
 Ok(())
}

```
