//! An `include!` ripr cannot place refuses assertion credit only as far as
//! the paste can reach (#14116). Before, one such `include!` anywhere plus one
//! private glob of a foreign crate anywhere refused every assertion in the
//! workspace. Every row changes `score` in `src/b.rs` from `x + 1` to `x + 2`
//! beside a sibling module holding `use rayon::prelude::*;`.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str = "pub mod a;\npub mod b;\npub mod gen;\n";
const GLOB: &str = "use rayon::prelude::*;\npub fn twice(x: i32) -> i32 { x * 2 }\n";
const OWNER: &str = "pub fn score(x: i32) -> i32 { x + 2 }\n";
const PLAIN: &str = "pub const K: i32 = 1;\n";
const NAMESAKE: &str = "pub fn score(x: i32) -> i32 { x + 3 }\n";
const NESTED: &str = "pub mod inner { include!(\"generated/g.rs\"); }\n";
const SCIP_GENERATED: &str = include_str!("../src/analysis/scip/generated/scip.rs");
const DIFF: &str = "diff --git a/src/b.rs b/src/b.rs
index 1111111..2222222 100644
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,1 +1,1 @@
-pub fn score(x: i32) -> i32 { x + 1 }
+pub fn score(x: i32) -> i32 { x + 2 }
";

/// `src/b.rs` with a unit test that reaches `score` through `imports`.
fn unit(imports: &str, call: &str) -> String {
    format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n{imports}\n    #[test]\n    fn pins() {{ assert_eq!({call}, 4); }}\n}}\n"
    )
}

/// The ordinary unit test: `use super::*;` and a bare call.
fn own_unit() -> String {
    unit("    use super::*;", "score(2)")
}

/// An integration test file: `head`, then one test asserting `score(2)`.
fn integration(head: &str) -> String {
    format!("{head}\n#[test]\nfn t() {{ assert_eq!(score(2), 4); }}\n")
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-include-limit-scope-{}-{sequence}",
            std::process::id()
        ));
        // Built before any write so `Drop` removes a partly written root.
        let repo = Self { root };
        let root = &repo.root;
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&path, text).map_err(|error| format!("write {path:?} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", LIB)?;
        write("src/a.rs", GLOB)?;
        write("src/gen.rs", PLAIN)?;
        write("diff.patch", DIFF)?;
        for (path, text) in files {
            write(path, text)?;
        }
        Ok(repo)
    }

    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut found = output.findings.into_iter().filter(|finding| {
            finding.probe.location.line == 1 && finding.probe.location.file.ends_with("b.rs")
        });
        match (found.next(), found.next()) {
            (Some(finding), None) => Ok(finding),
            _ => Err("premise: exactly one finding on the rewritten line".to_string()),
        }
    }

    fn class(&self) -> Result<ExposureClass, String> {
        Ok(self.finding()?.class)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The one `assertion not credited:` line a refused finding carries.
fn only_refusal(found: &Finding) -> Result<&str, String> {
    let mut refusals = found
        .evidence
        .iter()
        .filter(|line| line.starts_with("assertion not credited: "));
    match (refusals.next(), refusals.next()) {
        (Some(refusal), None) => Ok(refusal),
        _ => Err(format!(
            "premise: exactly one assertion refusal, got {:?}",
            found.evidence
        )),
    }
}

fn classify(files: &[(&str, &str)]) -> Result<ExposureClass, String> {
    TempRepo::create(files)?.class()
}

fn check(rows: &[(&str, Vec<(&str, String)>)], want: &ExposureClass) -> Result<(), String> {
    for (name, files) in rows {
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (*path, text.as_str()))
            .collect();
        let got = classify(&files)?;
        if got != *want {
            return Err(format!("{name}: got {got:?}, want {want:?}"));
        }
    }
    Ok(())
}

