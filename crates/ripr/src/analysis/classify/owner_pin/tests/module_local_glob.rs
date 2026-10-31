//! A private file-level foreign glob (`use rayon::prelude::*;`) is visible in
//! its own module and the modules declared beneath it, so it blocks the
//! standard-macro credit of those tests and no others. Anything that leaves
//! the module ancestry unestablished keeps the workspace-wide refusal.

use super::*;

const GLOBS: &str = "src/globs.rs";
const SUITE: &str = "src/suite.rs";
const SUITE_TESTS: &str = "src/suite/tests.rs";
const ROOT_LIB: &str = "mod globs;\nmod suite;\npub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
const GLOB: &str =
    "use rayon::prelude::*;\npub fn drain(input: &mut Vec<u8>) {\n    input.clear();\n}\n";
const PLAIN: &str = "pub fn drain(input: &mut Vec<u8>) {\n    input.clear();\n}\n";
const SUITE_BODY: &str =
    "use crate::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";

fn refusal_of(index: &RustIndex, test_file: &str) -> Option<AssertionRefusal> {
    let test = index
        .tests()
        .iter()
        .find(|test| test.file == Path::new(test_file));
    assert!(
        test.is_some(),
        "the fixture test in {test_file} must be indexed"
    );
    let test = test?;
    assert!(
        !test.assertions.is_empty(),
        "the test's assertions must parse"
    );
    OwnerPinSyntax::default().refusal(test, &test.assertions[0], index)
}

fn is_binding(refusal: &Option<AssertionRefusal>) -> bool {
    matches!(
        refusal,
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    )
}

/// The fixture is native-valid: the modules mount and the test is credited
/// when no file carries a glob, so every refusal below is the glob's.
#[test]
fn the_control_layout_credits_the_test() {
    let index = index(&[(LIB, ROOT_LIB), (GLOBS, PLAIN), (SUITE, SUITE_BODY)]);
    assert_eq!(refusal_of(&index, SUITE), None);
}

#[test]
fn a_sibling_modules_private_glob_does_not_block_the_test() {
    let index = index(&[(LIB, ROOT_LIB), (GLOBS, GLOB), (SUITE, SUITE_BODY)]);
    assert_eq!(refusal_of(&index, SUITE), None);
}

#[test]
fn a_glob_in_the_tests_own_file_or_an_ancestor_still_blocks_it() {
    let own = format!("use rayon::prelude::*;\n{SUITE_BODY}");
    let index_own = index(&[(LIB, ROOT_LIB), (GLOBS, PLAIN), (SUITE, own.as_str())]);
    assert!(is_binding(&refusal_of(&index_own, SUITE)), "own file");
    let lib = format!("use rayon::prelude::*;\n{ROOT_LIB}");
    let index_root = index(&[(LIB, lib.as_str()), (GLOBS, PLAIN), (SUITE, SUITE_BODY)]);
    assert!(is_binding(&refusal_of(&index_root, SUITE)), "crate root");
}

#[test]
fn a_glob_reaches_a_child_test_module_file_through_use_super() {
    let parent = "use rayon::prelude::*;\nmod tests;\npub fn w() {}\n";
    let child = "use super::*;\n\n#[test]\nfn weighs() {\n    assert_eq!(2 + 2, 4);\n}\n";
    let lib = "mod suite;\n";
    let blocked = index(&[(LIB, lib), (SUITE, parent), (SUITE_TESTS, child)]);
    assert!(is_binding(&refusal_of(&blocked, SUITE_TESTS)));
    // Control: the same child without the parent's glob is credited.
    let control = index(&[
        (LIB, lib),
        (SUITE, "mod tests;\npub fn w() {}\n"),
        (SUITE_TESTS, child),
    ]);
    assert_eq!(refusal_of(&control, SUITE_TESTS), None);
}

