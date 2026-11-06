//! Terminal replay record. This is not authenticated artifact or job custody.
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Excluded {
    pub id: usize,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct KnownGap {
    pub id: usize,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShellReplay {
    pub name: &'static str,
    pub executed: Vec<usize>,
    pub excluded: Vec<Excluded>,
    pub known_gaps: Vec<KnownGap>,
}

/// A second completion check for the strict, bidirectional gap ledger.
/// This audit is updated by the production classifier, independently of the
/// human-readable problem list.
pub(super) struct GapAudit {
    hits: Vec<usize>,
    unexpected: bool,
}

impl GapAudit {
    pub(super) fn new(rows: usize) -> Self {
        Self {
            hits: vec![0; rows],
            unexpected: false,
        }
    }

    fn ensure_complete(&self) -> Result<(), String> {
        if self.unexpected || self.hits.contains(&0) {
            Err("known-gap audit has unexplained or healed failures".to_owned())
        } else {
            Ok(())
        }
    }
}

/// Split an observed failure and retain accepted known-gap cases for the
/// terminal document. Unexplained failures are independently audited.
pub(super) fn classify_known_gap<'a>(
    problems: &'a [String],
    prefixes: &[&str],
    case_id: usize,
    reason: &'static str,
    observed: &mut Vec<KnownGap>,
    audit: &mut GapAudit,
    gap_index: usize,
) -> (Vec<&'a String>, Vec<&'a String>) {
    let (explained, unexplained): (Vec<_>, Vec<_>) = problems
        .iter()
        .partition(|problem| prefixes.iter().any(|prefix| problem.starts_with(prefix)));
    if !explained.is_empty() {
        observed.push(KnownGap {
            id: case_id,
            reason,
        });
        if let Some(hits) = audit.hits.get_mut(gap_index) {
            *hits += 1;
        } else {
            audit.unexpected = true;
        }
    }
    audit.unexpected |= !unexplained.is_empty();
    (explained, unexplained)
}

pub(super) fn known_gap_unreproduced(hits: usize) -> bool {
    hits == 0
}

#[derive(Clone, Debug)]
struct Context {
    repository: String,
    head: String,
    base: String,
    group: String,
    tree: String,
    workflow_generation: String,
    request: String,
    run_id: String,
    run_attempt: String,
    os: String,
    runner_os: String,
    runtime_os: String,
    required_shells: Vec<String>,
}

fn full_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok()
}

fn repository_name(value: &str) -> bool {
    let mut parts = value.split('/');
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    };
    matches!((parts.next(), parts.next(), parts.next()), (Some(owner), Some(repo), None) if valid(owner) && valid(repo))
        && value.len() <= 200
}