#[test]
fn the_glob_alone_and_an_unresolved_include_are_the_two_baselines() -> Result<(), String> {
    // No include: the sibling glob is confined to its module and the exact
    // assertion pins the owner.
    let confined = classify(&[("src/b.rs", &own_unit())])?;
    if confined != ExposureClass::Exposed {
        return Err(format!("no include: got {confined:?}"));
    }
    // A dynamic include may paste anything anywhere: the refusal stays.
    let dynamic = classify(&[
        ("src/b.rs", &own_unit()),
        (
            "src/gen.rs",
            "pub mod inner { include!(concat!(env!(\"OUT_DIR\"), \"/g.rs\")); }\n",
        ),
    ])?;
    if dynamic != ExposureClass::StaticUnknown {
        return Err(format!("dynamic include: got {dynamic:?}"));
    }
    Ok(())
}

#[test]
fn an_include_in_a_test_crate_leaves_the_librarys_tests_credited() -> Result<(), String> {
    let nested = "mod frag { include!(\"fixtures/frag.rs\"); }\n#[test]\nfn t() { assert_eq!(frag::K, 1); }\n";
    let file_level = "include!(\"fixtures/frag.rs\");\n#[test]\nfn t() { assert_eq!(K, 1); }\n";
    check(
        &[
            (
                "a nested include of an unindexed fixture",
                vec![
                    ("src/b.rs", own_unit()),
                    ("tests/it.rs", nested.to_string()),
                    ("tests/fixtures/frag.rs", PLAIN.to_string()),
                ],
            ),
            (
                "a file-level include of an unindexed fixture",
                vec![
                    ("src/b.rs", own_unit()),
                    ("tests/it.rs", file_level.to_string()),
                    ("tests/fixtures/frag.rs", PLAIN.to_string()),
                ],
            ),
            (
                "a fixture that globs a foreign crate stays inside its test crate",
                vec![
                    ("src/b.rs", own_unit()),
                    ("tests/it.rs", nested.to_string()),
                    ("tests/fixtures/frag.rs", GLOB.to_string()),
                ],
            ),
        ],
        &ExposureClass::Exposed,
    )
}

#[test]
fn a_read_inert_fragment_in_the_library_leaves_its_tests_credited() -> Result<(), String> {
    check(
        &[
            (
                "a nested include of a fragment no module declares",
                vec![
                    ("src/b.rs", own_unit()),
                    ("src/gen.rs", NESTED.to_string()),
                    ("src/generated/g.rs", PLAIN.to_string()),
                ],
            ),
            (
                "a nested include of a fixture",
                vec![
                    ("src/b.rs", own_unit()),
                    (
                        "src/gen.rs",
                        "pub mod inner { include!(\"fixtures/g.rs\"); }\n".to_string(),
                    ),
                    ("src/fixtures/g.rs", PLAIN.to_string()),
                ],
            ),
            (
                "a nested include whose fragment declares only inline modules",
                vec![
                    (
                        "src/lib.rs",
                        "pub mod a;\npub mod analysis;\npub mod b;\n".to_string(),
                    ),
                    ("src/analysis/mod.rs", "pub mod scip;\n".to_string()),
                    (
                        "src/analysis/scip/mod.rs",
                        "pub mod generated { pub mod scip { include!(\"generated/scip.rs\"); } }\n"
                            .to_string(),
                    ),
                    (
                        "src/analysis/scip/generated/scip.rs",
                        SCIP_GENERATED.to_string(),
                    ),
                    ("src/b.rs", own_unit()),
                ],
            ),
        ],
        &ExposureClass::Exposed,
    )
}

