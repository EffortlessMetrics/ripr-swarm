use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{
    COMMAND_LINE_BUDGET, RustfmtInvocation, argument_length, batch, canonical_or_keep,
    command_line_length, formatter, invocations, metadata_arguments, read_metadata,
    read_metadata_through, run_all, serving_cargo,
};

/// The `cargo` a test asks for metadata, found as the step finds it.
fn serving_cargo_for_test() -> std::ffi::OsString {
    serving_cargo(
        std::env::var_os("CARGO").as_deref(),
        std::env::var_os("PATH").as_deref(),
    )
    .map_or_else(|| std::ffi::OsString::from("cargo"), Into::into)
}

/// The workspace root of a `cargo metadata --no-deps` document.
fn workspace_root_of(document: &Value) -> Result<PathBuf, String> {
    document["workspace_root"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| "cargo metadata reports no workspace root".to_string())
}

/// One workspace package as `cargo metadata --no-deps` reports it. Each
/// target is `(kind, src path)`; a target may name its own edition.
fn package(directory: &str, edition: &str, targets: &[(&str, &str)]) -> Value {
    json!({
        "id": format!("{directory} 0.1.0 (path+file:///repo/{directory})"),
        "edition": edition,
        "manifest_path": format!("/repo/{directory}/Cargo.toml"),
        "targets": targets
            .iter()
            .map(|(kind, src_path)| json!({ "kind": [kind], "src_path": src_path }))
            .collect::<Vec<_>>(),
    })
}

fn metadata(packages: Vec<Value>) -> Value {
    json!({ "workspace_root": "/repo", "packages": packages })
}

fn roots_of(invocation: &RustfmtInvocation) -> &[String] {
    &invocation.argv[invocation.argv.len() - invocation.root_count..]
}

fn planned_roots(plan: &[RustfmtInvocation]) -> BTreeSet<String> {
    plan.iter()
        .flat_map(|invocation| roots_of(invocation).iter().cloned())
        .collect()
}

/// A two-member workspace: `old` on edition 2021 and `new` on 2024.
fn two_members() -> Value {
    metadata(vec![
        package("old", "2021", &[("lib", "/repo/old/src/lib.rs")]),
        package(
            "new",
            "2024",
            &[
                ("bin", "/repo/new/src/main.rs"),
                ("test", "/repo/new/tests/a.rs"),
            ],
        ),
    ])
}

/// The reported shape of this workspace (#12004): one package with more than
/// four hundred integration-test roots under a checkout of ordinary depth:
/// 425 roots of 76 characters, about 33,550 on one line.
fn ripr_roots() -> Vec<String> {
    const ROOT_LENGTH: usize = 76;
    const PREFIX: &str = "\\\\?\\E:\\Code\\zcode\\Rust\\ripr-swarm\\crates\\ripr\\tests\\";
    assert!(ROOT_LENGTH > PREFIX.len() + 3 + "case_number_000".len());
    (0..425)
        .map(|index| {
            let stem = format!("case_number_{index:03}");
            let padding = "_".repeat(ROOT_LENGTH - PREFIX.len() - stem.len() - 3);
            format!("{PREFIX}{stem}{padding}.rs")
        })
        .collect()
}

#[test]
fn unbatched_the_reported_shape_exceeds_the_windows_command_line() {
    // Windows `CreateProcess` refuses a command line longer than 32,767
    // characters (os error 206), which is what `cargo fmt --check` meets here.
    assert!(ripr_roots().iter().all(|root| root.len() == 76));
    let mut one_line = vec![
        "--edition".to_owned(),
        "2024".to_owned(),
        "--check".to_owned(),
    ];
    one_line.extend(ripr_roots());
    assert!(command_line_length(&one_line) > 32_767);
}