fn context(get: impl Fn(&str) -> Option<String>, runtime_os: &str) -> Result<Context, String> {
    let field = |name: &str| {
        get(name)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("missing {name}"))
    };
    let event = field("GITHUB_EVENT_NAME")?;
    let profile = field("RIPR_PASTE_PROFILE")?;
    let repository = field("GITHUB_REPOSITORY")?;
    let head = field("RIPR_PASTE_H")?;
    let base = field("RIPR_PASTE_B")?;
    let group = field("RIPR_PASTE_G")?;
    let tree = field("RIPR_PASTE_TREE")?;
    let workflow_generation = field("RIPR_PASTE_GENERATION")?;
    let request = field("RIPR_PASTE_REQUEST")?;
    let run_id = field("GITHUB_RUN_ID")?;
    let run_attempt = field("GITHUB_RUN_ATTEMPT")?;
    let github_sha = field("GITHUB_SHA")?;
    let github_workflow_sha = field("GITHUB_WORKFLOW_SHA")?;
    let os = field("RIPR_PASTE_OS")?;
    let runner_os = field("RUNNER_OS")?;
    let required = field("RIPR_PASTE_REQUIRE")?;
    let required_shells: Vec<_> = required.split(',').map(str::to_owned).collect();
    let valid_shell = |name: &str| matches!(name, "bash" | "sh" | "zsh" | "pwsh" | "powershell");
    let unique: BTreeSet<_> = required_shells.iter().collect();
    if event != "merge_group"
        || profile != "merge-group"
        || !repository_name(&repository)
        || ![&head, &base, &group, &tree, &workflow_generation]
            .into_iter()
            .all(|sha| full_sha(sha))
        || github_sha != group
        || github_workflow_sha != workflow_generation
        || !positive_decimal(&run_id)
        || !positive_decimal(&run_attempt)
        || request != format!("{run_id}-{run_attempt}")
        || !matches!(
            (os.as_str(), runner_os.as_str(), runtime_os),
            ("ubuntu-latest", "Linux", "linux")
                | ("macos-latest", "macOS", "macos")
                | ("windows-latest", "Windows", "windows")
        )
        || required_shells.is_empty()
        || unique.len() != required_shells.len()
        || !required_shells.iter().all(|name| valid_shell(name))
    {
        return Err("invalid native replay receipt context".to_owned());
    }
    Ok(Context {
        repository,
        head,
        base,
        group,
        tree,
        workflow_generation,
        request,
        run_id,
        run_attempt,
        os,
        runner_os,
        runtime_os: runtime_os.to_owned(),
        required_shells,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryEntry {
    case_id: usize,
    source: String,
    origin: String,
    bash: String,
    powershell: Option<String>,
    bash_only: bool,
    powershell_unavailable: bool,
}

fn parse_inventory(bytes: &[u8], expected_count: usize) -> Result<Vec<(bool, bool)>, String> {
    if bytes.len() > 16 * 1024 * 1024 || expected_count > 100_000 {
        return Err("inventory exceeds its inspection bound".to_owned());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "inventory is not UTF-8".to_owned())?;
    let mut flags = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let entry: InventoryEntry =
            serde_json::from_str(line).map_err(|err| format!("malformed inventory: {err}"))?;
        if entry.case_id != index
            || entry.source.is_empty()
            || entry.bash.is_empty()
            || !matches!(entry.origin.as_str(), "Foreign" | "Portable")
            || entry.powershell.as_ref().is_some_and(String::is_empty)
        {
            return Err("inventory case identity, command, or eligibility is invalid".to_owned());
        }
        flags.push((entry.bash_only, entry.powershell_unavailable));
    }
    if expected_count < 15 || flags.len() != expected_count {
        return Err(
            "inventory denominator is missing or differs from collected commands".to_owned(),
        );
    }
    Ok(flags)
}

fn terminal_document(
    ctx: &Context,
    inventory: &[u8],
    expected_count: usize,
    shells: &[ShellReplay],
) -> Result<Value, String> {
    let flags = parse_inventory(inventory, expected_count)?;
    let mut names = BTreeSet::new();
    let mut shell_values = Vec::new();
    for shell in shells {
        if !matches!(shell.name, "bash" | "sh" | "zsh" | "pwsh" | "powershell")
            || !names.insert(shell.name)
        {
            return Err("unknown or duplicate executed shell".to_owned());
        }
        let powershell = matches!(shell.name, "pwsh" | "powershell");
        let mut expected_executed = Vec::new();
        let mut expected_excluded = Vec::new();
        for (id, &(bash_only, unavailable)) in flags.iter().enumerate() {
            let reason = match (powershell, bash_only, unavailable) {
                (true, true, true) => Some("bash_only_and_powershell_unavailable"),
                (true, true, false) => Some("bash_only"),
                (true, false, true) => Some("powershell_unavailable"),
                _ => None,
            };
            if let Some(reason) = reason {
                expected_excluded.push(Excluded { id, reason });
            } else {
                expected_executed.push(id);
            }
        }
        if expected_executed.is_empty()
            || shell.executed != expected_executed
            || shell.excluded != expected_excluded
        {
            return Err(format!(
                "{} replay denominator differs from inventory",
                shell.name
            ));
        }
        let mut gap_ids = BTreeSet::new();
        for gap in &shell.known_gaps {
            if !shell.executed.contains(&gap.id)
                || !gap_ids.insert(gap.id)
                || gap.reason.trim().is_empty()
            {
                return Err(format!("{} known-gap observation is invalid", shell.name));
            }
        }
        shell_values.push(json!({"shell":shell.name, "executed_case_ids":shell.executed,
            "excluded":shell.excluded.iter().map(|item| json!({"case_id":item.id,"reason":item.reason})).collect::<Vec<_>>(),
            "known_gaps":shell.known_gaps.iter().map(|item| json!({"case_id":item.id,"reason":item.reason})).collect::<Vec<_>>() }));
    }
    if !ctx
        .required_shells
        .iter()
        .all(|required| names.contains(required.as_str()))
    {
        return Err("a required shell has no completed replay record".to_owned());
    }
    let digest = format!("{:x}", Sha256::digest(inventory));
    Ok(
        json!({"schema_id":"ripr.printed-command-terminal.v1", "schema_version":1,
        "claim":"main_replay_test_completed_with_disclosed_known_gaps", "repository":ctx.repository,
        "head_sha":ctx.head, "base_sha":ctx.base, "group_sha":ctx.group, "tree_sha":ctx.tree,
        "workflow_generation":ctx.workflow_generation, "request_id":ctx.request,
        "run_id":ctx.run_id, "run_attempt":ctx.run_attempt, "job_key":"paste", "matrix_os":ctx.os,
        "runner_os":ctx.runner_os, "runtime_os":ctx.runtime_os,
        "inventory_sha256":digest, "inventory_case_count":expected_count,
        "required_shells":ctx.required_shells, "shells":shell_values }),
    )
}

/// A destination that was cleared before fixture setup. It is created only
/// after the replay function has no unexpected problems. Cargo/job/deadline
/// status and artifact custody remain separate obligations.
pub(super) struct ReceiptTarget {
    context: Context,
    source: PathBuf,
    destination: PathBuf,
}

fn safe_temp_path(path: &Path, temp: &Path, name: &str) -> Result<(), String> {
    if path.file_name().and_then(|name| name.to_str()) != Some(name) {
        return Err("receipt path has an unexpected filename".to_owned());
    }
    let parent = path.parent().ok_or("receipt path has no parent")?;
    let actual =
        std::fs::canonicalize(parent).map_err(|error| format!("receipt parent: {error}"))?;
    let expected = std::fs::canonicalize(temp).map_err(|error| format!("runner temp: {error}"))?;
    if actual != expected {
        return Err("receipt path is outside runner temp".to_owned());
    }
    Ok(())
}

fn clear_stale(destination: &Path, temp: &Path) -> Result<(), String> {
    safe_temp_path(destination, temp, "printed-command-receipt.json")?;
    match std::fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_file() => std::fs::remove_file(destination)
            .map_err(|error| format!("remove stale terminal receipt: {error}")),
        Ok(_) => Err("stale terminal receipt is not a regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect stale terminal receipt: {error}")),
    }
}

