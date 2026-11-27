//! Repo mode withholds module-identity credit the way diff mode does. Two
//! workspace passes decide when a test's import names another module than the
//! owner's: same-named free functions in sibling modules (#8759) and a src
//! short name that a plain module of the same project also produces (#13745).
//! Both only withdraw credit, so repo mode skipping them credited a test for
//! an owner it never imports.

use super::*;

/// The repo-mode finding at `line` of `owner_file`, after writing `files`
/// into a fresh temporary workspace.
fn repo_finding(
    label: &str,
    files: &[(&str, &str)],
    owner_file: &str,
    line: usize,
) -> Result<Finding, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-py-repo-identity-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
        }
        std::fs::write(&path, contents).map_err(|err| format!("write {relative}: {err}"))?;
    }
    let input = super::repo::select_repo_input_with_limit(
        &root,
        super::repo::RepoWorkingSetLimit {
            limit: 800,
            source: super::repo::RepoWorkingSetCapSource::Default,
        },
    );
    let evidence = super::repo::build_repo_evidence(&input, &root);
    let _ = std::fs::remove_dir_all(&root);
    let owner_file = Path::new(owner_file);
    evidence
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.file == owner_file && finding.probe.location.line == line
        })
        .ok_or_else(|| format!("expected a repo finding at {}:{line}", owner_file.display()))
}

const QUOTE: &str = "def quote(total):\n    return round(total * 1.07, 2)\n";
const QUOTE_TEST: &str =
    "from pkg.a import quote\n\n\ndef test_a():\n    assert quote(100.0) == 107.0\n";

/// #8759 in repo mode: the test imports `pkg.a.quote`, so `pkg/b.py::quote`
/// is not related to it; `pkg/a.py::quote` still is.
#[test]
fn given_same_named_function_in_unimported_module_when_repo_mode_then_no_relation()
-> Result<(), String> {
    let files = [
        ("pkg/a.py", QUOTE),
        ("pkg/b.py", QUOTE),
        ("tests/test_a.py", QUOTE_TEST),
    ];
    let rival = repo_finding("rival", &files, "pkg/b.py", 2)?;
    assert_eq!(rival.class, ExposureClass::NoStaticPath, "{rival:?}");
    assert!(rival.related_tests.is_empty(), "{rival:?}");

    let imported = repo_finding("rival-imported", &files, "pkg/a.py", 2)?;
    assert_eq!(imported.related_tests.len(), 1, "{imported:?}");
    Ok(())
}

const PRICE: &str = "def price(x):\n    return x + 2\n";
const PRICE_TEST: &str =
    "from core import price\n\n\ndef test_price():\n    assert price(1) == 3\n";

/// #13745 in repo mode: a plain `core.py` beside the project's `src/` makes
/// `from core import price` ambiguous, so the src owner is not exposed. A
/// different project's `core.py` does not (discriminating control).
#[test]
fn given_src_short_name_shadowed_by_plain_module_when_repo_mode_then_not_exposed()
-> Result<(), String> {
    let shadowed = repo_finding(
        "shadowed",
        &[
            ("svc/a/src/core.py", PRICE),
            ("svc/a/core.py", PRICE),
            ("svc/a/tests/test_core.py", PRICE_TEST),
        ],
        "svc/a/src/core.py",
        2,
    )?;
    assert_ne!(shadowed.class, ExposureClass::Exposed, "{shadowed:?}");

    let other_project = repo_finding(
        "other-project",
        &[
            ("svc/a/src/core.py", PRICE),
            ("svc/b/core.py", PRICE),
            ("svc/a/tests/test_core.py", PRICE_TEST),
        ],
        "svc/a/src/core.py",
        2,
    )?;
    assert_eq!(
        other_project.class,
        ExposureClass::Exposed,
        "{other_project:?}"
    );
    Ok(())
}

/// Production has cap priority, so a real run with a capped production rival
/// also caps every test. Exercise that selection first, then supply the same
/// test to both evidence producers to isolate the ambiguity decision.
#[test]
fn capped_source_identity_withholds_short_import_credit() -> Result<(), String> {
    let root = super::reexport_tests::unique_tempdir("capped-src-identity")?;
    std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
    let owner = PathBuf::from("src/value.py");
    let test = PathBuf::from("test_value.py");
    std::fs::write(root.join(&owner), PRICE).map_err(|err| err.to_string())?;
    std::fs::write(root.join(&test), PRICE_TEST.replace("core", "value"))
        .map_err(|err| err.to_string())?;
    for (omitted, shadowed) in [("zother.py", false), ("value.py", true)] {
        std::fs::write(root.join(omitted), PRICE).map_err(|err| err.to_string())?;
        for limit in [1, 3] {
            let mut input = super::repo::select_repo_input_with_limit(
                &root,
                super::repo::RepoWorkingSetLimit {
                    limit,
                    source: super::repo::RepoWorkingSetCapSource::Default,
                },
            );
            if limit == 1 {
                assert_eq!(
                    input.production_files.as_slice(),
                    std::slice::from_ref(&owner)
                );
                assert!(input.test_files.is_empty());
                let actual = super::repo::build_repo_evidence(&input, &root);
                assert_eq!(actual.status.as_str(), "capped");
                assert_eq!(actual.counts.capped, 2);
                let finding = actual
                    .findings
                    .iter()
                    .find(|f| f.probe.location.file == owner && f.probe.location.line == 2)
                    .ok_or("missing capped owner")?;
                assert_ne!(finding.class, ExposureClass::Exposed);
                input.test_files.push(test.clone());
            }
            let evidence = super::repo::build_repo_evidence(&input, &root);
            let finding = evidence
                .findings
                .iter()
                .find(|f| f.probe.location.file == owner && f.probe.location.line == 2)
                .ok_or("missing owner")?;
            assert_eq!(
                finding.class == ExposureClass::Exposed,
                !shadowed,
                "limit={limit} omitted={omitted}: {finding:?}"
            );
        }
        std::fs::remove_file(root.join(omitted)).map_err(|err| err.to_string())?;
    }
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())
}
