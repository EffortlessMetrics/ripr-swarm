# Independent review experiment

```rust
#[path = "/workspace/review-8201-exclusion/crates/ripr/src/analysis/input_boundary.rs"]
mod boundary;
use std::path::Path;
fn check(root: &Path, file: &Path, allowed: bool, label: &str) -> Result<(), Box<dyn std::error::Error>> {
 let guard = boundary::Boundary::new(root)?;
 boundary::observe_module_read(root, file, false);
 let result = guard.verify();
 assert_eq!(result.is_ok(), allowed, "{label}: {result:?}");
 println!("{label}: expected allowed={allowed}; observed {result:?}");
 Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let outer=Path::new("/workspace/8201-evidence/exclusion94/module-fixture");
 let root=outer.join("target/selected"); let holder=outer.join(".git");
 std::fs::create_dir_all(root.join("sub"))?; std::fs::create_dir_all(&holder)?;
 std::fs::write(outer.join("Cargo.toml"),"[workspace]\n")?;
 std::fs::write(root.join("Cargo.toml"),"[workspace]\n")?;
 std::fs::write(root.join("source.rs"),"pub fn f() {}")?;
 let alias=holder.join("selected-alias");std::os::unix::fs::symlink(&root,&alias)?;
 check(&root,&root.join("source.rs"),true,"outer Cargo target is above selected root")?;
 check(&alias,&alias.join("source.rs"),true,"outer git alias is above selected root")?;
 check(&root,&root.join("missing/deep.rs"),true,"physical absent probe")?;
 check(&alias,&alias.join("missing/deep.rs"),true,"aliased absent probe")?;
 for dir in [".git","target"] {
  std::fs::create_dir(root.join(dir))?;
  std::os::unix::fs::symlink(root.join("source.rs"),root.join(dir).join("linked.rs"))?;
  check(&root,&root.join(dir).join("linked.rs"),false,&format!("physical {dir} alias"))?;
  check(&alias,&alias.join(dir).join("linked.rs"),false,&format!("selected alias with {dir} alias"))?;
 }
 let outside=outer.join("outside");std::fs::create_dir(&outside)?;
 std::fs::write(outside.join("source.rs"),"pub fn outside() {}")?;
 std::os::unix::fs::symlink(&outside,root.join("outside-alias"))?;
 check(&root,&root.join("outside-alias/source.rs"),false,"canonical outside source")?;
 check(&root,&root.join("outside-alias/missing.rs"),false,"absent probe through existing outside prefix")?;
 check(&root,&root.join("sub/../source.rs"),false,"parent-dir traversed spelling")?;
 let broken=outer.join("broken");std::fs::create_dir(&broken)?;
 let first=boundary::Boundary::new(&broken)?;let concurrent=boundary::Boundary::new(&root)?;
 std::fs::remove_dir(&broken)?;boundary::observe_module_read(&broken,&broken.join("absent.rs"),false);
 assert!(first.verify().is_err());concurrent.verify()?;drop(first);drop(concurrent);
 boundary::Boundary::new(&root)?.verify()?;
 println!("failed root isolated from concurrent and later fresh root");
 Ok(())
}

```
