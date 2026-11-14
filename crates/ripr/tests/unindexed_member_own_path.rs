//! Production-path witness for #14260: in draft mode a workspace member
//! outside the index that reaches its own files through `#[path]` does not
//! switch placement off for the indexed package. Every shape that could
//! compile an indexed file (the #10149 doubt) still does. Static evidence
//! only: the native runs recorded on the issue are the execution proof.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const SUPPORT: &str = "pub fn value() -> u32 {\n    7\n}\n";
const OWN_PATH: &str = "#[path = \"../support.rs\"]\nmod support;\n";

/// The finding for `app`'s changed `x * 4` (was `x * 3`). `tool` is a member
/// that does not depend on `app`, so a draft index leaves it out; `files` are
/// written under `tool/`. `app/src/par.rs` holds a private foreign glob that
/// is confined to its own module only while placement is available.
fn finding(mode: Mode, files: &[(&str, &str)]) -> Result<Finding, String> {
    finding_with_links(mode, files, &[])
}

/// [`finding`] plus symbolic links `(path under tool/, target under the
/// workspace root)`, which only Unix can create.
fn finding_with_links(
    mode: Mode,
    files: &[(&str, &str)],
    links: &[(&str, &str)],
) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-unindexed-member-own-path-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        let parent = path
            .parent()
            .ok_or_else(|| format!("no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|error| format!("dir {path:?}: {error}"))?;
        std::fs::write(&path, text).map_err(|error| format!("write {path:?}: {error}"))
    };
    write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"app\", \"tool\"]\nexclude = [\"rayon\"]\nresolver = \"2\"\n",
    )?;
    // A path dependency outside the workspace: ripr does not index it, and
    // it is not the registry `rayon`.
    write(
        "rayon/Cargo.toml",
        "[package]\nname = \"rayon\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )?;
    write(
        "rayon/src/lib.rs",
        "pub mod prelude {\n    pub struct Unrelated;\n}\n",
    )?;
    write(
        "app/Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nrayon = { path = \"../rayon\" }\n",
    )?;
    write(
        "app/src/lib.rs",
        "mod par;\nmod suite;\n\npub fn weight(x: u32) -> u32 {\n    x * 4\n}\n",
    )?;
    write(
        "app/src/par.rs",
        "use rayon::prelude::*;\n\npub fn noop() {}\n",
    )?;
    write(
        "app/src/suite.rs",
        "#[test]\nfn weighs() {\n    assert_eq!(crate::weight(4), 16);\n}\n",
    )?;
    write(
        "tool/Cargo.toml",
        "[package]\nname = \"tool\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )?;
    for (path, text) in files {
        write(&format!("tool/{path}"), text)?;
    }
    // On Windows the body always returns, so clippy sees a loop that never
    // loops; on Unix it iterates every link. The cfg split is intentional.
    #[cfg_attr(
        not(unix),
        expect(
            clippy::never_loop,
            reason = "the cfg(not(unix)) body always returns, so the loop only iterates on Unix"
        )
    )]
    for (link, target) in links {
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join(target), root.join("tool").join(link))
            .map_err(|error| format!("link {link}: {error}"))?;
        #[cfg(not(unix))]
        return Err(format!("symlink {link} -> {target} needs Unix"));
    }
    let diff = "diff --git a/app/src/lib.rs b/app/src/lib.rs\nindex 0000000..1111111 100644\n--- a/app/src/lib.rs\n+++ b/app/src/lib.rs\n@@ -3,4 +3,4 @@\n \n pub fn weight(x: u32) -> u32 {\n-    x * 3\n+    x * 4\n }\n";
    write("diff.patch", diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == 5 && finding.probe.expression.contains("x * 4")
        })
        .ok_or_else(|| "no finding for the changed `x * 4`".to_string())
}

fn refusals(finding: &Finding) -> Vec<&str> {
    finding
        .evidence
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with("assertion not credited: "))
        .collect()
}