#[test]
fn an_unread_mounting_or_binding_fragment_keeps_the_refusal() -> Result<(), String> {
    let nested = "mod frag { include!(\"fixtures/frag.rs\"); }\n";
    let uses_owner = integration(&format!("{nested}use demo::b::score;"));
    check(
        &[
            (
                "a missing fragment",
                vec![
                    ("src/b.rs", own_unit()),
                    ("tests/it.rs", nested.to_string()),
                ],
            ),
            (
                "a fragment declaring an out-of-line module",
                vec![
                    ("src/b.rs", own_unit()),
                    ("tests/it.rs", nested.to_string()),
                    ("tests/fixtures/frag.rs", "mod y;\n".to_string()),
                ],
            ),
            (
                "a fragment mounting a file by path",
                vec![
                    ("src/b.rs", own_unit()),
                    ("src/gen.rs", NESTED.to_string()),
                    (
                        "src/generated/g.rs",
                        "#[path = \"../b.rs\"] mod again;\n".to_string(),
                    ),
                ],
            ),
            (
                "a library fragment exporting an assertion macro",
                vec![
                    ("src/b.rs", own_unit()),
                    ("src/gen.rs", NESTED.to_string()),
                    (
                        "src/generated/g.rs",
                        "#[macro_export]\nmacro_rules! assert_eq { ($($t:tt)*) => {} }\n"
                            .to_string(),
                    ),
                ],
            ),
            (
                "a library fragment globbing a foreign crate",
                vec![
                    ("src/b.rs", own_unit()),
                    ("src/gen.rs", NESTED.to_string()),
                    (
                        "src/generated/g.rs",
                        "pub use rayon::prelude::*;\n".to_string(),
                    ),
                ],
            ),
            (
                "the test crate's own test beside its binding fragment",
                vec![
                    ("src/b.rs", OWNER.to_string()),
                    ("tests/it.rs", uses_owner),
                    ("tests/fixtures/frag.rs", GLOB.to_string()),
                ],
            ),
            (
                "a file the binding test crate also compiles",
                vec![
                    ("src/b.rs", own_unit()),
                    (
                        "tests/it.rs",
                        format!("{nested}#[path = \"../src/b.rs\"]\nmod b;\n"),
                    ),
                    ("tests/fixtures/frag.rs", GLOB.to_string()),
                ],
            ),
        ],
        &ExposureClass::StaticUnknown,
    )
}

/// A `[lib] path` under `tests/` is a library other crates can import, so
/// a fragment that may bind an assertion macro there is not confined to it.
#[test]
fn a_library_rooted_under_tests_is_not_an_unimportable_crate() -> Result<(), String> {
    let workspace = "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\nmembers = [\"dep\"]\n";
    let member = "[package]\nname = \"dep\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"tests/lib.rs\"\n\n[dependencies]\ndemo = { path = \"..\" }\n";
    let binding = "#[macro_export]\nmacro_rules! assert_eq { ($($t:tt)*) => {} }\n";
    let nested = "mod frag { include!(\"fixtures/frag.rs\"); }\n";
    check(
        &[(
            "a binding fragment in a custom library root",
            vec![
                ("Cargo.toml", workspace.to_string()),
                ("src/b.rs", own_unit()),
                ("dep/Cargo.toml", member.to_string()),
                ("dep/tests/lib.rs", nested.to_string()),
                ("dep/tests/fixtures/frag.rs", binding.to_string()),
            ],
        )],
        &ExposureClass::StaticUnknown,
    )
}

/// A fragment path that resolves outside the repository through a symlink is
/// never read: the include keeps the workspace-wide refusal.
#[cfg(unix)]
#[test]
fn a_fragment_symlinked_out_of_the_repository_is_not_read() -> Result<(), String> {
    let repo = TempRepo::create(&[
        ("src/b.rs", &own_unit()),
        (
            "tests/it.rs",
            "mod frag { include!(\"fixtures/frag.rs\"); }\n",
        ),
    ])?;
    let outside = repo.root.with_extension("outside.rs");
    std::fs::write(&outside, PLAIN).map_err(|error| error.to_string())?;
    let fixtures = repo.root.join("tests/fixtures");
    let linked = std::fs::create_dir_all(&fixtures)
        .and_then(|()| std::os::unix::fs::symlink(&outside, fixtures.join("frag.rs")))
        .map_err(|error| error.to_string());
    let class = linked.and_then(|()| repo.class());
    let _ = std::fs::remove_file(&outside);
    match class? {
        ExposureClass::StaticUnknown => Ok(()),
        other => Err(format!("symlinked fragment: got {other:?}")),
    }
}

