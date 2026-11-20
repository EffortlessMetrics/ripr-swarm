//! Keep the scratch runner's wrapper and Cargo settings paired (#14685).
const WORKFLOW: &str = include_str!("../../.github/workflows/rust-gates.yml");

#[test]
fn sccache_environments_disable_incremental_compilation() {
    let mut lines = WORKFLOW.lines().peekable();
    let mut found = false;
    while let Some(line) = lines.next() {
        if line.trim() != "env:" {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let mut wrapper = false;
        let mut incremental = None;
        while let Some(&entry) = lines.peek() {
            let field = entry.trim();
            if !field.is_empty()
                && !field.starts_with('#')
                && entry.len() - entry.trim_start().len() <= indent
            {
                break;
            }
            lines.next();
            if let Some((key, value)) = field.split_once(':') {
                let value = value
                    .split(" #")
                    .next()
                    .unwrap()
                    .trim()
                    .trim_matches(['\'', '"']);
                match key {
                    "RUSTC_WRAPPER" => wrapper = value == "sccache",
                    "CARGO_INCREMENTAL" => incremental = Some(value),
                    _ => {}
                }
            }
        }
        if wrapper {
            found = true;
            assert_eq!(
                incremental,
                Some("0"),
                "sccache env requires CARGO_INCREMENTAL=0"
            );
        }
    }
    assert!(found, "expected a sccache environment in rust-gates");
}

#[test]
fn scratch_setup_persists_nonincremental_compilation_with_the_wrapper() {
    let setup = WORKFLOW
        .split("      - name: Configure scratch environment\n")
        .nth(1)
        .expect("scratch setup")
        .split("\n      - ")
        .next()
        .unwrap();
    let (exports, _) = setup
        .split_once("} >> \"$GITHUB_ENV\"")
        .expect("scratch environment export");
    let (_, exports) = exports
        .rsplit_once("\n          {\n")
        .expect("scratch export group");
    for key in ["CARGO_INCREMENTAL", "RUSTC_WRAPPER"] {
        assert!(
            exports
                .lines()
                .any(|line| line.trim() == format!("echo \"{key}=${key}\"")),
            "scratch setup must export {key}"
        );
    }
}