fn assert_credited(label: &str, found: &Finding) {
    assert_eq!(
        found.class,
        ExposureClass::Exposed,
        "{label}: {:?}",
        found.evidence
    );
    assert_eq!(refusals(found), Vec::<&str>::new(), "{label}");
}

/// Placement is off, so the glob in another module of `app` blocks credit.
fn assert_withheld(label: &str, found: &Finding) {
    assert_ne!(found.class, ExposureClass::Exposed, "{label}");
    let refusals = refusals(found);
    assert_eq!(refusals.len(), 1, "{label}: {:?}", found.evidence);
    assert!(
        refusals[0].contains("`use rayon::prelude::*;` at app/src/par.rs:1 glob-imports"),
        "{label}: {refusals:?}"
    );
}

#[test]
fn a_member_reaching_its_own_file_through_path_keeps_draft_placement() -> Result<(), String> {
    // Control: the same member with the default module layout.
    let plain = [
        ("src/lib.rs", "mod support;\n"),
        ("src/support.rs", SUPPORT),
    ];
    assert_credited("plain mod, draft", &finding(Mode::Draft, &plain)?);

    let own = [("src/lib.rs", OWN_PATH), ("support.rs", SUPPORT)];
    assert_credited("own #[path], draft", &finding(Mode::Draft, &own)?);
    // Ready indexes `tool` and already agreed.
    assert_credited("own #[path], ready", &finding(Mode::Ready, &own)?);

    // An ordinary `cfg_attr` and a `std::path` import are not a path override.
    let benign = [
        (
            "src/lib.rs",
            "use std::path::PathBuf;\n#[cfg_attr(test, derive(Debug))]\nstruct Kept(PathBuf);\n#[path = \"../support.rs\"]\nmod support;\n",
        ),
        ("support.rs", SUPPORT),
    ];
    assert_credited("benign cfg_attr, draft", &finding(Mode::Draft, &benign)?);

    // A chain of own files, and a `build.rs` root, are still own files.
    let chain = [
        ("src/lib.rs", OWN_PATH),
        ("support.rs", "#[path = \"deep/leaf.rs\"]\nmod leaf;\n"),
        ("deep/leaf.rs", SUPPORT),
        (
            "build.rs",
            "#[path = \"support.rs\"]\nmod support;\nfn main() {}\n",
        ),
    ];
    assert_credited("own #[path] chain, draft", &finding(Mode::Draft, &chain)?);

    // Explicit target paths are crate roots, not followed files: many of them
    // must not exhaust the walk's budget of path-named files.
    let mut manifest =
        String::from("[package]\nname = \"tool\"\nversion = \"0.0.0\"\nedition = \"2021\"\n");
    let names = (0..70).map(|index| format!("t{index}")).collect::<Vec<_>>();
    for name in &names {
        manifest.push_str(&format!(
            "\n[[test]]\nname = \"{name}\"\npath = \"checks/{name}.rs\"\n"
        ));
    }
    let mut many: Vec<(String, String)> = vec![
        ("Cargo.toml".to_string(), manifest),
        ("src/lib.rs".to_string(), OWN_PATH.to_string()),
        ("support.rs".to_string(), SUPPORT.to_string()),
    ];
    many.extend(names.iter().map(|name| {
        (
            format!("checks/{name}.rs"),
            "#[test]\nfn ok() {}\n".to_string(),
        )
    }));
    let many = many
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    assert_credited("70 explicit targets, draft", &finding(Mode::Draft, &many)?);
    Ok(())
}

