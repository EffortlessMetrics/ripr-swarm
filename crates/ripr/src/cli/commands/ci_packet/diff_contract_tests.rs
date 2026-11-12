//! Real-Git controls for the shared side-route diff contract (#9365).
use super::*;
use crate::analysis::{load_diff_range, load_pr_evidence_diff_range};
use crate::domain::{
    GitCandidateBase, GitCandidateDiffSemantics, GitCandidateSubject, GitObjectId, GitTreeish,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Repo(PathBuf);
impl Drop for Repo {
    fn drop(&mut self) {
        if let Err(error) = crate::testing::fixture_git::remove_fixture_tree(&self.0) {
            eprintln!("diff contract cleanup {}: {error}", self.0.display());
        }
    }
}
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run_git(root, args).map_err(|error| error.to_string())
}
fn capture(root: &Path) -> Result<Vec<u8>, String> {
    let mut run = PacketRun {
        root: root.to_path_buf(),
        exe: PathBuf::from("unused"),
        settings: CiSettings {
            base_ref: "main".to_string(),
            ..CiSettings::default()
        },
        only: Vec::new(),
        seen: Vec::new(),
        top_seam: None,
        failed: Vec::new(),
        recorded: None,
    };
    run.capture_pull_request_diff()?;
    fs::read(root.join("target/ripr/reports/pr.diff")).map_err(|error| error.to_string())
}
fn candidate(root: &Path, base: &str) -> Result<String, String> {
    let subject = GitCandidateSubject {
        repository_root: root.to_path_buf(),
        base: GitCandidateBase::Treeish(GitTreeish::new(base).map_err(|e| e.to_string())?),
        candidate_tree: GitObjectId::parse(&git(root, &["rev-parse", "HEAD^{tree}"])?)
            .map_err(|e| e.to_string())?,
        diff_semantics: GitCandidateDiffSemantics::DirectTreeToTree,
    };
    crate::analysis::git_candidate_execution::resolve(&subject, Some(Duration::from_secs(20)))
        .map(|resolved| resolved.diff)
        .map_err(|e| e.to_string())
}

