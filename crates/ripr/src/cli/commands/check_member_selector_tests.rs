//! Member-directory `file:line` selector binding (#8767, #8872): which repair
//! items are rewritten to a workspace-relative path, which pass through to the
//! seam/gap id resolvers untouched, and the two refusals that keep a rebound
//! selector from landing on another package's file. The process-level
//! behavior stays with `tests/task_first_facade.rs`; these pin the decision
//! table and the exact refusal text of the binder itself.

use super::super::tests::unique_command_test_dir;
use super::*;
use crate::testing::cwd_lock;

/// A workspace `ws/` with a root package, a `member` package and an `other`
/// package, plus a sibling `outside/` directory that is not under the root.
struct Workspace {
    base: PathBuf,
    root: PathBuf,
}

impl Workspace {
    fn new(label: &str) -> Result<Self, String> {
        let base = unique_command_test_dir(label);
        let guard = Self {
            root: base.join("ws"),
            base,
        };
        for file in [
            "ws/src/lib.rs",
            "ws/member/src/lib.rs",
            "ws/member/src/only_member.rs",
            "ws/other/src/lib.rs",
            "outside/probe.rs",
        ] {
            let path = guard.base.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            std::fs::write(&path, "pub fn one() -> u8 { 1 }\n").map_err(|err| err.to_string())?;
        }
        Ok(guard)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

struct CwdRestore(PathBuf);

impl Drop for CwdRestore {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// Enter `ws/member`, the invocation directory every bound selector is
/// relative to. The returned guard restores the previous directory.
fn enter_member(workspace: &Workspace) -> Result<CwdRestore, String> {
    let original = std::env::current_dir().map_err(|err| err.to_string())?;
    let restore = CwdRestore(original);
    std::env::set_current_dir(workspace.root.join("member")).map_err(|err| err.to_string())?;
    Ok(restore)
}

fn shadow_refusal(bound: &str, suffix: &str) -> String {
    format!(
        "`{bound}` also matches the workspace-root file `{suffix}` by path suffix, and a file:line selector cannot tell them apart; pass `--root .` to analyze this directory alone, or select the gap by its seam or gap ID"
    )
}

#[test]
fn root_shadowed_selector_is_refused_only_for_a_proper_suffix_that_is_a_root_file()
-> Result<(), String> {
    let workspace = Workspace::new("selector-shadow")?;
    let root = workspace.root.as_path();

    assert_eq!(
        refuse_root_shadowed_selector(root, "member/src/lib.rs"),
        Err(shadow_refusal("member/src/lib.rs", "src/lib.rs"))
    );
    // No proper suffix of this path names a file under the root.
    assert_eq!(
        refuse_root_shadowed_selector(root, "member/src/only_member.rs"),
        Ok(())
    );
    // The whole path is not a proper suffix: the root file names itself.
    assert_eq!(refuse_root_shadowed_selector(root, "src/lib.rs"), Ok(()));
    Ok(())
}

#[test]
fn root_shadowed_selector_never_follows_a_parent_suffix_out_of_the_root() -> Result<(), String> {
    let workspace = Workspace::new("selector-shadow-parent")?;
    let root = workspace.root.as_path();

    // Fixture precondition: the `..` suffix does reach a real file, so only
    // the parent-component skip keeps it from being reported as a shadow.
    if !root.join("../outside/probe.rs").is_file() {
        return Err("fixture: ../outside/probe.rs must exist beside the root".to_string());
    }
    assert_eq!(
        refuse_root_shadowed_selector(root, "pkg/../outside/probe.rs"),
        Ok(())
    );
    // A later suffix without `..` is still checked and named.
    assert_eq!(
        refuse_root_shadowed_selector(root, "member/../src/lib.rs"),
        Err(shadow_refusal("member/../src/lib.rs", "src/lib.rs"))
    );
    Ok(())
}

#[test]
fn member_selector_item_binds_an_invocation_directory_file_and_keeps_the_line() -> Result<(), String>
{
    let _cwd = cwd_lock::hold_cwd();
    let workspace = Workspace::new("selector-bind")?;
    let _restore = enter_member(&workspace)?;
    let root = workspace.root.as_path();

    assert_eq!(
        bind_member_selector_item(Some("member"), root, "src/only_member.rs:7"),
        Ok("member/src/only_member.rs:7".to_string())
    );
    assert_eq!(
        bind_member_selector_item(Some("member"), root, "./src/only_member.rs:7"),
        Ok("member/src/only_member.rs:7".to_string())
    );
    // An unmoved root (no prefix) leaves even a shadowed selector alone.
    assert_eq!(
        bind_member_selector_item(None, root, "src/lib.rs:3"),
        Ok("src/lib.rs:3".to_string())
    );
    Ok(())
}

#[test]
fn member_selector_item_refuses_a_rebound_file_the_root_package_shadows() -> Result<(), String> {
    let _cwd = cwd_lock::hold_cwd();
    let workspace = Workspace::new("selector-bind-shadow")?;
    let _restore = enter_member(&workspace)?;
    let root = workspace.root.as_path();

    assert_eq!(
        bind_member_selector_item(Some("member"), root, "src/lib.rs:3"),
        Err(shadow_refusal("member/src/lib.rs", "src/lib.rs"))
    );
    // Already workspace-relative: nothing was rebound, so the typed path is
    // kept even though its `src/lib.rs` suffix names the root package's file.
    assert_eq!(
        bind_member_selector_item(Some("member"), root, "other/src/lib.rs:4"),
        Ok("other/src/lib.rs:4".to_string())
    );
    Ok(())
}

#[test]
fn member_selector_item_refuses_a_file_found_in_neither_directory() -> Result<(), String> {
    let _cwd = cwd_lock::hold_cwd();
    let workspace = Workspace::new("selector-bind-missing")?;
    let _restore = enter_member(&workspace)?;
    let root = workspace.root.as_path();

    assert_eq!(
        bind_member_selector_item(Some("member"), root, "./src/absent.rs:2"),
        Err(
            "`./src/absent.rs` names no file under the invocation directory `member` or the workspace root; pass `--root .` to select this directory, or the full workspace-relative path (for example `member/src/absent.rs`)"
                .to_string()
        )
    );
    Ok(())
}

#[test]
fn member_selector_item_passes_ids_and_non_file_line_items_through_unchanged() -> Result<(), String>
{
    let _cwd = cwd_lock::hold_cwd();
    let workspace = Workspace::new("selector-bind-ids")?;
    let _restore = enter_member(&workspace)?;
    let root = workspace.root.as_path();

    // Each of these would be rebound or refused if it were read as a
    // `file:line` selector: `src/lib.rs` exists in the invocation directory
    // and is shadowed by the root package, and `src/absent.rs` exists nowhere.
    for item in [
        "gap:rust:src/lib.rs:3",
        "repo-probe:src/lib.rs:predicate:12345678",
        "src/lib.rs",
        "src/lib.rs:three",
        "src/absent.rs:",
        "seam:12",
    ] {
        assert_eq!(
            bind_member_selector_item(Some("member"), root, item),
            Ok(item.to_string()),
            "{item} must reach the id resolvers unchanged"
        );
    }
    Ok(())
}