#[test]
fn batches_keep_every_command_line_within_the_budget() {
    let head = ["--edition", "2024", "--check"].map(String::from);
    let roots = ripr_roots();
    let batches = batch(&head, &roots);
    assert!(batches.len() > 1, "one line would exceed the budget");
    for invocation in &batches {
        assert!(command_line_length(&invocation.argv) <= COMMAND_LINE_BUDGET);
        assert_eq!(invocation.root_count, roots_of(invocation).len());
    }
    let covered = batches
        .iter()
        .flat_map(|invocation| roots_of(invocation).iter())
        .collect::<Vec<_>>();
    // Every root exactly once, in the listed order.
    assert_eq!(covered, roots.iter().collect::<Vec<_>>());
}

#[test]
fn a_root_longer_than_the_budget_still_runs_alone() {
    let head = ["--edition", "2024"].map(String::from);
    let long = "x".repeat(COMMAND_LINE_BUDGET + 50);
    let roots = ["short.rs".to_owned(), long.clone(), "tail.rs".to_owned()];
    let batches = batch(&head, &roots);
    assert_eq!(batches.len(), 3);
    assert_eq!(roots_of(&batches[0]), ["short.rs"].as_slice());
    assert_eq!(roots_of(&batches[1]), [long.as_str()].as_slice());
    assert_eq!(roots_of(&batches[2]), ["tail.rs"].as_slice());
}

#[test]
fn every_argument_is_charged_a_space_and_two_quotes() {
    // Rust quotes an argument that holds a space; charging both for every
    // argument keeps the estimate an upper bound.
    assert_eq!(argument_length("src/lib.rs"), 3 + "src/lib.rs".len());
    assert_eq!(argument_length("two words"), 3 + "two words".len());
    // The Windows limit counts UTF-16 code units, not bytes or chars.
    assert_eq!(argument_length("\u{1F600}"), 3 + 2);
}

#[test]
fn an_empty_root_list_runs_nothing() {
    let head = ["--edition", "2024"].map(String::from);
    assert!(batch(&head, &[]).is_empty());
}

/// The edition each run names, in plan order.
fn editions(plan: &[RustfmtInvocation]) -> Result<Vec<&str>, String> {
    plan.iter()
        .map(|invocation| {
            invocation
                .argv
                .iter()
                .position(|argument| argument == "--edition")
                .and_then(|edition| invocation.argv.get(edition + 1))
                .map(String::as_str)
                .ok_or_else(|| format!("run names no edition: {:?}", invocation.argv))
        })
        .collect()
}

#[test]
fn each_edition_runs_rustfmt_once_per_batch_and_check_is_optional() -> Result<(), String> {
    let checked = invocations(&two_members(), true, Path::new("/repo"))?;
    assert_eq!(editions(&checked)?, ["2021", "2024"]);
    for invocation in &checked {
        assert!(
            invocation
                .argv
                .last()
                .is_some_and(|arg| arg.ends_with(".rs"))
        );
        // `--check` is an option, so it precedes the roots.
        let check = invocation
            .argv
            .iter()
            .position(|arg| arg == "--check")
            .ok_or_else(|| format!("run carries no --check: {:?}", invocation.argv))?;
        assert!(check < invocation.argv.len() - invocation.root_count);
    }
    let written = invocations(&two_members(), false, Path::new("/repo"))?;
    assert_eq!(editions(&written)?, ["2021", "2024"]);
    assert!(
        written
            .iter()
            .all(|invocation| !invocation.argv.iter().any(|arg| arg == "--check"))
    );
    Ok(())
}

