from pathlib import Path

TARGET = Path("crates/ripr/src/analysis/mod.rs")
text = TARGET.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    text = text.replace(old, new, 1)


function_anchor = '''#[cfg(feature = "lang-typescript")]
pub(crate) fn targeted_typescript_findings_for_scope(
'''
helper = '''#[cfg(feature = "lang-typescript")]
fn typescript_rerun_scope_escapes_workspace(file: &Path) -> bool {
    file.is_absolute()
        || file.components().any(|component| {
            matches!(
                component,
                std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::ParentDir
            )
        })
}

#[cfg(feature = "lang-typescript")]
pub(crate) fn targeted_typescript_findings_for_scope(
'''
replace_once(function_anchor, helper, "insert lexical confinement authority")

old_guard = '''    if file.is_absolute()
        || file
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
'''
new_guard = '''    if typescript_rerun_scope_escapes_workspace(file) {
'''
replace_once(old_guard, new_guard, "replace incomplete path guard")

end_anchor = '''    Ok(result.findings)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptRepoReadiness {
'''
tests = '''    Ok(result.findings)
}

#[cfg(all(test, feature = "lang-typescript"))]
#[test]
fn typescript_rerun_scope_confinement_rejects_rooted_and_parent_paths() -> Result<(), String> {
    for path in [
        Path::new("../outside.ts"),
        Path::new("src/../../outside.ts"),
        Path::new("/etc/hostname"),
    ] {
        if !typescript_rerun_scope_escapes_workspace(path) {
            return Err(format!(
                "escaping TypeScript rerun path was accepted: {}",
                path.display()
            ));
        }
    }

    #[cfg(windows)]
    for path in [
        Path::new(r"\\Windows\\System32"),
        Path::new(r"C:\\Windows\\System32"),
        Path::new(r"C:outside.ts"),
    ] {
        if !typescript_rerun_scope_escapes_workspace(path) {
            return Err(format!(
                "Windows rooted or prefixed rerun path was accepted: {}",
                path.display()
            ));
        }
    }

    for path in [Path::new("src/discount.ts"), Path::new("./src/discount.ts")] {
        if typescript_rerun_scope_escapes_workspace(path) {
            return Err(format!(
                "contained TypeScript rerun path was rejected: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeScriptRepoReadiness {
'''
replace_once(end_anchor, tests, "insert confinement regression")

TARGET.write_text(text, encoding="utf-8")