#[test]
fn the_refusal_names_the_include_that_makes_placement_unavailable() -> Result<(), String> {
    // `benches/` sorts before `src/`, so its include limitation is first in
    // iteration order; it stays inside the bench crate and must not be named.
    let repo = TempRepo::create(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod aa;\npub mod b;\npub mod gen;\n",
        ),
        (
            "src/aa.rs",
            "pub mod inert { include!(\"generated/plain.rs\"); }\n",
        ),
        ("src/generated/plain.rs", PLAIN),
        ("src/b.rs", &own_unit()),
        ("src/gen.rs", NESTED),
        ("src/generated/g.rs", "mod y;\n"),
        (
            "benches/bench.rs",
            "mod frag { include!(\"fixtures/g.rs\"); }\nfn main() {}\n",
        ),
        ("benches/fixtures/g.rs", PLAIN),
    ])?;
    let found = repo.finding()?;
    assert_eq!(found.class, ExposureClass::StaticUnknown);
    let refusal = only_refusal(&found)?;
    for expected in [
        "placement is unavailable",
        "`use rayon::prelude::*;` at src/a.rs:1 could not be confined",
        "first include limitation: `include!(",
        "generated/g.rs",
        "src/gen.rs:1",
        "rust_include_nested_module_context",
    ] {
        assert!(
            refusal.contains(expected),
            "{expected:?} missing from {refusal:?}"
        );
    }
    // The earlier bench-crate include is not workspace-reaching, so it is not
    // the one named.
    assert!(!refusal.contains("benches"), "{refusal:?}");

    Ok(())
}

#[test]
fn a_separate_placement_failure_keeps_the_direct_glob_explanation() -> Result<(), String> {
    // The mounting include is not the only reason placement is unavailable:
    // the unrelated parser fallback independently disables it too.
    let repo = TempRepo::create(&[
        (
            "src/lib.rs",
            "pub mod a;\npub mod b;\npub mod broken;\npub mod gen;\n",
        ),
        ("src/broken.rs", "fn broken(\n"),
        ("src/b.rs", &own_unit()),
        ("src/gen.rs", NESTED),
        ("src/generated/g.rs", "mod y;\n"),
    ])?;
    let found = repo.finding()?;
    assert_eq!(found.class, ExposureClass::StaticUnknown);
    let refusal = only_refusal(&found)?;
    assert!(
        refusal.contains("`use rayon::prelude::*;` at src/a.rs:1 glob-imports"),
        "{refusal:?}"
    );
    for unexpected in ["placement is unavailable", "first include limitation"] {
        assert!(!refusal.contains(unexpected), "{refusal:?}");
    }
    Ok(())
}

#[test]
fn a_public_glob_keeps_its_direct_explanation_with_an_include_present() -> Result<(), String> {
    // A `pub use ...::*;` is unconfined by construction, so no placement
    // decision can change it: naming an include would blame an unrelated
    // cause. The direct foreign-glob explanation stays.
    let repo = TempRepo::create(&[
        ("src/a.rs", "pub use rayon::prelude::*;\n"),
        ("src/b.rs", &own_unit()),
        ("src/gen.rs", NESTED),
        ("src/generated/g.rs", "mod y;\n"),
    ])?;
    let found = repo.finding()?;
    assert_eq!(found.class, ExposureClass::StaticUnknown);
    let refusal = only_refusal(&found)?;
    assert!(
        refusal.contains("`pub use rayon::prelude::*;` at src/a.rs:1 glob-imports"),
        "{refusal:?}"
    );
    for unexpected in ["placement is unavailable", "include limitation"] {
        assert!(!refusal.contains(unexpected), "{refusal:?}");
    }

    // A public glob is unconfined regardless of placement, so it keeps its
    // direct explanation instead of borrowing the include's.
    let public = TempRepo::create(&[
        (
            "src/a.rs",
            "pub use rayon::prelude::*;\npub fn twice(x: i32) -> i32 { x * 2 }\n",
        ),
        ("src/b.rs", &own_unit()),
        ("src/gen.rs", NESTED),
        ("src/generated/g.rs", "mod y;\n"),
    ])?
    .finding()?;
    assert_eq!(public.class, ExposureClass::StaticUnknown);
    let public = public
        .evidence
        .iter()
        .find(|line| line.starts_with("assertion not credited: "))
        .ok_or_else(|| "premise: the public glob keeps its refusal".to_string())?;
    assert!(
        public.contains("`pub use rayon::prelude::*;` at src/a.rs:1 glob-imports"),
        "{public:?}"
    );
    assert!(!public.contains("first include limitation"), "{public:?}");

    Ok(())
}