/// Called before fixture setup; stale files in the fixed runner-temp location
/// are removed even if later context or replay validation fails.
pub(super) fn prepare_from_environment() -> Result<Option<ReceiptTarget>, String> {
    let Some(destination) =
        std::env::var_os("RIPR_PASTE_RECEIPT").filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let destination = PathBuf::from(destination);
    let temp = std::env::var_os("RUNNER_TEMP")
        .filter(|value| !value.is_empty())
        .ok_or("missing runner temp")?;
    let temp = Path::new(&temp);
    clear_stale(&destination, temp)?;
    let ctx = context(|name| std::env::var(name).ok(), std::env::consts::OS)?;
    let source = std::env::var_os("RIPR_PASTE_REPORT")
        .filter(|value| !value.is_empty())
        .ok_or("missing inventory path")?;
    let source = PathBuf::from(source);
    safe_temp_path(&source, temp, "printed-commands.jsonl")?;
    Ok(Some(ReceiptTarget {
        context: ctx,
        source,
        destination,
    }))
}

impl ReceiptTarget {
    pub(super) fn write(
        self,
        expected_inventory: &[u8],
        audit: &GapAudit,
        expected_count: usize,
        shells: &[ShellReplay],
    ) -> Result<(), String> {
        audit.ensure_complete()?;
        let metadata = std::fs::symlink_metadata(&self.source)
            .map_err(|err| format!("inspect inventory: {err}"))?;
        if !metadata.file_type().is_file() || metadata.len() > 16 * 1024 * 1024 {
            return Err("inventory must be a bounded regular file".to_owned());
        }
        // Bound the read itself, not only the preceding metadata snapshot.
        let mut bytes = Vec::new();
        std::fs::File::open(&self.source)
            .map_err(|err| format!("open inventory: {err}"))?
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| format!("read inventory: {err}"))?;
        if bytes != expected_inventory {
            return Err("inventory changed after command collection".to_owned());
        }
        let document = terminal_document(&self.context, &bytes, expected_count, shells)?;
        write_new(&self.destination, &document)
    }
}