#[test]
fn a_member_that_may_compile_an_indexed_file_still_withholds_draft_placement() -> Result<(), String>
{
    let lib = |text: &'static str| [("src/lib.rs", text), ("support.rs", SUPPORT)];
    let cases: [(&str, Vec<(&str, &str)>); 13] = [
        (
            "path leaves the member",
            lib("#[path = \"../../app/src/par.rs\"]\nmod par;\n").to_vec(),
        ),
        (
            "include! in the root",
            lib("include!(\"../support.rs\");\n").to_vec(),
        ),
        ("path target is missing", vec![("src/lib.rs", OWN_PATH)]),
        (
            "own file carries include!",
            vec![
                ("src/lib.rs", OWN_PATH),
                ("support.rs", "include!(\"../app/src/par.rs\");\n"),
            ],
        ),
        (
            "own file's path leaves the member",
            vec![
                ("src/lib.rs", OWN_PATH),
                ("support.rs", "#[path = \"../app/src/par.rs\"]\nmod par;\n"),
            ],
        ),
        (
            "path nested in an inline module",
            lib("mod inner {\n    #[path = \"../support.rs\"]\n    mod support;\n}\n").to_vec(),
        ),
        (
            "path is not a plain literal",
            lib("#[path = concat!(\"../\", \"support.rs\")]\nmod support;\n").to_vec(),
        ),
        (
            "own file declares a default child that mounts a file",
            vec![
                ("src/lib.rs", OWN_PATH),
                ("support.rs", "mod child;\n"),
                ("child.rs", "include!(\"../app/src/par.rs\");\n"),
            ],
        ),
        (
            "own file's cfg_attr introduces a path",
            vec![
                ("src/lib.rs", OWN_PATH),
                (
                    "support.rs",
                    "#[cfg_attr(test, path = \"../app/src/par.rs\")]\nmod par;\n",
                ),
            ],
        ),
        (
            "own file's raw-identifier path",
            vec![
                ("src/lib.rs", OWN_PATH),
                (
                    "support.rs",
                    "#[r#path = \"../app/src/par.rs\"]\nmod par;\n",
                ),
            ],
        ),
        (
            "own path beside a default child that mounts a file",
            vec![
                (
                    "src/lib.rs",
                    "#[path = \"../support.rs\"]\nmod support;\nmod child;\n",
                ),
                ("support.rs", SUPPORT),
                (
                    "src/child.rs",
                    "#[path = \"../../app/src/par.rs\"]\nmod par;\n",
                ),
            ],
        ),
        (
            "own path beside an auto-discovered binary that mounts a file",
            vec![
                ("src/lib.rs", OWN_PATH),
                ("support.rs", SUPPORT),
                (
                    "src/bin/other.rs",
                    "#[path = \"../../../app/src/par.rs\"]\nmod par;\n",
                ),
            ],
        ),
        (
            "inner path attribute",
            lib("#![path = \"../support.rs\"]\n").to_vec(),
        ),
    ];
    for (label, files) in cases {
        assert_withheld(label, &finding(Mode::Draft, &files)?);
    }

    // A chain of path-named files past the walk's budget is read as a mount.
    let names = (0..70)
        .map(|index| format!("p{index}.rs"))
        .collect::<Vec<_>>();
    let mut chain: Vec<(String, String)> = vec![
        ("src/lib.rs".to_string(), OWN_PATH.to_string()),
        (
            "support.rs".to_string(),
            format!("#[path = \"{}\"]\nmod hop;\n", names[0]),
        ),
    ];
    for (index, name) in names.iter().enumerate() {
        let body = match names.get(index + 1) {
            Some(next) => format!("#[path = \"{next}\"]\nmod hop;\n"),
            None => SUPPORT.to_string(),
        };
        chain.push((name.clone(), body));
    }
    let chain = chain
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    assert_withheld("path chain past the budget", &finding(Mode::Draft, &chain)?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_crate_root_linked_to_an_indexed_file_withholds_draft_placement() -> Result<(), String> {
    let files = [("src/lib.rs", OWN_PATH), ("support.rs", SUPPORT)];
    let found = finding_with_links(Mode::Draft, &files, &[("src/main.rs", "app/src/par.rs")])?;
    assert_withheld("root symlinked to an indexed file", &found);
    Ok(())
}