#[test]
fn an_unplaced_files_glob_keeps_its_direct_explanation() -> Result<(), String> {
    // `src/a.rs` is in no crate: even with placement on, its private glob
    // would not be confined, so the workspace include is not the cause of its
    // ambiguity and must not be named.
    let repo = TempRepo::create(&[
        ("src/lib.rs", "pub mod b;\npub mod gen;\n"),
        ("src/b.rs", &own_unit()),
        ("src/gen.rs", NESTED),
        ("src/generated/g.rs", "mod y;\n"),
    ])?;
    let found = repo.finding()?;
    assert_eq!(found.class, ExposureClass::StaticUnknown);
    let refusal = only_refusal(&found)?;
    assert!(
        refusal.contains("`use rayon::prelude::*;` at src/a.rs:1 glob-imports"),
        "{refusal:?}"
    );
    assert!(!refusal.contains("placement is unavailable"), "{refusal:?}");

    Ok(())
}

#[test]
fn a_glob_reaching_from_an_ancestor_keeps_its_direct_explanation() -> Result<(), String> {
    // The lib root globs a foreign crate and the test's module imports it
    // through `use super::*;`. With placement on the glob still reaches the
    // assertion, so the include is not why credit is withheld.
    let repo = TempRepo::create(&[
        ("src/a.rs", PLAIN),
        (
            "src/lib.rs",
            "use rayon::prelude::*;\npub mod b;\npub mod gen;\n",
        ),
        ("src/b.rs", &own_unit()),
        ("src/gen.rs", NESTED),
        ("src/generated/g.rs", "mod y;\n"),
    ])?;
    let found = repo.finding()?;
    assert_eq!(found.class, ExposureClass::StaticUnknown);
    let refusal = only_refusal(&found)?;
    assert!(
        refusal.contains("`use rayon::prelude::*;` at src/lib.rs:1 glob-imports"),
        "{refusal:?}"
    );
    assert!(!refusal.contains("first include limitation"), "{refusal:?}");

    Ok(())
}

#[test]
fn a_fragments_namesake_is_never_taken_for_the_owner() -> Result<(), String> {
    let library = |b: String| {
        vec![
            ("src/b.rs", b),
            ("src/gen.rs", NESTED.to_string()),
            ("src/generated/g.rs", NAMESAKE.to_string()),
        ]
    };
    let from_tests = |head: &str| {
        let mut files = library(OWNER.to_string());
        files.push(("tests/it.rs", integration(head)));
        files
    };
    check(
        &[
            (
                "a unit test importing the fragment's function",
                library(unit("    use crate::gen::inner::score;", "score(2)")),
            ),
            (
                "a unit test globbing the fragment's module",
                library(unit("    use crate::gen::inner::*;", "score(2)")),
            ),
            (
                "a unit test calling the fragment's function by path",
                library(unit("", "crate::gen::inner::score(2)")),
            ),
            (
                "an integration test importing the fragment's function",
                from_tests("use demo::gen::inner::score;"),
            ),
            (
                "an integration test globbing the fragment's module",
                from_tests("use demo::gen::inner::*;"),
            ),
        ],
        &ExposureClass::WeaklyExposed,
    )?;
    // Control: the same library, with the test importing the owner itself.
    check(
        &[(
            "an integration test importing the owner",
            from_tests("use demo::b::score;"),
        )],
        &ExposureClass::Exposed,
    )
}