#[test]
fn a_path_attribute_mount_is_a_module_of_its_declarer() {
    // `#[path]` mounts a file as its own module: its glob stays inside it.
    let lib = "#[path = \"globs.rs\"]\nmod mounted;\nmod suite;\n";
    let mounted = index(&[(LIB, lib), (GLOBS, GLOB), (SUITE, SUITE_BODY)]);
    assert_eq!(refusal_of(&mounted, SUITE), None);
    // An unresolvable `#[path]` may mount any file anywhere, so nothing is
    // confined.
    let lib = "#[path = concat!(\"glo\", \"bs.rs\")]\nmod mounted;\nmod suite;\n";
    let unknown = index(&[(LIB, lib), (GLOBS, GLOB), (SUITE, SUITE_BODY)]);
    assert!(is_binding(&refusal_of(&unknown, SUITE)));
}

#[test]
fn an_include_fragment_glob_lands_in_its_includer_and_stays_workspace_wide() {
    let mut index = index(&[(LIB, ROOT_LIB), (GLOBS, GLOB), (SUITE, SUITE_BODY)]);
    assert_eq!(refusal_of(&index, SUITE), None, "control before include");
    index.include_targets.insert(PathBuf::from(GLOBS));
    assert!(is_binding(&refusal_of(&index, SUITE)));
}

#[test]
fn an_unresolved_include_anywhere_leaves_the_glob_workspace_wide() {
    use crate::analysis::facts::RustIncludeLimitation;
    let mut index = index(&[(LIB, ROOT_LIB), (GLOBS, GLOB), (SUITE, SUITE_BODY)]);
    index.include_limitations.push(RustIncludeLimitation {
        parent: PathBuf::from(LIB),
        line: 1,
        expression: "include!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"))".to_string(),
        reason_code: "rust_include_unresolved".to_string(),
    });
    assert!(is_binding(&refusal_of(&index, SUITE)));
}

#[test]
fn a_public_glob_is_not_confined() {
    let public = "pub use rayon::prelude::*;\n";
    let index = index(&[(LIB, ROOT_LIB), (GLOBS, public), (SUITE, SUITE_BODY)]);
    assert!(is_binding(&refusal_of(&index, SUITE)));
}

#[test]
fn a_withheld_file_keeps_its_glob_workspace_wide() {
    let mut withheld = WithheldMacroBindings::default();
    let packages = BTreeSet::new();
    let drop_ins = DropInManifests::default();
    withheld.absorb(Path::new(GLOBS), GLOB, &packages, &drop_ins);
    assert!(withheld.any_name, "a withheld file has no module edges");
}

/// The retained lookup agrees with the reference scan for a test in a
/// different file than its root, for every name and every asking file.
#[test]
fn the_refusal_site_names_the_glob_only_for_tests_it_reaches() {
    let index = index(&[(LIB, ROOT_LIB), (GLOBS, GLOB), (SUITE, SUITE_BODY)]);
    let syntax = OwnerPinSyntax::default();
    let roots = syntax.target_roots(&index);
    let modules = syntax.resolved_module_declarations(&index);
    let resolved = |path: &Path, line: usize, declaration: &str| {
        modules.contains(&(path.to_path_buf(), line, declaration.to_string()))
    };
    let root = Some(Path::new(LIB));
    for name in assertion_context_macros() {
        for file in [LIB, GLOBS, SUITE] {
            let file = Some(Path::new(file));
            assert_eq!(
                syntax.workspace_macro_site_for(name, root, file, &index),
                workspace_macro_binding_site_for(name, root, file, &index, roots, &resolved),
                "{name} at {file:?}"
            );
        }
    }
    let site = |file: &str| {
        syntax
            .workspace_macro_site_for("assert_eq", root, Some(Path::new(file)), &index)
            .map(|(path, site)| (path, site.kind))
    };
    assert_eq!(site(SUITE), None);
    assert!(
        matches!(site(GLOBS), Some((path, MacroBindingKind::ForeignGlob(_))) if path == Path::new(GLOBS))
    );
    // The test's file is unknown: the glob keeps its crate-local reach.
    assert!(
        syntax
            .workspace_macro_site_for("assert_eq", root, None, &index)
            .is_some()
    );
}
