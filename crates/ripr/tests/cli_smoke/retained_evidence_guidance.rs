//! RIPR-SPEC-0011 / #8222: input validation failures keep their operational
//! outcome and offer an exact, read-only attempt inspection route.
use super::{
    BOUNDARY_GAP_SEAM_ID, assert_success, init_producer_fixture_repo, run_command, shell_words,
    unique_temp_workspace,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn tree_hashes(root: &Path) -> TestResult<BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> TestResult {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                files.insert(path.strip_prefix(root)?.to_path_buf(), Vec::new());
                visit(root, &path, files)?;
            } else {
                files.insert(
                    path.strip_prefix(root)?.to_path_buf(),
                    Sha256::digest(std::fs::read(path)?).to_vec(),
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files)?;
    Ok(files)
}

#[test]
fn missing_retained_evidence_names_executable_status_without_mutation() -> TestResult {
    for explicit_store in [false, true] {
        let base = unique_temp_workspace("retained-evidence-guidance");
        let root = base.join("repo 'quoted'");
        let caller = base.join("caller");
        let replay_cwd = base.join("other caller");
        std::fs::create_dir_all(&caller)?;
        std::fs::create_dir_all(&replay_cwd)?;
        init_producer_fixture_repo(&root)?;
        let relative_root = "../repo 'quoted'";
        let store = if explicit_store {
            "target/ripr/evidence 'archive'"
        } else {
            "target/ripr/repair-attempts"
        };
        let store_args: Vec<&str> = if explicit_store {
            vec!["--store", store]
        } else {
            vec![]
        };
        let mut before_args = vec![
            "agent",
            "repair",
            "--root",
            relative_root,
            "--seam-id",
            BOUNDARY_GAP_SEAM_ID,
            "--phase",
            "before",
            "--json",
        ];
        before_args.extend(&store_args);
        let before = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&caller), &before_args)?;
        assert_success(&before);
        let before: serde_json::Value = serde_json::from_slice(&before.stdout)?;
        let id = before["repair_attempt"]["attempt_id"]
            .as_str()
            .ok_or("before must publish its actual attempt")?;
        let store_path = root.join(store);
        let manifest_path = store_path.join(id).join("attempt.json");
        let original_manifest = std::fs::read(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_slice(&original_manifest)?;
        assert_eq!(manifest["state"], "awaiting_edit");
        let original = tree_hashes(&store_path)?;
        for role in ["before_snapshot", "agent_packet"] {
            let relative = manifest["artifacts"]
                .as_array()
                .ok_or("artifacts required")?
                .iter()
                .find(|artifact| artifact["role"] == role)
                .and_then(|artifact| artifact["path"].as_str())
                .ok_or("role required")?;
            let path = root.join(relative);
            let backup = path.with_extension("audit-backup");
            std::fs::rename(&path, &backup)?;
            // Restore evidence after a returned error; a panic retains this owned fixture for diagnosis.
            let result = (|| -> TestResult {
                let missing = tree_hashes(&store_path)?;
                for facade in if explicit_store {
                    vec![false]
                } else {
                    vec![false, true]
                } {
                    let mut args = if facade {
                        vec!["continue", "--root", relative_root, "--attempt", id]
                    } else {
                        vec![
                            "agent",
                            "repair",
                            "--root",
                            relative_root,
                            "--attempt",
                            id,
                            "--phase",
                            "after",
                            "--json",
                        ]
                    };
                    args.extend(&store_args);
                    let after = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&caller), &args)?;
                    assert_eq!(after.status.code(), Some(2), "{after:?}");
                    assert!(after.stdout.is_empty(), "{after:?}");
                    let stderr = String::from_utf8_lossy(&after.stderr);
                    assert!(stderr.contains("canonicalize artifact "), "{stderr}");
                    assert!(stderr.contains(relative), "{stderr}");
                    assert!(stderr.contains("failed:"), "{stderr}");
                    assert!(
                        stderr.contains(&format!("repair attempt `{id}`")),
                        "{stderr}"
                    );
                    assert!(
                        stderr.contains("No new repair attempt was started."),
                        "{stderr}"
                    );
                    let command = stderr
                        .split("inspect it with `")
                        .nth(1)
                        .and_then(|tail| tail.split('`').next())
                        .ok_or_else(|| format!("missing inspection command: {stderr}"))?;
                    let words = shell_words::posix_words(command)?;
                    assert_eq!(words.first().map(String::as_str), Some("ripr"));
                    assert_eq!(words.get(1).map(String::as_str), Some("agent"));
                    assert_eq!(words.get(2).map(String::as_str), Some("status"));
                    let root_word = words
                        .iter()
                        .position(|word| word == "--root")
                        .and_then(|index| words.get(index + 1))
                        .ok_or("root flag required")?;
                    assert!(Path::new(root_word).is_absolute(), "{command}");
                    assert_eq!(Path::new(root_word).canonicalize()?, root.canonicalize()?);
                    assert_eq!(
                        words
                            .iter()
                            .position(|word| word == "--store")
                            .and_then(|index| words.get(index + 1))
                            .map(String::as_str),
                        explicit_store.then_some(store),
                        "{command}"
                    );
                    let mut args: Vec<&str> = words.iter().skip(1).map(String::as_str).collect();
                    let status = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&replay_cwd), &args)?;
                    assert_success(&status);
                    let stdout = String::from_utf8_lossy(&status.stdout);
                    assert!(
                        stdout.contains("Status: corrupt_or_unavailable"),
                        "{stdout}"
                    );
                    assert!(stdout.contains(id), "{stdout}");
                    args.push("--json");
                    let status = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&replay_cwd), &args)?;
                    assert_success(&status);
                    let status: serde_json::Value = serde_json::from_slice(&status.stdout)?;
                    assert_eq!(status["attempt"]["status_class"], "corrupt_or_unavailable");
                    assert_eq!(status["store"]["locator"], store, "{status:#}");
                    assert_eq!(
                        tree_hashes(&store_path)?,
                        missing,
                        "inspection changed evidence/inventory"
                    );
                    assert_eq!(std::fs::read(&manifest_path)?, original_manifest);
                }
                Ok(())
            })();
            std::fs::rename(&backup, &path)?;
            result?;
            assert_eq!(
                tree_hashes(&store_path)?,
                original,
                "restored evidence must match"
            );
        }
        // The healthy exact selection still resolves this attempt after restoration.
        let mut args = vec![
            "agent",
            "status",
            "--root",
            relative_root,
            "--attempt",
            id,
            "--json",
        ];
        args.extend(&store_args);
        let status = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&caller), &args)?;
        assert_success(&status);
        let status: serde_json::Value = serde_json::from_slice(&status.stdout)?;
        assert_eq!(status["attempt"]["status_class"], "awaiting_edit");
        assert_eq!(tree_hashes(&store_path)?, original);
        std::fs::remove_dir_all(base)?;
    }
    Ok(())
}
