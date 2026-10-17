//! Real Git witnesses for plain-diff path boundaries (RIPR-SPEC-0170).

use super::{common, run_command};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use common::fixture_git::{fixture_git_ok, fixture_git_output};

const PATHS: [&str; 6] = [
    "src/a.py",
    "src/a.rs",
    "src/b space.py",
    "src/b space.rs",
    "src/café 😀.py",
    "src/café 😀.rs",
];
const BEFORE: &str = "pub fn selected(input: i32) -> bool {\r\n    input > 0\r\n}\r\n";
const AFTER: &str =
    "// shifted\r\npub fn selected(input: i32) -> bool {\r\n    input >= 0\r\n}\r\n";

struct Repo(PathBuf);

impl Repo {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent)?;
        let root = parent.join(format!(
            "ripr-plain-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Own the exclusive directory before installing a cleanup guard.
        fs::create_dir(&root)?;
        let repo = Self(root);
        fs::create_dir(repo.0.join("src"))?;
        fs::write(
            repo.0.join("Cargo.toml"),
            "[package]\nname=\"plain_paths\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )?;
        fs::write(
            repo.0.join("src/lib.rs"),
            "pub mod a;\n#[path=\"b space.rs\"] pub mod b;\n#[path=\"café 😀.rs\"] pub mod c;\n",
        )?;
        fs::write(
            repo.0.join("ripr.toml"),
            "[languages]\nenabled=[\"rust\",\"python\"]\n",
        )?;
        fs::write(repo.0.join("diff-order.txt"), "*space*\n*\n")?;
        fixture_git_ok(&repo.0, &["init", "-q"])?;
        // Hostile local diff settings challenge the command-level stimulus pins.
        for (key, value) in [
            ("core.autocrlf", "false"),
            ("core.quotePath", "true"),
            ("diff.noPrefix", "true"),
            ("diff.mnemonicPrefix", "true"),
            ("diff.orderFile", "diff-order.txt"),
            ("commit.gpgsign", "false"),
            ("user.name", "Plain Paths"),
            ("user.email", "paths@example.invalid"),
        ] {
            fixture_git_ok(&repo.0, &["config", key, value])?;
        }
        for path in PATHS {
            fs::write(
                repo.0.join(path),
                if path.ends_with(".rs") {
                    BEFORE
                } else {
                    "def selected(value):\r\n    return value > 0\r\n"
                },
            )?;
        }
        fixture_git_ok(&repo.0, &["add", "."])?;
        fixture_git_ok(&repo.0, &["commit", "-qm", "base"])?;
        for path in PATHS {
            let after = if path.ends_with(".rs") {
                AFTER.to_string()
            } else {
                "# shifted\r\ndef selected(value):\r\n    return value >= 0\r\n".to_string()
            };
            fs::write(repo.0.join(path), after.replace("shifted", path))?;
        }
        Ok(repo)
    }

    fn patches(&self) -> Result<(String, String), String> {
        let inventory = fixture_git_output(
            &self.0,
            &[
                "-c",
                "diff.orderFile=/dev/null",
                "diff",
                "--name-only",
                "-z",
                "HEAD",
                "--",
            ],
        )?;
        assert_eq!(inventory, format!("{}\0", PATHS.join("\0")));
        let git_patch = fixture_git_output(
            &self.0,
            &[
                "-c",
                "diff.orderFile=/dev/null",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--unified=0",
                "HEAD",
                "--",
            ],
        )?;
        assert!(git_patch.starts_with("diff --git a/src/a.py b/src/a.py\n"));
        // Keep Git's exact path markers and CRLF bodies. Plain unified input
        // has no extended Git section/index records to rescue the boundary.
        let plain = git_patch
            .split_inclusive('\n')
            .filter(|line| !line.starts_with("diff --git ") && !line.starts_with("index "))
            .collect::<String>();
        assert!(plain.contains("--- a/src/b space.rs\t\n+++ b/src/b space.rs\t\n"));
        assert!(plain.contains("caf\\303\\251"));
        assert!(plain.contains("+    input >= 0\r\n"));
        Ok((git_patch, plain))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn plain_patch_json_keeps_portable_locations_and_exact_crlf_provenance()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = Repo::new()?;
    let (git_patch, plain) = repo.patches()?;
    let patch_path = repo.0.join("plain.patch");
    fs::write(&patch_path, plain)?;
    let git_path = repo.0.join("git.patch");
    fs::write(&git_path, git_patch)?;
    let check = |patch: &str| -> Result<String, Box<dyn std::error::Error>> {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&repo.0),
            &[
                "check", "--root", ".", "--diff", patch, "--mode", "instant", "--json",
            ],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    };
    let git_report: serde_json::Value = serde_json::from_str(&check("git.patch")?)?;
    let first = check("plain.patch")?;
    let second = check("plain.patch")?;
    assert_eq!(
        first, second,
        "unchanged input must render deterministic JSON"
    );
    let report: serde_json::Value = serde_json::from_str(&first)?;
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    assert!(!findings.is_empty());
    assert_eq!(report["summary"]["changed_rust_files"], 3);
    assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
    for path in PATHS {
        let locations: Vec<_> = findings
            .iter()
            .filter(|finding| {
                finding["probe"]["file"]
                    .as_str()
                    .is_some_and(|file| file.strip_prefix("./").unwrap_or(file) == path)
            })
            .collect();
        assert!(
            !locations.is_empty(),
            "missing finding for {path}: {report}"
        );
        assert!(
            locations
                .iter()
                .all(|finding| finding["probe"]["line"] == 3)
        );
    }
    // The existing source-subject producer stamps these Python gap records.
    // Generic Rust predicate findings do not currently emit such records.
    let mut expected_files = Vec::new();
    let mut digests = std::collections::BTreeSet::new();
    for path in PATHS.into_iter().filter(|path| path.ends_with(".py")) {
        let bytes = fs::read(repo.0.join(path))?;
        assert!(bytes.windows(2).any(|bytes| bytes == b"\r\n"));
        let digest = format!("sha256:{:x}", Sha256::digest(bytes));
        digests.insert(digest.clone());
        expected_files.push(serde_json::json!({"path": path, "digest": digest}));
    }
    assert_eq!(
        digests.len(),
        3,
        "distinct source bytes must have distinct digests"
    );
    assert_eq!(report["findings"], git_report["findings"]);
    assert_eq!(report["source_subject"], git_report["source_subject"]);
    assert_eq!(report["source_subject"]["digest_algorithm"], "sha256");
    assert_eq!(
        report["source_subject"]["files"],
        serde_json::json!(expected_files)
    );
    Ok(())
}