/// cargo-fmt scopes `cargo fmt` without `--all` by the invocation directory:
/// every member from the workspace root, the member under the invocation
/// directory inside one, nothing elsewhere.
#[test]
fn the_invocation_directory_decides_which_members_are_formatted() -> Result<(), String> {
    let every = invocations(&two_members(), true, Path::new("/repo"))?;
    assert_eq!(
        planned_roots(&every),
        BTreeSet::from([
            "/repo/new/src/main.rs".to_owned(),
            "/repo/new/tests/a.rs".to_owned(),
            "/repo/old/src/lib.rs".to_owned(),
        ])
    );
    let member = invocations(&two_members(), true, Path::new("/repo/new"))?;
    assert_eq!(
        planned_roots(&member),
        BTreeSet::from([
            "/repo/new/src/main.rs".to_owned(),
            "/repo/new/tests/a.rs".to_owned(),
        ])
    );
    let other = invocations(&two_members(), true, Path::new("/repo/old"))?;
    assert_eq!(
        planned_roots(&other),
        BTreeSet::from(["/repo/old/src/lib.rs".to_owned()])
    );
    // Inside a member, but below its manifest, or outside the workspace: no
    // package is selected, and cargo-fmt refuses ("Failed to find targets")
    // rather than widening.
    for elsewhere in ["/repo/new/src", "/repo/new/tests", "/elsewhere"] {
        let refused = invocations(&two_members(), true, Path::new(elsewhere));
        assert_eq!(
            refused,
            Err(format!(
                "cargo fmt finds no target from {elsewhere}: run it from the workspace root \
                 or from a member directory"
            )),
            "{elsewhere}"
        );
    }
    // A workspace of one package is that package wherever the caller stands.
    let single = metadata(vec![package(
        "one",
        "2024",
        &[("lib", "/repo/one/src/lib.rs")],
    )]);
    let plan = invocations(&single, true, Path::new("/elsewhere"))?;
    assert_eq!(
        planned_roots(&plan),
        BTreeSet::from(["/repo/one/src/lib.rs".to_owned()])
    );
    Ok(())
}

/// A member with no target is not a package to format.
#[test]
fn a_member_without_targets_refuses() {
    let document = metadata(vec![
        package("empty", "2024", &[]),
        package("full", "2024", &[("lib", "/repo/full/src/lib.rs")]),
    ]);
    assert_eq!(
        invocations(&document, true, Path::new("/repo/empty")),
        Err(
            "cargo fmt finds no target from /repo/empty: run it from the workspace root or \
             from a member directory"
                .to_string()
        )
    );
}

#[test]
fn roots_are_every_target_whatever_its_kind_or_extension() -> Result<(), String> {
    // cargo-fmt reads each target's kind for its verbose listing only, and
    // filters no root by extension: whatever cargo reports goes to rustfmt.
    let names = [
        "lib",
        "bin",
        "example",
        "test",
        "bench",
        "custom-build",
        "proc-macro",
        // A kind this code does not know: cargo-fmt hands it over too.
        "metabuild",
    ];
    let targets: Vec<Value> = names
        .iter()
        .map(|kind| json!({ "kind": [kind], "src_path": format!("/repo/kinds/{kind}.rs") }))
        .chain([json!({ "kind": ["bin"], "src_path": "/repo/kinds/main.c" })])
        .collect();
    let document = metadata(vec![json!({
        "id": "kinds 0.1.0 (path+file:///repo/kinds)",
        "edition": "2024",
        "manifest_path": "/repo/kinds/Cargo.toml",
        "targets": targets,
    })]);
    let roots = planned_roots(&invocations(&document, true, Path::new("/repo"))?);
    let expected = names
        .iter()
        .map(|kind| format!("/repo/kinds/{kind}.rs"))
        .chain(["/repo/kinds/main.c".to_owned()])
        .collect::<BTreeSet<_>>();
    assert_eq!(roots, expected);
    Ok(())
}