fn write_new(destination: &Path, document: &Value) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|err| format!("create terminal replay receipt: {err}"))?;
    file.write_all(document.to_string().as_bytes())
        .map_err(|err| format!("write terminal replay receipt: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn fields() -> BTreeMap<&'static str, String> {
        BTreeMap::from([
            ("GITHUB_EVENT_NAME", "merge_group".to_owned()),
            ("RIPR_PASTE_PROFILE", "merge-group".to_owned()),
            ("RUNNER_OS", "Linux".to_owned()),
            (
                "GITHUB_REPOSITORY",
                "EffortlessMetrics/ripr-swarm".to_owned(),
            ),
            ("RIPR_PASTE_H", "a".repeat(40)),
            ("RIPR_PASTE_B", "b".repeat(40)),
            ("RIPR_PASTE_G", "c".repeat(40)),
            ("RIPR_PASTE_TREE", "d".repeat(40)),
            ("RIPR_PASTE_GENERATION", "e".repeat(40)),
            ("RIPR_PASTE_REQUEST", "123-2".to_owned()),
            ("GITHUB_RUN_ID", "123".to_owned()),
            ("GITHUB_RUN_ATTEMPT", "2".to_owned()),
            ("GITHUB_SHA", "c".repeat(40)),
            ("GITHUB_WORKFLOW_SHA", "e".repeat(40)),
            ("RIPR_PASTE_OS", "ubuntu-latest".to_owned()),
            ("RIPR_PASTE_REQUIRE", "bash,pwsh".to_owned()),
        ])
    }

    fn ctx(fields: &BTreeMap<&str, String>) -> Result<Context, String> {
        context(|name| fields.get(name).cloned(), "linux")
    }

    fn inventory_bytes() -> Vec<u8> {
        (0..15)
            .map(|id| {
                json!({"case_id":id,"source":"fixture","origin":"Foreign",
            "bash":"ripr check","powershell":null,"bash_only":id==3,
            "powershell_unavailable":id==4})
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }

    fn shells() -> Vec<ShellReplay> {
        vec![
            ShellReplay {
                name: "bash",
                executed: (0..15).collect(),
                excluded: vec![],
                known_gaps: vec![KnownGap {
                    id: 2,
                    reason: "fixture known gap",
                }],
            },
            ShellReplay {
                name: "pwsh",
                executed: (0..15).filter(|id| *id != 3 && *id != 4).collect(),
                excluded: vec![
                    Excluded {
                        id: 3,
                        reason: "bash_only",
                    },
                    Excluded {
                        id: 4,
                        reason: "powershell_unavailable",
                    },
                ],
                known_gaps: vec![],
            },
        ]
    }

    #[test]
    fn completed_shell_replays_preserve_exclusions_and_known_gaps() -> Result<(), String> {
        let document = terminal_document(&ctx(&fields())?, &inventory_bytes(), 15, &shells())?;
        assert_eq!(
            document["claim"],
            "main_replay_test_completed_with_disclosed_known_gaps"
        );
        assert_eq!(document["shells"][0]["known_gaps"][0]["case_id"], 2);
        assert_eq!(document["shells"][1]["excluded"][0]["reason"], "bash_only");
        assert_eq!(document["inventory_case_count"], 15);
        assert_eq!(
            document["inventory_sha256"],
            "f19682d76d330a2b2cabdd6c9daffe968756cbd63d10e60cc80d2f43f736f562"
        );
        let changed = String::from_utf8(inventory_bytes())
            .map_err(|error| error.to_string())?
            .replacen("ripr check", "ripr changed", 1);
        let other = terminal_document(&ctx(&fields())?, changed.as_bytes(), 15, &shells())?;
        assert_ne!(document["inventory_sha256"], other["inventory_sha256"]);
        Ok(())
    }

    #[test]
    fn context_rejects_wrong_native_subject_request_or_required_shells() {
        for (key, value) in [
            ("GITHUB_EVENT_NAME", "push"),
            ("GITHUB_SHA", "wrong"),
            ("GITHUB_WORKFLOW_SHA", "wrong"),
            ("RIPR_PASTE_TREE", "main"),
            ("RIPR_PASTE_REQUEST", "123-1"),
            ("GITHUB_RUN_ATTEMPT", "02"),
            ("RIPR_PASTE_REQUIRE", "bash,bash"),
            ("RIPR_PASTE_REQUIRE", "bash,unknown"),
            ("RIPR_PASTE_OS", "other"),
            ("RUNNER_OS", "Windows"),
        ] {
            let mut data = fields();
            data.insert(key, value.to_owned());
            assert!(ctx(&data).is_err(), "accepted {key}={value}");
        }
    }

    #[test]
    fn valid_platform_triples_and_mismatches_are_checked() {
        for (matrix, runner, runtime) in [
            ("ubuntu-latest", "Linux", "linux"),
            ("macos-latest", "macOS", "macos"),
            ("windows-latest", "Windows", "windows"),
        ] {
            let mut data = fields();
            data.insert("RIPR_PASTE_OS", matrix.to_owned());
            data.insert("RUNNER_OS", runner.to_owned());
            assert!(context(|key| data.get(key).cloned(), runtime).is_ok());
            assert!(context(|key| data.get(key).cloned(), "unexpected").is_err());
        }
    }

    #[test]
    fn synthetic_gap_disclosure_and_completion_fail_closed() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-paste-gap-{}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|error| error.to_string())?;
        let source = dir.join("printed-commands.jsonl");
        let destination = dir.join("printed-command-receipt.json");
        let bytes = inventory_bytes();
        std::fs::write(&source, &bytes).map_err(|error| error.to_string())?;
        let target = || -> Result<ReceiptTarget, String> {
            Ok(ReceiptTarget {
                context: ctx(&fields())?,
                source: source.clone(),
                destination: destination.clone(),
            })
        };
        let mut observed = Vec::new();
        let mut audit = GapAudit::new(1);
        let failures = vec!["known: expected fixture issue".to_owned()];
        let (explained, unexplained) = classify_known_gap(
            &failures,
            &["known:"],
            2,
            "fixture known gap",
            &mut observed,
            &mut audit,
            0,
        );
        assert_eq!(explained.len(), 1);
        assert!(unexplained.is_empty());
        assert_eq!(
            observed,
            vec![KnownGap {
                id: 2,
                reason: "fixture known gap"
            }]
        );
        target()?.write(&bytes, &audit, 15, &shells())?;
        assert!(destination.exists());
        std::fs::remove_file(&destination).map_err(|error| error.to_string())?;

        let mut audit = GapAudit::new(1);
        let failures = vec![
            "known: expected fixture issue".to_owned(),
            "different failure".to_owned(),
        ];
        classify_known_gap(
            &failures,
            &["known:"],
            2,
            "fixture known gap",
            &mut Vec::new(),
            &mut audit,
            0,
        );
        assert!(target()?.write(&bytes, &audit, 15, &shells()).is_err());
        assert!(!destination.exists());
        let healed = GapAudit::new(1);
        assert!(target()?.write(&bytes, &healed, 15, &shells()).is_err());
        assert!(!destination.exists());
        std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn missing_replay_and_wrong_exclusion_or_gap_fail() -> Result<(), String> {
        let ctx = ctx(&fields())?;
        let mut missing = shells();
        missing.pop();
        assert!(terminal_document(&ctx, &inventory_bytes(), 15, &missing).is_err());
        let mut omitted = shells();
        omitted[1].executed.pop();
        assert!(terminal_document(&ctx, &inventory_bytes(), 15, &omitted).is_err());
        let mut invented = shells();
        invented[1].excluded[0].reason = "powershell_unavailable";
        assert!(terminal_document(&ctx, &inventory_bytes(), 15, &invented).is_err());
        let mut false_gap = shells();
        false_gap[1].known_gaps.push(KnownGap {
            id: 3,
            reason: "not executed",
        });
        assert!(terminal_document(&ctx, &inventory_bytes(), 15, &false_gap).is_err());
        let mut duplicate_gap = shells();
        duplicate_gap[0].known_gaps.push(KnownGap {
            id: 2,
            reason: "duplicate",
        });
        assert!(terminal_document(&ctx, &inventory_bytes(), 15, &duplicate_gap).is_err());
        Ok(())
    }

    #[test]
    fn duplicate_or_missing_inventory_fields_cannot_define_the_denominator() {
        let bytes = inventory_bytes();
        let text = String::from_utf8_lossy(&bytes);
        let duplicate = text.replacen("\"case_id\":0", "\"case_id\":0,\"case_id\":0", 1);
        assert_ne!(duplicate, text);
        assert!(parse_inventory(duplicate.as_bytes(), 15).is_err());
        assert!(parse_inventory(&bytes, 16).is_err());
        assert!(parse_inventory(b"", 0).is_err());
    }

    #[test]
    fn stale_receipt_is_cleared_before_later_failure_and_outside_path_refuses() -> Result<(), String>
    {
        let temp = std::env::temp_dir().join(format!("ripr-paste-prepare-{}", std::process::id()));
        let other = std::env::temp_dir().join(format!("ripr-paste-other-{}", std::process::id()));
        std::fs::create_dir(&temp).map_err(|error| error.to_string())?;
        std::fs::create_dir(&other).map_err(|error| error.to_string())?;
        let stale = temp.join("printed-command-receipt.json");
        let outside = other.join("printed-command-receipt.json");
        std::fs::write(&stale, b"stale").map_err(|error| error.to_string())?;
        std::fs::write(&outside, b"outside").map_err(|error| error.to_string())?;
        clear_stale(&stale, &temp)?;
        assert!(!stale.exists(), "stale receipt remained after preparation");
        let mut invalid = fields();
        invalid.insert("GITHUB_SHA", "wrong".to_owned());
        assert!(ctx(&invalid).is_err());
        assert!(!stale.exists(), "later failure recreated a receipt");
        assert!(clear_stale(&outside, &temp).is_err());
        assert_eq!(
            std::fs::read(&outside).map_err(|error| error.to_string())?,
            b"outside"
        );
        std::fs::remove_dir_all(&temp).map_err(|error| error.to_string())?;
        std::fs::remove_dir_all(&other).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn substituted_inventory_never_gets_a_receipt() -> Result<(), String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-paste-substitution-{}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|error| error.to_string())?;
        let path = dir.join("printed-commands.jsonl");
        let destination = dir.join("printed-command-receipt.json");
        let original = inventory_bytes();
        let text = String::from_utf8(original.clone()).map_err(|error| error.to_string())?;
        let substituted = text.replacen("ripr check", "ripr altered", 1);
        assert_ne!(text, substituted);
        // The old structural-only validator accepted this same-count substitution.
        assert!(terminal_document(&ctx(&fields())?, substituted.as_bytes(), 15, &shells()).is_ok());
        std::fs::write(&path, substituted).map_err(|error| error.to_string())?;
        let target = ReceiptTarget {
            context: ctx(&fields())?,
            source: path,
            destination: destination.clone(),
        };
        assert!(
            target
                .write(&original, &GapAudit::new(0), 15, &shells())
                .is_err()
        );
        assert!(!destination.exists());
        std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn stale_destination_is_never_overwritten() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-paste-receipt-{}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|error| error.to_string())?;
        let path = dir.join("receipt.json");
        std::fs::write(&path, b"stale").map_err(|error| error.to_string())?;
        let result = write_new(&path, &json!({"fresh":true}));
        let retained = std::fs::read(&path).map_err(|error| error.to_string())?;
        std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
        assert!(result.is_err());
        assert_eq!(retained, b"stale");
        Ok(())
    }
}
