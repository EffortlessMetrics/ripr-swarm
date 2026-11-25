use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{
    COMMAND_LINE_BUDGET, RustfmtInvocation, WINDOWS_COMMAND_LINE_LIMIT, argument_length, batch,
    canonical_or_keep, cargo_fmt, cargo_home_bin, checked_metadata, command_line_length, formatter,
    invocations, metadata_arguments, metadata_document, preflight, prepare, read_document,
    read_metadata, read_metadata_through, run_all, run_plan, serving_cargo, utf8_program,
    windows_command_line, windows_command_line_units,
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

/// A path `cargo metadata` reports, without the verbatim prefix a canonical
/// path carries on Windows.
fn without_verbatim_prefix(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

/// The directory of the manifest of the package called `name`.
fn package_directory(document: &Value, name: &str) -> Result<PathBuf, String> {
    document["packages"]
        .as_array()
        .ok_or_else(|| "cargo metadata reports no packages".to_string())?
        .iter()
        .find(|entry| entry["name"].as_str() == Some(name))
        .and_then(|entry| entry["manifest_path"].as_str())
        .map(Path::new)
        .and_then(Path::parent)
        .map(|directory| PathBuf::from(without_verbatim_prefix(&directory.to_string_lossy())))
        .ok_or_else(|| format!("cargo metadata reports no package {name}"))
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
    // `cargo` and `rustfmt` start in the process working directory, which
    // other tests in this process move.
    let _cwd_guard = crate::acquire_test_cwd_read_guard();
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
        Err("cargo metadata --no-deps reported no edition for target \
             /repo/odd/src/lib.rs"
            .to_string())
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
fn the_formatter_is_rustfmt_env_else_the_rustfmt_beside_cargo_fmt() -> Result<(), String> {
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
    // A metadata wrapper: a `rustfmt` beside `cargo` and no `cargo-fmt`, as
    // when `CARGO` names another toolchain's cargo.
    let wrapper = root.join("metadata-wrapper");
    // A toolchain without its rustfmt component: `cargo-fmt`, and no `rustfmt`.
    let broken = root.join("broken-toolchain");
    let home_bin = root.join("cargo-home").join("bin");
    let no_home_bin = root.join("no-home-bin");
    for directory in [
        &toolchain,
        &other,
        &shadow,
        &bare,
        &wrapper,
        &broken,
        &home_bin,
        &no_home_bin,
    ] {
        std::fs::create_dir_all(directory).map_err(|err| err.to_string())?;
    }
    // A toolchain has `cargo-fmt` and `rustfmt` beside `cargo`. On Unix cargo
    // runs a subcommand file only when it carries a permission bit.
    let files = |directory: &Path, names: &[&str]| -> Result<(), String> {
        for name in names {
            let file = directory.join(named(name));
            std::fs::write(&file, "").map_err(|err| err.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
                    .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    };
    files(&toolchain, &["cargo", "cargo-fmt", "rustfmt"])?;
    files(&other, &["cargo", "cargo-fmt", "rustfmt"])?;
    files(&shadow, &["rustfmt"])?;
    files(&bare, &["cargo"])?;
    files(&wrapper, &["cargo", "rustfmt"])?;
    files(&broken, &["cargo", "cargo-fmt"])?;
    files(&home_bin, &["cargo-fmt", "rustfmt"])?;
    let os = |path: &Path| path.to_string_lossy().into_owned();
    let cargo = |directory: &Path| directory.join(named("cargo")).into_os_string();
    let joined = |directories: &[&Path]| -> Result<std::ffi::OsString, String> {
        std::env::join_paths(directories.iter().map(|directory| os(directory)))
            .map_err(|err| err.to_string())
    };
    let run = |path: &std::ffi::OsStr| formatter(None, Some(path), &no_home_bin);

    // PATH-shadowing control: the shadowing rustfmt comes first on PATH and
    // is not run; cargo finds `toolchain`'s cargo-fmt there, and runs the
    // rustfmt beside that one.
    let path = joined(&[&shadow, &toolchain, &other, &bare])?;
    assert_eq!(
        serving_cargo(None, Some(path.as_os_str())),
        Some(toolchain.join(named("cargo")))
    );
    assert_eq!(
        cargo_fmt(Some(path.as_os_str()), &no_home_bin),
        Some(toolchain.join(named("cargo-fmt")))
    );
    assert_eq!(run(path.as_os_str()), Ok(toolchain.join(named("rustfmt"))));

    // CARGO names the cargo that answers the metadata.
    assert_eq!(
        serving_cargo(Some(&cargo(&other)), Some(path.as_os_str())),
        Some(other.join(named("cargo")))
    );
    // An empty CARGO names none, so PATH decides.
    assert_eq!(
        serving_cargo(Some(std::ffi::OsStr::new("")), Some(path.as_os_str())),
        Some(toolchain.join(named("cargo")))
    );

    // Split toolchain: a metadata wrapper with a `rustfmt` of its own and no
    // `cargo-fmt` comes first on PATH. `cargo fmt` finds no cargo-fmt in it,
    // looks on, and runs the rustfmt beside the cargo-fmt it finds; the
    // wrapper's rustfmt is not borrowed.
    let split = joined(&[&wrapper, &toolchain])?;
    assert_eq!(
        run(split.as_os_str()),
        Ok(toolchain.join(named("rustfmt"))),
        "the formatter is the one beside the cargo-fmt cargo fmt runs"
    );

    // The first cargo on PATH has no cargo-fmt beside it: cargo looks on and
    // runs the first toolchain's that has one.
    let bare_first = joined(&[&bare, &toolchain])?;
    assert_eq!(
        run(bare_first.as_os_str()),
        Ok(toolchain.join(named("rustfmt")))
    );

    // Cargo looks in its home's `bin` directory before PATH, unless PATH
    // already holds that directory: then PATH's order decides.
    let other_only = joined(&[&other])?;
    assert_eq!(
        formatter(None, Some(other_only.as_os_str()), &home_bin),
        Ok(home_bin.join(named("rustfmt")))
    );
    let other_then_home = joined(&[&other, &home_bin])?;
    assert_eq!(
        formatter(None, Some(other_then_home.as_os_str()), &home_bin),
        Ok(other.join(named("rustfmt")))
    );
    // A home without a `bin` that holds cargo-fmt leaves it to PATH.
    assert_eq!(
        formatter(None, Some(other_only.as_os_str()), &no_home_bin),
        Ok(other.join(named("rustfmt")))
    );

    // No cargo-fmt anywhere: `cargo fmt` has no subcommand to run, and the
    // remedy names the override. With no PATH at all, the same.
    for none in [joined(&[&shadow, &bare])?, std::ffi::OsString::new()] {
        assert_eq!(cargo_fmt(Some(&none), &no_home_bin), None);
        let Err(error) = run(&none) else {
            return Err("no cargo-fmt anywhere must not name a formatter".to_owned());
        };
        assert!(
            error.contains("no cargo-fmt") && error.contains("set RUSTFMT"),
            "the remedy names the override: {error}"
        );
    }

    // A cargo-fmt with no rustfmt beside it: that toolchain's formatter is
    // missing, and no later toolchain's is borrowed.
    let broken_only = joined(&[&broken])?;
    let Err(error) = run(broken_only.as_os_str()) else {
        return Err("a cargo-fmt with no rustfmt must not name a formatter".to_owned());
    };
    assert!(
        error.contains("no rustfmt sits beside the cargo-fmt") && error.contains("set RUSTFMT"),
        "the remedy names the override: {error}"
    );

    // A `cargo-fmt` reached through a symbolic link, with no rustfmt beside
    // the link. On Linux cargo-fmt finds itself running as the link's target
    // and runs the rustfmt beside that.
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::symlink;
        let links = root.join("links");
        std::fs::create_dir_all(&links).map_err(|err| err.to_string())?;
        symlink(
            toolchain.join(named("cargo-fmt")),
            links.join(named("cargo-fmt")),
        )
        .map_err(|err| err.to_string())?;
        let path = std::env::join_paths([os(&links)]).map_err(|err| err.to_string())?;
        let real = |path: &Path| std::fs::canonicalize(path).map_err(|err| err.to_string());
        let running = real(&toolchain.join(named("cargo-fmt")))?;
        assert_eq!(
            cargo_fmt(Some(path.as_os_str()), &no_home_bin),
            Some(running)
        );
        assert_eq!(
            formatter(None, Some(path.as_os_str()), &no_home_bin),
            Ok(real(&toolchain)?.join(named("rustfmt")))
        );
    }

    // RUSTFMT names the formatter outright; an empty RUSTFMT is read as unset.
    assert_eq!(
        formatter(
            Some(std::ffi::OsStr::new("/opt/other/rustfmt")),
            Some(broken_only.as_os_str()),
            &no_home_bin
        ),
        Ok(PathBuf::from("/opt/other/rustfmt"))
    );
    assert_eq!(
        formatter(
            Some(std::ffi::OsStr::new("")),
            Some(path.as_os_str()),
            &no_home_bin
        ),
        Ok(toolchain.join(named("rustfmt")))
    );

    // On Windows an extensionless `cargo` is never the cargo a bare `cargo`
    // starts, so a directory holding only that one is passed over.
    if cfg!(windows) {
        let loose = root.join("loose-cargo");
        std::fs::create_dir_all(&loose).map_err(|err| err.to_string())?;
        std::fs::write(loose.join("cargo"), "").map_err(|err| err.to_string())?;
        let loose_first = joined(&[&loose, &toolchain, &other, &bare])?;
        assert_eq!(
            serving_cargo(None, Some(loose_first.as_os_str())),
            Some(toolchain.join(named("cargo")))
        );
    }

    // No cargo at all serves the step and no RUSTFMT is set.
    assert_eq!(serving_cargo(None, None), None);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// The command line the runner's `Command` hands Windows, by the standard
/// library's rules: the program in quotes, an argument in quotes when it is
/// empty or holds a space or a tab, a backslash run before a quote doubled
/// with one more added, and a trailing run doubled before the closing quote.
#[test]
fn the_windows_command_line_follows_the_standard_librarys_rules() {
    let words = |list: &[&str]| {
        list.iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    // The program is always quoted; a plain argument is not.
    assert_eq!(
        windows_command_line("/t/rustfmt.exe", &words(&["--edition", "2024"])),
        "\"/t/rustfmt.exe\" --edition 2024"
    );
    assert_eq!(
        windows_command_line("/t o/rustfmt.exe", &[]),
        "\"/t o/rustfmt.exe\""
    );
    // An argument is quoted when it is empty or holds a space or a tab.
    assert_eq!(windows_command_line("r", &words(&[""])), "\"r\" \"\"");
    assert_eq!(
        windows_command_line("r", &words(&["two words"])),
        "\"r\" \"two words\""
    );
    assert_eq!(
        windows_command_line("r", &words(&["a\tb"])),
        "\"r\" \"a\tb\""
    );
    // A backslash run before a quote is doubled, with one more added.
    assert_eq!(windows_command_line("r", &words(&["a\"b"])), "\"r\" a\\\"b");
    assert_eq!(
        windows_command_line("r", &words(&["a\\\"b"])),
        "\"r\" a\\\\\\\"b"
    );
    assert_eq!(
        windows_command_line("r", &words(&["a\\\\\"b"])),
        "\"r\" a\\\\\\\\\\\"b"
    );
    // A trailing backslash run is doubled before a closing quote, and left
    // alone in an argument that is not quoted.
    assert_eq!(
        windows_command_line("r", &words(&["x y\\"])),
        "\"r\" \"x y\\\\\""
    );
    assert_eq!(windows_command_line("r", &words(&["x\\"])), "\"r\" x\\");
    // The units are UTF-16 code units, the terminating NUL included.
    assert_eq!(
        windows_command_line_units("r", &words(&["ab"])),
        "\"r\" ab".encode_utf16().count() + 1
    );
    assert_eq!(
        windows_command_line_units("r", &words(&["\u{1F600}"])),
        4 + 2 + 1
    );
}

/// One run of `head` followed by the roots named.
fn planned(head: &[String], roots: &[&str]) -> RustfmtInvocation {
    let mut argv = head.to_vec();
    argv.extend(roots.iter().map(|root| (*root).to_owned()));
    RustfmtInvocation {
        argv,
        root_count: roots.len(),
    }
}

/// Windows `CreateProcess` refuses a command line over 32,767 UTF-16 code
/// units, the terminating NUL included. A run that would be refused there
/// fails the step before any batch starts, so nothing is left half formatted.
/// A single root over the batching budget and inside the limit still runs,
/// alone.
#[test]
fn a_run_windows_cannot_start_is_refused_before_any_batch_runs() -> Result<(), String> {
    // A formatter that cannot start is looked for in the process working
    // directory, which other tests in this process move.
    let _cwd_guard = crate::acquire_test_cwd_read_guard();
    let head = ["--edition", "2024"].map(String::from);
    // A formatter path with a space is quoted on the command line.
    let program = "/t o/rustfmt.exe";
    let units = |root: &str| {
        windows_command_line_units(program, &[head.to_vec(), vec![root.to_owned()]].concat())
    };
    // Two adjacent command lines, the last one over the limit by one unit.
    let mut root = "x".repeat(32_000);
    while units(&root) < WINDOWS_COMMAND_LINE_LIMIT {
        root.push('x');
    }
    assert_eq!(units(&root), WINDOWS_COMMAND_LINE_LIMIT);
    root.push('x');
    assert_eq!(units(&root), WINDOWS_COMMAND_LINE_LIMIT + 1);
    // Over the batching budget and inside the limit: legal, and alone.
    let long_root = "x".repeat(COMMAND_LINE_BUDGET + 50);
    let long = planned(&head, &[long_root.as_str()]);
    assert!(command_line_length(&long.argv) > COMMAND_LINE_BUDGET);
    assert!(windows_command_line_units(program, &long.argv) <= WINDOWS_COMMAND_LINE_LIMIT);
    let short = planned(&head, &["short.rs"]);
    assert_eq!(preflight(program, &[short.clone(), long.clone()]), Ok(()));
    // One unit over the limit: refused, and the refusal names the batch.
    let over = planned(&head, &[root.as_str()]);
    let Err(error) = preflight(program, &[short.clone(), over.clone()]) else {
        return Err("a run Windows cannot start must be refused".to_owned());
    };
    assert!(
        error.contains("batch 2 of 2 takes 32768 UTF-16 code units")
            && error.contains(&format!("at most {WINDOWS_COMMAND_LINE_LIMIT}"))
            && error.contains("a target root or the formatter's own path is too long")
            && error.contains("no batch was run"),
        "{error}"
    );
    // Nothing starts. With a formatter that cannot start, a legal plan
    // reports the formatter, which shows the first batch was started. A
    // refused plan reports the refusal and starts none.
    let mut diagnostics = Vec::new();
    let Err(error) = run_plan(
        "ripr-no-such-formatter",
        &[short.clone(), long.clone()],
        &[],
        Some(&mut diagnostics),
    ) else {
        return Err("a formatter that cannot start must fail the step".to_owned());
    };
    assert!(
        error.starts_with("failed to run ripr-no-such-formatter:"),
        "{error}"
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let mut diagnostics = Vec::new();
    let Err(error) = run_plan(
        "ripr-no-such-formatter",
        &[short.clone(), over],
        &[],
        Some(&mut diagnostics),
    ) else {
        return Err("a run Windows cannot start must fail the step".to_owned());
    };
    assert!(
        error.starts_with("the formatter cannot be started on Windows:"),
        "{error}"
    );
    assert!(
        diagnostics.is_empty(),
        "no batch was started: {diagnostics:?}"
    );
    Ok(())
}

/// A reply that parses and still lacks what the plan reads fails its
/// question, as it fails cargo-fmt's typed reader, so the question is asked
/// once more without `--offline`.
#[test]
fn a_document_that_lacks_what_the_plan_reads_fails_its_question() -> Result<(), String> {
    let document = r#"{"workspace_root":"/repo","packages":[{"name":"a","edition":"2024","manifest_path":"/repo/a/Cargo.toml","targets":[{"kind":["lib"],"src_path":"/repo/a/src/lib.rs"}]}]}"#;
    assert_eq!(checked_metadata(&read_document(document)?), Ok(()));
    for malformed in [
        "{}",
        "[1]",
        r#"{"workspace_root":"/repo","packages":{}}"#,
        r#"{"workspace_root":"/repo","packages":[{"manifest_path":"/repo/a/Cargo.toml"}]}"#,
        r#"{"workspace_root":"/repo","packages":[{"edition":"2024","targets":[{"kind":["lib"],"src_path":"/repo/a/src/lib.rs"}]}]}"#,
        r#"{"workspace_root":"/repo","packages":[{"edition":"2024","manifest_path":"/repo/a/Cargo.toml","targets":[{"kind":["lib"]}]}]}"#,
        r#"{"workspace_root":"/repo","packages":[{"manifest_path":"/repo/a/Cargo.toml","targets":[{"kind":["lib"],"src_path":"/repo/a/src/lib.rs"}]}]}"#,
    ] {
        let Err(problem) = read_document(malformed) else {
            return Err(format!(
                "a reply that lacks a field must fail its question: {malformed}"
            ));
        };
        assert!(
            problem.starts_with("cargo metadata --no-deps reported no"),
            "{problem}"
        );
        // The offline reply is malformed: the same question is asked once
        // more, online, and the online reply is read.
        let mut asked = Vec::new();
        let retried = read_metadata_through("cargo", &mut |offline| {
            asked.push(offline);
            read_document(if offline { malformed } else { document })
        })?;
        assert_eq!(asked, [true, false], "{malformed}");
        assert_eq!(retried["workspace_root"], "/repo");
        // Both malformed: the failure names both.
        let mut asked = Vec::new();
        let Err(error) = read_metadata_through("cargo", &mut |offline| {
            asked.push(offline);
            read_document(malformed)
        }) else {
            return Err("two malformed replies must not read a document".to_owned());
        };
        assert_eq!(asked, [true, false]);
        assert!(
            error.contains("failed offline (cargo metadata --no-deps reported no")
                && error.contains("without --offline (cargo metadata --no-deps reported no"),
            "{error}"
        );
    }
    Ok(())
}

/// The metadata is read before the formatter is chosen, as cargo-fmt reads
/// it: a `CARGO` that names nothing fails as a cargo that cannot be started,
/// and the failure does not send the reader after a rustfmt.
#[test]
fn a_cargo_that_names_nothing_fails_as_a_cargo_not_as_a_missing_rustfmt() -> Result<(), String> {
    let missing = std::env::temp_dir()
        .join(format!("ripr-no-such-cargo-{}", std::process::id()))
        .join("cargo")
        .into_os_string();
    let Err(error) = prepare(
        true,
        &[],
        &missing,
        None,
        None,
        Path::new("/no-cargo-home/bin"),
    ) else {
        return Err("a cargo that names nothing must not plan anything".to_owned());
    };
    assert!(
        error.contains("metadata --no-deps failed offline") && error.contains("ripr-no-such-cargo"),
        "{error}"
    );
    assert!(
        !error.contains("no cargo-fmt") && !error.contains("no rustfmt sits beside"),
        "the failure named a formatter remedy: {error}"
    );
    Ok(())
}

/// A program path that is not UTF-8 cannot be named to the runner, which
/// takes UTF-8; it is refused by name and not started under a lossy spelling.
#[test]
fn a_program_path_that_is_not_utf8_is_refused_by_name() -> Result<(), String> {
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[0xD800]))
    };
    #[cfg(not(windows))]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(vec![b'c', 0xFF]))
    };
    assert_eq!(
        utf8_program(Path::new("/usr/bin/cargo"), "the cargo")?,
        "/usr/bin/cargo"
    );
    let Err(error) = utf8_program(&path, "the formatter to run") else {
        return Err("a path that is not UTF-8 must be refused".to_owned());
    };
    assert!(
        error.starts_with("the formatter to run is not a UTF-8 path"),
        "{error}"
    );
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
        metadata_document(document)
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
            metadata_document(document)
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

    // A reply that is not a document fails its question, as it fails
    // cargo-fmt's, so the question is asked once more online.
    let mut asked = Vec::new();
    let retried = read_metadata_through("cargo", &mut |offline| {
        asked.push(offline);
        metadata_document(if offline { "not a document" } else { document })
    })?;
    assert_eq!(asked, [true, false]);
    assert_eq!(retried["workspace_root"], "/repo");
    let mut asked = Vec::new();
    let Err(error) = read_metadata_through("cargo", &mut |offline| {
        asked.push(offline);
        metadata_document("not a document")
    }) else {
        return Err("two replies that are not documents must not read a document".to_owned());
    };
    assert_eq!(asked, [true, false]);
    assert!(
        error.contains("failed offline (cargo metadata --no-deps printed no JSON document")
            && error
                .contains("without --offline (cargo metadata --no-deps printed no JSON document"),
        "{error}"
    );
    Ok(())
}

/// A formatter that cannot be started stops the step at that batch, as
/// cargo-fmt stops there: no later batch is started against it.
#[test]
fn a_formatter_that_cannot_start_stops_the_step() -> Result<(), String> {
    // `cargo` and `rustfmt` start in the process working directory, which
    // other tests in this process move.
    let _cwd_guard = crate::acquire_test_cwd_read_guard();
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
    let program = formatter(
        None,
        std::env::var_os("PATH").as_deref(),
        &cargo_home_bin()?,
    )?;
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
        .map(|path| without_verbatim_prefix(&path).to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        planned.len() > 400,
        "this workspace is the large one: {}",
        planned.len()
    );
    let mut attributed = BTreeSet::new();
    for package in ["ripr", "ripr-contract", "xtask", "repo-policy"] {
        // A root belongs to the package whose manifest directory holds it. A
        // path component equal to the package's name would not do: in a
        // checkout directory named `ripr`, every root has one.
        let directory = package_directory(&document, package)?;
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
            .map(|path| without_verbatim_prefix(&path).to_owned())
            .collect::<BTreeSet<_>>();
        assert!(
            !listed.is_empty(),
            "`cargo fmt -v` listed no root for {package}"
        );
        let in_package = planned
            .iter()
            .filter(|root| Path::new(root).starts_with(&directory))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(
            !in_package.is_empty(),
            "the plan holds no root under {}",
            directory.display()
        );
        assert_eq!(in_package, listed, "roots of {package}");
        attributed.extend(in_package);
    }
    assert_eq!(
        attributed, planned,
        "every planned root is under one of the four manifests"
    );
    Ok(())
}