/// cargo-fmt keys its target set by path alone, so the first target that names
/// a path decides the edition of its only run.
#[test]
fn a_path_two_targets_name_runs_once_under_the_first_targets_edition() -> Result<(), String> {
    let document = metadata(vec![json!({
        "id": "shared 0.1.0 (path+file:///repo/shared)",
        "edition": "2024",
        "manifest_path": "/repo/shared/Cargo.toml",
        "targets": [
            { "kind": ["test"], "src_path": "/repo/shared/tests/shared.rs", "edition": "2021" },
            { "kind": ["bin"], "src_path": "/repo/shared/tests/shared.rs", "edition": "2024" },
        ],
    })]);
    let plan = invocations(&document, true, Path::new("/repo"))?;
    let mut runs = Vec::new();
    for invocation in &plan {
        runs.push((
            editions(std::slice::from_ref(invocation))?[0].to_owned(),
            roots_of(invocation).to_vec(),
        ));
    }
    assert_eq!(
        runs,
        [(
            "2021".to_owned(),
            vec!["/repo/shared/tests/shared.rs".to_owned()]
        )]
    );
    Ok(())
}

/// cargo-fmt runs every edition group before it reports the first failure, so
/// one `--check` run shows every diff. A failed batch hides no later batch.
#[test]
fn every_batch_runs_before_the_first_failure_is_reported() -> Result<(), String> {
    fn batch_of(argument: &str) -> RustfmtInvocation {
        RustfmtInvocation {
            argv: vec![argument.to_owned()],
            root_count: 0,
        }
    }
    let good = || batch_of("--version");
    let bad = || batch_of("--ripr-not-a-cargo-flag");
    assert_eq!(run_all("cargo", &[], &[], None), Ok(()));
    assert_eq!(run_all("cargo", &[good()], &[], None), Ok(()));
    assert_eq!(run_all("cargo", &[good(), good()], &[], None), Ok(()));
    for order in [[bad(), good()], [good(), bad()]] {
        let mut diagnostics = Vec::new();
        let Err(error) = run_all("cargo", &order, &[], Some(&mut diagnostics)) else {
            return Err("a failed batch must fail the step".to_owned());
        };
        assert!(
            error.starts_with(
                "cargo --ripr-not-a-cargo-flag (0 target roots; paths elided) failed with"
            ),
            "{error}"
        );
        assert_eq!(diagnostics.len(), 2, "both batches ran");
        // A recorded argument list carries the stand-in for the root paths
        // and says so; it is not read as what rustfmt ran with.
        assert!(
            diagnostics.iter().all(|row| row
                .argv
                .last()
                .is_some_and(|arg| arg.ends_with("paths elided)"))),
            "{diagnostics:?}"
        );
        let outcomes = diagnostics
            .iter()
            .map(|row| row.outcome)
            .collect::<Vec<_>>();
        let ran = order
            .iter()
            .map(|invocation| {
                if invocation.argv[0] == "--version" {
                    "pass"
                } else {
                    "failed"
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(outcomes, ran, "{error}");
    }
    let Err(error) = run_all("cargo", &[bad(), good(), bad()], &[], None) else {
        return Err("two failed batches must fail the step".to_owned());
    };
    assert!(error.ends_with("; 2 batches failed"), "{error}");
    Ok(())
}

/// cargo-fmt groups by each target's own edition, which defaults to its
/// package's.
#[test]
fn a_target_edition_overrides_its_package_edition() -> Result<(), String> {
    let document = json!({
        "workspace_root": "/repo",
        "packages": [{
            "id": "mixed 0.1.0 (path+file:///repo/mixed)",
            "edition": "2024",
            "manifest_path": "/repo/mixed/Cargo.toml",
            "targets": [
                { "kind": ["lib"], "src_path": "/repo/mixed/src/lib.rs", "edition": "2024" },
                { "kind": ["test"], "src_path": "/repo/mixed/tests/old.rs", "edition": "2021" },
                { "kind": ["test"], "src_path": "/repo/mixed/tests/none.rs" },
            ],
        }],
    });
    let plan = invocations(&document, true, Path::new("/repo"))?;
    let mut by_root = Vec::new();
    for invocation in &plan {
        let edition = editions(std::slice::from_ref(invocation))?[0];
        for root in roots_of(invocation) {
            by_root.push((root.clone(), edition.to_owned()));
        }
    }
    assert_eq!(
        by_root,
        [
            ("/repo/mixed/tests/old.rs".to_owned(), "2021".to_owned()),
            ("/repo/mixed/src/lib.rs".to_owned(), "2024".to_owned()),
            ("/repo/mixed/tests/none.rs".to_owned(), "2024".to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn a_target_whose_package_reports_no_edition_is_refused() {
    let document = json!({
        "workspace_root": "/repo",
        "packages": [{
            "id": "odd",
            "edition": "",
            "manifest_path": "/repo/odd/Cargo.toml",
            "targets": [{ "kind": ["lib"], "src_path": "/repo/odd/src/lib.rs" }],
        }],
    });
    assert_eq!(
        invocations(&document, true, Path::new("/repo")),
        Err("target /repo/odd/src/lib.rs reports no edition".to_string())
    );
}

/// cargo-fmt passes no config: rustfmt finds the nearest `rustfmt.toml` or
/// `.rustfmt.toml` for each file, so a member's own config keeps applying.
#[test]
fn no_config_path_is_passed_so_rustfmt_finds_each_files_nearest_config() -> Result<(), String> {
    let plan = invocations(&two_members(), true, Path::new("/repo"))?;
    assert!(!plan.is_empty());
    assert!(
        plan.iter()
            .all(|invocation| !invocation.argv.iter().any(|arg| arg.contains("config"))),
        "{plan:?}"
    );
    Ok(())
}

/// cargo-fmt runs `RUSTFMT` when it is set, else the rustfmt beside the
/// `cargo` that serves the step: the one `CARGO` names, else the first one on
/// `PATH`. A different `rustfmt` earlier on `PATH`, or a later directory's
/// toolchain, must not be picked up.
#[test]
fn the_formatter_is_rustfmt_env_else_the_rustfmt_beside_cargo() -> Result<(), String> {
    fn named(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        }
    }
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-workspace-format-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos()
    ));
    let toolchain = root.join("toolchain");
    let other = root.join("other-toolchain");
    let shadow = root.join("shadow");
    let bare = root.join("bare-cargo");
    for directory in [&toolchain, &other, &shadow, &bare] {
        std::fs::create_dir_all(directory).map_err(|err| err.to_string())?;
    }
    for file in [
        toolchain.join(named("cargo")),
        toolchain.join(named("rustfmt")),
        other.join(named("cargo")),
        other.join(named("rustfmt")),
        shadow.join(named("rustfmt")),
        bare.join(named("cargo")),
    ] {
        std::fs::write(&file, "").map_err(|err| err.to_string())?;
    }
    let os = |path: &Path| path.to_string_lossy().into_owned();
    let cargo = |directory: &Path| directory.join(named("cargo")).into_os_string();
    let missing = |directory: &Path| {
        Err(format!(
            "no rustfmt sits beside the cargo that serves this step ({}); install the rustfmt \
             component for that toolchain, or set RUSTFMT to the formatter to run",
            directory.join(named("cargo")).display()
        ))
    };

    // PATH-shadowing control: the shadowing rustfmt comes first on PATH, and
    // the first cargo's own rustfmt is still the one run.
    let path = std::env::join_paths([os(&shadow), os(&toolchain), os(&other), os(&bare)])
        .map_err(|err| err.to_string())?;
    assert_eq!(
        serving_cargo(None, Some(path.as_os_str())),
        Some(toolchain.join(named("cargo")))
    );
    assert_eq!(
        formatter(
            None,
            serving_cargo(None, Some(path.as_os_str()))
                .as_deref()
                .map(Path::as_os_str)
        ),
        Ok(toolchain.join(named("rustfmt")))
    );

    // CARGO names the cargo that serves the step, wherever it sits on PATH.
    assert_eq!(
        serving_cargo(Some(&cargo(&other)), Some(path.as_os_str())),
        Some(other.join(named("cargo")))
    );
    assert_eq!(
        formatter(None, Some(&cargo(&other))),
        Ok(other.join(named("rustfmt")))
    );
    // An empty CARGO names none, so PATH decides.
    assert_eq!(
        serving_cargo(Some(std::ffi::OsStr::new("")), Some(path.as_os_str())),
        Some(toolchain.join(named("cargo")))
    );

    // The first cargo on PATH has no rustfmt beside it: its formatter is
    // missing, and no later toolchain's is borrowed.
    let bare_first =
        std::env::join_paths([os(&bare), os(&toolchain)]).map_err(|err| err.to_string())?;
    assert_eq!(
        formatter(
            None,
            serving_cargo(None, Some(bare_first.as_os_str()))
                .as_deref()
                .map(Path::as_os_str)
        ),
        missing(&bare)
    );
    // Nor is one borrowed beside a CARGO that has none.
    assert_eq!(formatter(None, Some(&cargo(&bare))), missing(&bare));

    // RUSTFMT names the formatter outright; an empty RUSTFMT is read as unset.
    assert_eq!(
        formatter(
            Some(std::ffi::OsStr::new("/opt/other/rustfmt")),
            Some(&cargo(&toolchain))
        ),
        Ok(PathBuf::from("/opt/other/rustfmt"))
    );
    assert_eq!(
        formatter(Some(std::ffi::OsStr::new("")), Some(&cargo(&toolchain))),
        Ok(toolchain.join(named("rustfmt")))
    );

    // No cargo at all serves the step and no RUSTFMT is set.
    assert_eq!(serving_cargo(None, None), None);
    let Err(error) = formatter(None, None) else {
        return Err("no cargo and no RUSTFMT must not name a formatter".to_owned());
    };
    assert!(
        error.contains("set RUSTFMT"),
        "the remedy names the override: {error}"
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// cargo-fmt asks for the metadata offline first, and once more without
/// `--offline` only when that fails.
#[test]
fn metadata_is_asked_for_offline_first_and_retried_without_it() -> Result<(), String> {
    let words = |list: &[&str]| {
        list.iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        metadata_arguments(true),
        words(&[
            "metadata",
            "--no-deps",
            "--offline",
            "--format-version",
            "1"
        ])
    );
    assert_eq!(
        metadata_arguments(false),
        words(&["metadata", "--no-deps", "--format-version", "1"])
    );
    let document = r#"{"workspace_root":"/repo","packages":[]}"#;

    // The offline question answers: it is the only one asked.
    let mut asked = Vec::new();
    let answered = read_metadata_through("cargo", &mut |offline| {
        asked.push(offline);
        Ok(document.to_owned())
    })?;
    assert_eq!(asked, [true]);
    assert_eq!(answered["workspace_root"], "/repo");

    // The offline question fails: the same question is asked once more, online.
    let mut asked = Vec::new();
    let retried = read_metadata_through("cargo", &mut |offline| {
        asked.push(offline);
        if offline {
            Err("needs the index".to_owned())
        } else {
            Ok(document.to_owned())
        }
    })?;
    assert_eq!(asked, [true, false]);
    assert_eq!(retried["workspace_root"], "/repo");

    // Both fail: the failure names both.
    let mut asked = Vec::new();
    let Err(error) = read_metadata_through("cargo", &mut |offline| {
        asked.push(offline);
        Err(format!("attempt {offline}"))
    }) else {
        return Err("two failed questions must not read a document".to_owned());
    };
    assert_eq!(asked, [true, false]);
    assert!(
        error.contains("failed offline (attempt true)")
            && error.contains("without --offline (attempt false)"),
        "{error}"
    );

    // Output that is not a document is refused.
    let Err(error) = read_metadata_through("cargo", &mut |_| Ok("not a document".to_owned()))
    else {
        return Err("text that is not JSON must not read as a document".to_owned());
    };
    assert!(
        error.starts_with("cargo metadata --no-deps printed no JSON document"),
        "{error}"
    );
    Ok(())
}

/// A formatter that cannot be started stops the step at that batch, as
/// cargo-fmt stops there: no later batch is started against it.
#[test]
fn a_formatter_that_cannot_start_stops_the_step() -> Result<(), String> {
    fn invocation(argument: &str) -> RustfmtInvocation {
        RustfmtInvocation {
            argv: vec![argument.to_owned()],
            root_count: 0,
        }
    }
    let mut diagnostics = Vec::new();
    let Err(error) = run_all(
        "ripr-no-such-formatter",
        &[invocation("a"), invocation("b"), invocation("c")],
        &[],
        Some(&mut diagnostics),
    ) else {
        return Err("a formatter that cannot start must fail the step".to_owned());
    };
    assert!(
        error.starts_with("failed to run ripr-no-such-formatter:"),
        "{error}"
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "the batches after the one that could not start were not run: {diagnostics:?}"
    );
    assert_eq!(diagnostics[0].outcome, "status_call_unavailable");
    Ok(())
}

/// Two members with different formatting: run from one member's directory,
/// the step formats that member and leaves the other as it was, as `cargo fmt`
/// does. The document is the one `cargo metadata` reports for that workspace.
#[test]
fn from_a_member_directory_only_that_member_is_rewritten() -> Result<(), String> {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-workspace-format-members-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos()
    ));
    for member in ["alpha", "beta"] {
        let directory = root.join(member).join("src");
        std::fs::create_dir_all(&directory).map_err(|err| err.to_string())?;
        std::fs::write(
            root.join(member).join("Cargo.toml"),
            format!("[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .map_err(|err| err.to_string())?;
        std::fs::write(directory.join("lib.rs"), "pub fn one( ) -> u8 { 1 }\n")
            .map_err(|err| err.to_string())?;
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"alpha\", \"beta\"]\nresolver = \"2\"\n",
    )
    .map_err(|err| err.to_string())?;
    let output = crate::run::capture_output_in_dir(
        "cargo",
        &[
            "metadata".to_owned(),
            "--no-deps".to_owned(),
            "--format-version".to_owned(),
            "1".to_owned(),
        ],
        &root,
        "cargo metadata",
    )?;
    assert!(output.status.success(), "cargo metadata: {}", output.stderr);
    let document: Value = serde_json::from_str(&output.stdout)
        .map_err(|err| format!("cargo metadata printed no JSON document: {err}"))?;
    let workspace_root = workspace_root_of(&document)?;

    // From the workspace root both members; from inside one member that
    // member alone; below its manifest, nothing.
    let every = planned_roots(&invocations(&document, false, &workspace_root)?);
    assert_eq!(every.len(), 2, "{every:?}");
    let plan = invocations(&document, false, &workspace_root.join("alpha"))?;
    let roots = planned_roots(&plan);
    assert_eq!(
        roots.iter().cloned().collect::<Vec<_>>(),
        [canonical_or_keep(&root.join("alpha").join("src/lib.rs"))
            .to_string_lossy()
            .into_owned()],
        "{roots:?}"
    );
    assert!(
        invocations(&document, false, &workspace_root.join("alpha").join("src")).is_err(),
        "below a member's manifest there is nothing to format"
    );
    let program = formatter(None, Some(&serving_cargo_for_test()))?;
    for invocation in &plan {
        let args = invocation
            .argv
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let output = crate::run::capture_output(&program.to_string_lossy(), &args, "rustfmt")?;
        assert!(
            output.status.success(),
            "rustfmt {args:?} exited {}: {}",
            output.status,
            output.stderr
        );
    }
    let alpha = std::fs::read_to_string(root.join("alpha").join("src/lib.rs"))
        .map_err(|err| err.to_string())?;
    let beta = std::fs::read_to_string(root.join("beta").join("src/lib.rs"))
        .map_err(|err| err.to_string())?;
    assert_eq!(
        alpha, "pub fn one() -> u8 {\n    1\n}\n",
        "alpha was formatted"
    );
    assert_eq!(
        beta, "pub fn one( ) -> u8 { 1 }\n",
        "beta was left as it was"
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// In this repository the `xtask` alias runs from `xtask/` as readily as from
/// the root. There the step is xtask's own; from the root it is every
/// member's; from a directory that is neither it refuses.
#[test]
fn in_this_workspace_the_invocation_directory_scopes_the_plan() -> Result<(), String> {
    // `cargo metadata` reads the process working directory, which other
    // tests in this process move.
    let _cwd_guard = crate::acquire_test_cwd_read_guard();
    let document = read_metadata(&serving_cargo_for_test(), &[])?;
    let workspace_root = workspace_root_of(&document)?;
    let every = planned_roots(&invocations(&document, false, &workspace_root)?);
    assert!(every.len() > 400, "roots from the root: {}", every.len());
    let member = planned_roots(&invocations(
        &document,
        false,
        &workspace_root.join("xtask"),
    )?);
    let xtask_directory = canonical_or_keep(&workspace_root.join("xtask"));
    let inside_xtask = every
        .iter()
        .filter(|root| Path::new(root).starts_with(&xtask_directory))
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(!inside_xtask.is_empty(), "{every:?}");
    assert_eq!(member, inside_xtask, "from inside xtask/: xtask alone");
    for elsewhere in ["xtask/src", "docs", "crates"] {
        assert!(
            invocations(&document, false, &workspace_root.join(elsewhere)).is_err(),
            "from {elsewhere}: the plan must refuse"
        );
    }
    Ok(())
}

/// `cargo fmt -v` lists the roots it hands to rustfmt, every package the same
/// way, so its listing is the oracle for the plan. It prints the listing
/// before it starts rustfmt, so it is complete even for the package whose one
/// line is too long for the host to start rustfmt (#12004: `ripr` on
/// Windows).
#[test]
fn the_plan_lists_the_roots_cargo_fmt_lists() -> Result<(), String> {
    // `cargo` reads the process working directory, which other tests in this
    // process move. This test starts in `xtask/`, where the plan is xtask's
    // own; the oracle is for the plan read from the workspace root.
    let _cwd_guard = crate::acquire_test_cwd_read_guard();
    let document = read_metadata(&serving_cargo_for_test(), &[])?;
    let workspace_root = workspace_root_of(&document)?;
    let plan = invocations(&document, true, &workspace_root)?;
    assert!(
        plan.iter()
            .all(|invocation| command_line_length(&invocation.argv) <= COMMAND_LINE_BUDGET),
        "every run fits"
    );
    let planned = plan
        .iter()
        .flat_map(|invocation| roots_of(invocation).iter().cloned())
        .map(|path| path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        planned.len() > 400,
        "this workspace is the large one: {}",
        planned.len()
    );
    for package in ["ripr", "ripr-contract", "xtask", "repo-policy"] {
        let output = crate::run::capture_output(
            "cargo",
            &["fmt", "-p", package, "--check", "-v"],
            "cargo fmt -v",
        )?;
        let listed = output
            .stdout
            .lines()
            .filter_map(|line| {
                let quoted = line.trim().strip_prefix('[')?;
                let quoted = quoted.rsplit_once(']')?.1.trim();
                let quoted = quoted.strip_prefix('"')?.strip_suffix('"')?;
                // `cargo fmt` canonicalizes; `cargo metadata` reports the same
                // file without the verbatim prefix.
                Some(quoted.replace("\\\\", "\\"))
            })
            .map(|path| path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned())
            .collect::<BTreeSet<_>>();
        assert!(
            !listed.is_empty(),
            "`cargo fmt -v` listed no root for {package}"
        );
        let in_package = planned
            .iter()
            .filter(|root| {
                Path::new(root)
                    .components()
                    .any(|component| component.as_os_str() == package)
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(in_package, listed, "roots of {package}");
    }
    Ok(())
}