#[test]
fn side_route_diff_contract() -> Result<(), String> {
    // Only the fixture child loses caller Git selectors/config. Never unset
    // the hook's environment in this process or change global Git config.
    const CHILD: &str = "RIPR_SIDE_DIFF_FIXTURE_CHILD";
    const DONE: &str = "SIDE_DIFF_CONTRACT_ASSERTIONS_COMPLETED";
    if std::env::var_os(CHILD).is_none() {
        let selector = format!("{}::side_route_diff_contract", module_path!());
        let (_, selector) = selector.split_once("::").ok_or("missing crate prefix")?;
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        child
            .args(["--exact", selector, "--nocapture"])
            .env(CHILD, "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env(
                "GIT_CONFIG_SYSTEM",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            );
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_SHALLOW_FILE",
            "GIT_GRAFT_FILE",
            "GIT_REPLACE_REF_BASE",
            "GIT_NO_REPLACE_OBJECTS",
            "GIT_PREFIX",
            "GIT_IMPLICIT_WORK_TREE",
            "GIT_NAMESPACE",
            "GIT_EXTERNAL_DIFF",
            "GIT_DIFF_OPTS",
        ] {
            child.env_remove(key);
        }
        let output = crate::git::collect_output_with_deadline_and_limit(
            child,
            Duration::from_secs(120),
            1024 * 1024,
            "side diff fixture child",
        )
        .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() || !stdout.contains(DONE) {
            return Err(format!(
                "fixture child {}: {stdout}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return Ok(());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let repo =
        Repo(std::env::temp_dir().join(format!("ripr-side-diff-{}-{stamp}", std::process::id())));
    let root = &repo.0;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    git(root, &["init", "--initial-branch=main"])?;
    for (key, value) in [
        ("user.name", "Diff Contract"),
        ("user.email", "diff@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
        ("diff.renames", "true"),
    ] {
        git(root, &["config", "--local", key, value])?;
    }
    for name in ["alpha", "bravo"] {
        let text: String = (0..60)
            .map(|i| format!("pub const {}_{i}: u32 = {i};\n", name.to_uppercase()))
            .collect();
        fs::write(root.join(format!("{name}.rs")), text).map_err(|e| e.to_string())?;
    }
    for (name, bytes) in [
        ("pure.rs", b"// exact rename\n".as_slice()),
        ("gone.rs", b"// removed only\n"),
        ("blob.bin", &[0, 1, 2]),
        (".gitattributes", b"*.rs diff=audit\n"),
    ] {
        fs::write(root.join(name), bytes).map_err(|e| e.to_string())?;
    }
    git(root, &["add", "."])?;
    git(root, &["commit", "--quiet", "-m", "base"])?;
    let base = git(root, &["rev-parse", "HEAD"])?;
    git(root, &["update-ref", "refs/remotes/origin/main", &base])?;
    for (old, new) in [("alpha.rs", "xray space.rs"), ("bravo.rs", "yankée.rs")] {
        let text = fs::read_to_string(root.join(old)).map_err(|e| e.to_string())?;
        fs::remove_file(root.join(old)).map_err(|e| e.to_string())?;
        fs::write(root.join(new), format!("{text}// changed\n")).map_err(|e| e.to_string())?;
    }
    fs::rename(root.join("pure.rs"), root.join("-pure new.rs")).map_err(|e| e.to_string())?;
    fs::remove_file(root.join("gone.rs")).map_err(|e| e.to_string())?;
    fs::write(
        root.join("unrelated.rs"),
        "pub fn unrelated() -> bool { false }\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(root.join("blob.bin"), [0, 3, 4]).map_err(|e| e.to_string())?;
    git(root, &["add", "-A"])?;
    git(
        root,
        &["commit", "--quiet", "-m", "renames and nearest controls"],
    )?;
    let range = format!("{base}...HEAD");
    // Real unpinned old behavior: small exhaustive limit drops only edited
    // renames. A pure rename alone would give a false-green regression test.
    let raw = |limit: &str| {
        git(
            root,
            &[
                "-c",
                &format!("diff.renameLimit={limit}"),
                "diff",
                "-M",
                "--no-ext-diff",
                "--no-textconv",
                &range,
            ],
        )
    };
    let low = raw("1")?;
    let normal = raw("1000")?;
    assert_eq!(low.matches("rename from ").count(), 1, "{low}");
    assert_eq!(normal.matches("rename from ").count(), 3, "{normal}");
    let packet = capture(root)?;
    let tree = candidate(root, &base)?;
    let analysis = load_diff_range(root, &base, "HEAD")?;
    assert!(String::from_utf8_lossy(&packet).contains("GIT binary patch"));
    assert!(!tree.contains("GIT binary patch"));
    for patch in [String::from_utf8_lossy(&packet).as_ref(), &tree, &analysis] {
        assert_eq!(patch.matches("rename from ").count(), 3, "{patch}");
        assert!(patch.contains("rename to xray space.rs\n"), "{patch}");
        assert!(patch.contains("rename to -pure new.rs\n"), "{patch}");
        assert!(
            patch.contains("rename to \"yank\\303\\251e.rs\"\n"),
            "{patch}"
        );
        assert!(
            patch.contains("deleted file mode") && patch.contains("new file mode"),
            "{patch}"
        );
        assert!(!patch.contains("rename from gone.rs"), "{patch}");
    }
    for limit in ["1", "0", "999999"] {
        git(root, &["config", "--local", "diff.renameLimit", limit])?;
        assert_eq!(capture(root)?, packet, "packet under limit={limit}");
        assert_eq!(
            candidate(root, &base)?,
            tree,
            "candidate under limit={limit}"
        );
        assert_eq!(
            load_diff_range(root, &base, "HEAD")?,
            analysis,
            "analysis under limit={limit}"
        );
        assert_eq!(
            load_pr_evidence_diff_range(root, &base, "HEAD")?.as_bytes(),
            packet,
            "packet/loader parity under limit={limit}"
        );
    }
    for (key, value) in [
        ("diff.orderFile", "missing-order-file"),
        ("core.quotePath", "false"),
        ("diff.noprefix", "true"),
        ("color.diff", "always"),
        ("diff.context", "40"),
        ("diff.interHunkContext", "100"),
        ("diff.external", "exit 67"),
        ("diff.audit.textconv", "exit 68"),
    ] {
        git(root, &["config", "--local", key, value])?;
    }
    assert_eq!(capture(root)?, packet, "hostile packet presentation");
    assert_eq!(
        candidate(root, &base)?,
        tree,
        "hostile candidate presentation"
    );
    assert_eq!(
        load_diff_range(root, &base, "HEAD")?,
        analysis,
        "hostile analysis presentation"
    );
    // These helpers really fail when enabled: the positive routes above
    // must suppress them, not accidentally test inert configuration.
    for helper in ["--ext-diff", "--textconv"] {
        let other = if helper == "--ext-diff" {
            "--no-textconv"
        } else {
            "--no-ext-diff"
        };
        assert!(
            git(
                root,
                &[
                    "-c",
                    "diff.renameLimit=1000",
                    "-c",
                    "diff.orderFile=/dev/null",
                    "diff",
                    helper,
                    other,
                    &range
                ]
            )
            .is_err()
        );
    }
    // Git parses malformed repository values before command-line overrides.
    // Ref resolution still succeeds, but both diff subprocesses must fail
    // closed instead of turning missing evidence into an empty success.
    git(root, &["config", "--local", "diff.renameLimit", "invalid"])?;
    assert!(!git(root, &["rev-parse", "--verify", "HEAD^{commit}"])?.is_empty());
    let error = capture(root).err().ok_or("packet accepted a failed diff")?;
    assert!(
        error.contains("git diff failed") && error.contains("diff.renamelimit"),
        "{error}"
    );
    let error = candidate(root, &base)
        .err()
        .ok_or("candidate accepted a failed diff-tree")?;
    assert!(
        error.contains("failed with") && error.contains("diff.renamelimit"),
        "{error}"
    );
    assert!(load_diff_range(root, &base, "HEAD").is_err());
    git(root, &["config", "--local", "diff.renameLimit", "1"])?;
    // Explicit -M on immutable candidates remains independent of ordinary
    // diff's accepted diff.renames=false setting.
    git(root, &["config", "--local", "diff.renames", "false"])?;
    assert_eq!(candidate(root, &base)?, tree);
    let no_renames = capture(root)?;
    assert_eq!(
        String::from_utf8_lossy(&no_renames)
            .matches("rename from ")
            .count(),
        0
    );
    assert_eq!(
        load_pr_evidence_diff_range(root, &base, "HEAD")?.as_bytes(),
        no_renames
    );
    assert_eq!(
        load_diff_range(root, &base, "HEAD")?
            .matches("rename from ")
            .count(),
        0
    );
    // Empty success exercises the names-only route. Missing refs are errors,
    // never empty successful evidence; the caller's error wrapper stays intact.
    git(root, &["update-ref", "refs/remotes/origin/main", "HEAD"])?;
    assert!(capture(root)?.is_empty());
    git(root, &["update-ref", "-d", "refs/remotes/origin/main"])?;
    assert!(capture(root).is_err());
    assert!(candidate(root, "missing-base").is_err());
    assert!(load_diff_range(root, "missing-base", "HEAD").is_err());
    println!("{DONE}");
    Ok(())
}

/// The finite resource policy is itself a contract. Small runtime fixtures
/// cannot distinguish 1000 from unlimited, so pin this literal separately.
#[test]
fn shared_diff_rename_budget_is_finite() {
    for command in ["diff", "diff-tree"] {
        let args = crate::analysis::pinned_git_diff_args(command, &[], "--unified=0");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-c", "diff.renameLimit=1000"])
        );
    }
}
