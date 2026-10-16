# Independent review experiment

```rust
#[path = "/workspace/review-8201-repair/crates/ripr/src/analysis/input_boundary.rs"]
mod input_boundary;
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let root = std::path::Path::new("/workspace/8201-evidence/repair490/module-fixture");
 std::fs::create_dir_all(root.join(".git"))?;
 std::fs::write(root.join("source.rs"), "pub fn value() {}")?;
 if !root.join(".git/private.rs").exists() { std::os::unix::fs::symlink(root.join("source.rs"), root.join(".git/private.rs"))?; }
 let boundary = input_boundary::Boundary::new(root)?;
 input_boundary::observe_module_read(root, &root.join(".git/private.rs"), false);
 println!("observed lexical .git read via symlink to captured root source: {:?}", boundary.verify());
 Ok(())
}

```
