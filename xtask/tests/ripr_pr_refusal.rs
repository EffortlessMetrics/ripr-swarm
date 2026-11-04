//! Exercise the actual workflow shell from analyzer output through comment text.
#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

use std::process::Command;

const WORKFLOW: &str = include_str!("../../.github/workflows/ripr-pr.yml");
const REFUSAL: &str = "ripr: diff_scope_oversized: 2556 changed Rust lines across 3 Rust files exceed the RIPR_MAX_DIFF_CHANGED_RUST_LINES limit (2000); analysis was not run.";
const SUMMARY: &str = "diff_scope_oversized: 2556 changed Rust lines across 3 Rust files exceed the RIPR_MAX_DIFF_CHANGED_RUST_LINES limit (2000).";

fn block(name: &str) -> String {
    let part = WORKFLOW
        .split(&format!("      - name: {name}\n"))
        .nth(1)
        .unwrap();
    let part = part
        .split("        run: |\n")
        .nth(1)
        .unwrap()
        .split("\n      - ")
        .next()
        .unwrap();
    part.lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}
fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
fn refusal_is_bounded_and_other_diagnostics_are_not_published() {
    let root = std::env::temp_dir().join(format!("ripr-pr-refusal-{}", std::process::id()));
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("temp")).unwrap();
    executable(
        &root.join("bin/ripr"),
        "#!/bin/sh\ncat \"$CASE_ERR\" >&2\nprintf '%s' \"$CASE_OUT\"\nexit \"$CASE_STATUS\"\n",
    );
    executable(&root.join("bin/gh"), "#!/bin/sh\nexit 0\n");
    symlink(root.join("temp/out"), root.join("out")).unwrap();
    let path = format!(
        "{}:{}",
        root.join("bin").display(),
        std::env::var("PATH").unwrap()
    );
    let cases = [
        (
            format!("{REFUSAL}\nSECRET_SECOND\n::warning::hostile"),
            "",
            "2",
            true,
        ),
        (
            format!("{REFUSAL} ``` SECRET_INLINE\nSECRET_SECOND"),
            "",
            "2",
            true,
        ),
        (format!("SECRET_FIRST\n{REFUSAL}"), "", "2", false),
        (
            format!("{REFUSAL}\nSECRET_SECOND"),
            "successful report",
            "0",
            false,
        ),
        (
            format!("{}\nSECRET_SECOND", "x".repeat(50000)),
            "",
            "2",
            false,
        ),
        (REFUSAL.replace("2556", "2556\rSECRET"), "", "2", false),
    ];
    for (stderr, stdout, status, expected) in cases {
        if root.join("temp/out").exists() {
            fs::remove_dir_all(root.join("temp/out")).unwrap();
        }
        fs::write(root.join("stderr"), stderr).unwrap();
        let invoke = |script: &str| {
            Command::new("bash")
                .args(["-e", "-o", "pipefail", "-c", script])
                .current_dir(&root)
                .env("PATH", &path)
                .env("RUNNER_TEMP", root.join("temp"))
                .env("RIPR_BIN", root.join("bin/ripr"))
                .env("CASE_ERR", root.join("stderr"))
                .env("CASE_OUT", stdout)
                .env("CASE_STATUS", status)
                .env("BASE_SHA", "a")
                .env("HEAD_SHA", "0123456789012345")
                .env("REPO", "a/b")
                .env("PR", "1")
                .env("RUN_URL", "https://example.test")
                .env("GH_TOKEN", "dummy")
                .output()
                .unwrap()
        };
        let analysis = invoke(&block("Run ripr on the pull request diff"));
        assert!(
            analysis.status.success(),
            "{}",
            String::from_utf8_lossy(&analysis.stderr)
        );
        assert!(String::from_utf8_lossy(&analysis.stdout).contains("::stop-commands::"));
        let result = invoke(&block("Upsert the ripr comment"));
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let body = fs::read_to_string(root.join("body.md")).unwrap();
        assert_eq!(body.contains(SUMMARY), expected, "{body}");
        assert!(!body.contains("SECRET"), "{body}");
        if status == "0" {
            assert!(body.contains("successful report"));
        }
        for (forged, accepted) in [
            ("```\nSECRET_FORGED".to_string(), false),
            (format!("{SUMMARY}\nSECRET_FORGED"), true),
        ] {
            fs::write(root.join("temp/out/report.txt"), "").unwrap();
            fs::write(root.join("temp/out/refusal.txt"), forged).unwrap();
            let result = invoke(&block("Upsert the ripr comment"));
            assert!(result.status.success());
            let body = fs::read_to_string(root.join("body.md")).unwrap();
            assert_eq!(body.contains(SUMMARY), accepted);
            assert!(!body.contains("SECRET_FORGED"));
        }
    }
    fs::remove_dir_all(root).unwrap();
}
