use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{
    COMMAND_LINE_BUDGET, FORMATTED_TARGET_KINDS, RustfmtInvocation, argument_length, batch,
    command_line_length, invocations,
};

fn package(id: &str, edition: &str, targets: &[(&str, &str)]) -> Value {
    json!({
        "id": id,
        "edition": edition,
        "targets": targets
            .iter()
            .map(|(kind, src_path)| json!({ "kind": [kind], "src_path": src_path }))
            .collect::<Vec<_>>(),
    })
}

fn metadata(workspace_root: &str, packages: Vec<Value>) -> Value {
    json!({
        "workspace_root": workspace_root,
        "packages": packages,
    })
}

fn roots_of(invocation: &RustfmtInvocation) -> &[String] {
    &invocation.argv[invocation.argv.len() - invocation.root_count..]
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

#[test]
fn each_edition_runs_rustfmt_once_per_batch_and_check_is_optional() -> Result<(), String> {
    let document = metadata(
        "/repo",
        vec![
            package(
                "old 0.1.0 (path+file:///repo/old)",
                "2021",
                &[("lib", "/repo/old/src/lib.rs")],
            ),
            package(
                "new 0.1.0 (path+file:///repo/new)",
                "2024",
                &[
                    ("bin", "/repo/new/src/main.rs"),
                    ("test", "/repo/new/tests/a.rs"),
                ],
            ),
        ],
    );
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
    let checked = invocations(&document, true)?;
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
    let written = invocations(&document, false)?;
    assert_eq!(editions(&written)?, ["2021", "2024"]);
    assert!(
        written
            .iter()
            .all(|invocation| !invocation.argv.iter().any(|arg| arg == "--check"))
    );
    Ok(())
}

#[test]
fn roots_are_the_targets_cargo_fmt_hands_to_rustfmt() -> Result<(), String> {
    let targets: Vec<Value> = FORMATTED_TARGET_KINDS
        .iter()
        .map(|kind| json!({ "kind": [kind], "src_path": format!("/repo/kinds/{kind}.rs") }))
        .collect();
    let document = metadata(
        "/repo",
        vec![
            json!({
                "id": "kinds 0.1.0 (path+file:///repo/kinds)",
                "edition": "2024",
                "targets": targets,
            }),
            package(
                "skipped 0.1.0 (path+file:///repo/skipped)",
                "2024",
                &[
                    // Not a Rust source root.
                    ("bin", "/repo/skipped/src/main.c"),
                    // A kind `cargo fmt` does not hand to rustfmt.
                    ("metabuild", "/repo/skipped/metabuild.rs"),
                ],
            ),
        ],
    );
    let plan = invocations(&document, true)?;
    let roots = plan
        .iter()
        .flat_map(|invocation| roots_of(invocation).iter().cloned())
        .collect::<BTreeSet<_>>();
    let expected = FORMATTED_TARGET_KINDS
        .iter()
        .map(|kind| format!("/repo/kinds/{kind}.rs"))
        .collect::<BTreeSet<_>>();
    assert_eq!(roots.len(), expected.len());
    assert_eq!(roots, expected);
    Ok(())
}

#[test]
fn a_package_without_an_edition_is_refused() {
    let document = metadata(
        "/repo",
        vec![json!({ "id": "odd", "edition": "", "targets": [] })],
    );
    assert_eq!(
        invocations(&document, true),
        Err("workspace package odd reports no edition".to_string())
    );
}

#[test]
fn a_workspace_rustfmt_config_is_passed_to_every_run() -> Result<(), String> {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-workspace-format-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
    let config = root.join(".rustfmt.toml");
    std::fs::write(&config, "max_width = 100\n").map_err(|err| err.to_string())?;
    let document = metadata(
        &root.to_string_lossy(),
        vec![package(
            "one 0.1.0 (path+file:///one)",
            "2024",
            &[("lib", "/one/src/lib.rs")],
        )],
    );
    let plan = invocations(&document, true).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&root);
    })?;
    std::fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
    assert_eq!(plan.len(), 1);
    assert_eq!(
        plan[0].argv[..3],
        [
            "--config-path".to_owned(),
            config.to_string_lossy().into_owned(),
            "--edition".to_owned()
        ]
    );
    Ok(())
}

/// `cargo fmt -v` lists the roots it hands to rustfmt. It can speak only for a
/// package whose one line still fits (#12004), but it reads every package the
/// same way, so the packages short enough to run are the oracle for the plan.
#[test]
fn the_plan_lists_the_roots_cargo_fmt_lists() -> Result<(), String> {
    let plan = super::plan(true, &[])?;
    assert!(
        plan.iter()
            .all(|invocation| command_line_length(&invocation.argv) <= COMMAND_LINE_BUDGET),
        "every run fits"
    );
    let planned = plan
        .iter()
        .flat_map(|invocation| roots_of(invocation).iter().cloned())
        .collect::<BTreeSet<_>>();
    assert!(
        planned.len() > 400,
        "this workspace is the large one: {}",
        planned.len()
    );
    for package in ["ripr-contract", "xtask", "repo-policy"] {
        let output = crate::run::capture_output(
            "cargo",
            &["fmt", "-p", package, "--check", "-v"],
            "cargo fmt -v",
        )?;
        let listed = output
            .stdout
            .lines()
            .chain(output.stderr.lines())
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
