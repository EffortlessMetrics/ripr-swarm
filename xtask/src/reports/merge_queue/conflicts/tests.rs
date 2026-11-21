use super::*;

fn page(base: &str, pr: u64, pos: u64, count: u64, next: bool) -> Value {
    json!({"data":{"repository":{"nameWithOwner":REPO,"ref":{"name":"main","target":{"oid":base}},
        "mergeQueue":{"resourcePath":"/EffortlessMetrics/ripr-swarm/queue/main","entries":{"totalCount":count,"pageInfo":{"hasNextPage":next,"endCursor":pos.to_string()},
        "nodes":[{"position":pos,"pullRequest":{"number":pr,"headRefOid":base}}]}}}}})
}

#[test]
fn incomplete_or_moved_census_is_refused() -> Result<(), String> {
    let base = "a".repeat(40);
    let first = page(&base, 3, 1, 2, true);
    let second = page(&base, 5, 2, 2, false);
    let mut partial = first.clone();
    partial["errors"] = json!([{"message":"unavailable"}]);
    let mut wrong_queue = first.clone();
    wrong_queue["data"]["repository"]["mergeQueue"]["resourcePath"] = json!("/other/queue/main");
    let mut bad_cursor = first.clone();
    bad_cursor["data"]["repository"]["mergeQueue"]["entries"]["pageInfo"]["endCursor"] =
        Value::Null;
    for pages in [
        vec![partial, second.clone()],
        vec![wrong_queue, second.clone()],
        vec![bad_cursor, second.clone()],
        vec![first.clone()],
        vec![first.clone(), page(&"b".repeat(40), 5, 2, 2, false)],
        vec![first.clone(), page(&base, 3, 2, 2, false)],
        vec![first.clone(), page(&base, 5, 3, 2, false)],
        vec![page("HEAD", 3, 1, 1, false)],
    ] {
        if parse_pages(&serde_json::to_vec(&pages).map_err(|e| e.to_string())?).is_ok() {
            return Err("inconsistent queue pages accepted".into());
        }
    }
    let (b, rows) = parse_pages(&serde_json::to_vec(&[first, second]).map_err(|e| e.to_string())?)?;
    if b != base || rows.len() != 2 {
        return Err("complete frozen census lost an entry".into());
    }
    Ok(())
}

// Build actual Git objects without developing branches or changing user identity.
fn commit(sweep: &Sweep<'_>, value: &str, parent: Option<&str>) -> Result<String, String> {
    fn oid(bytes: Vec<u8>) -> Result<String, String> {
        String::from_utf8(bytes)
            .map(|s| s.trim().to_owned())
            .map_err(|e| e.to_string())
    }
    let blob = oid(sweep.checked(
        "git",
        &["hash-object", "-w", "--stdin"],
        Some(value.as_bytes()),
    )?)?;
    let attr = oid(sweep.checked(
        "git",
        &["hash-object", "-w", "--stdin"],
        Some(b"* merge=riprQueueFixture\nunion.txt merge=union\n"),
    )?)?;
    let tree = oid(sweep.checked(
        "git",
        &["mktree"],
        Some(
            format!("100644 blob {attr}\t.gitattributes\n100644 blob {blob}\tfile name\n100644 blob {blob}\tunion.txt\n")
                .as_bytes(),
        ),
    )?)?;
    let mut args = vec![
        "-c",
        "user.name=Queue fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit-tree",
        &tree,
        "-m",
        value,
    ];
    if let Some(p) = parent {
        args.extend(["-p", p]);
    }
    oid(sweep.checked("git", &args, None)?)
}

fn row(head: &str) -> Row {
    Row {
        position: 1,
        pr: 1,
        head: head.to_owned(),
        result: Outcome::Unexamined {
            reason: "pending".into(),
        },
    }
}

#[test]
fn first_parent_transition_tracks_reintroduced_conflict_and_never_credits_a_prefix()
-> Result<(), String> {
    let dir = crate::tests::temp_dir("queue-conflicts");
    let s = Sweep {
        objects: &dir,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    s.checked("git", &["init", "--bare", "--template=", "."], None)?;
    let original = commit(&s, "original", None)?;
    let head = commit(&s, "candidate", Some(&original))?;
    let sibling = commit(&s, "sibling", Some(&original))?;
    if s.compare(&original, &head)? != Comparison::Clean
        || s.compare(&original, &sibling)? != Comparison::Clean
        || s.compare(&head, &sibling)?
            != Comparison::Conflict(BTreeSet::from(["file name".to_owned()]))
    {
        return Err("individually clean heads were confused with cumulative composition".into());
    }
    let first = commit(&s, "different", Some(&original))?;
    let recovered = commit(&s, "original", Some(&first))?;
    let second = commit(&s, "different again", Some(&recovered))?;
    let tip = commit(&s, "still different", Some(&second))?;
    let mut rows = [row(&head), row(&tip), row(&"f".repeat(40))];
    s.classify(&tip, &mut rows);
    s.boundaries(&tip, &mut rows)?;
    let [conflicted, clean, missing] = &rows;
    let Outcome::Conflict { paths, .. } = &conflicted.result else {
        return Err("missing conflict".into());
    };
    let boundary = paths
        .get("file name")
        .and_then(Option::as_ref)
        .ok_or("missing verified transition")?;
    if boundary.first_parent != recovered
        || boundary.introduced_at != second
        || !matches!(clean.result, Outcome::Clean)
        || !matches!(missing.result, Outcome::Unexamined { .. })
    {
        return Err("wrong latest continuous conflict transition or uncertainty".into());
    }
    let exhausted = Sweep {
        objects: &dir,
        deadline: Instant::now(),
    };
    s.classify(&tip, &mut rows);
    if exhausted.attribute_history(&tip, &mut rows).is_ok()
        || !matches!(&rows.first().ok_or("missing conflict")?.result,Outcome::Conflict{unattributed_reason:Some(why),..} if why.contains("budget"))
    {
        return Err("expired history lookup lost the row-specific attribution reason".into());
    }
    exhausted.classify(&tip, &mut rows);
    if rows
        .iter()
        .any(|row| !matches!(row.result, Outcome::Unexamined { .. }))
    {
        return Err("budget exhaustion was treated as a comparison".into());
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

#[test]
fn empty_or_failed_replay_releases_only_its_own_object_store() -> Result<(), String> {
    let dir = crate::tests::temp_dir("queue-report-cleanup");
    if run(&["--input".into()]).err().as_deref() != Some(USAGE) {
        return Err("bare --input did not return usage".into());
    }
    let input = dir.join("pages.json");
    let mut empty = page(&"a".repeat(40), 1, 1, 0, false);
    empty["data"]["repository"]["mergeQueue"]["entries"]["nodes"] = json!([]);
    fs::write(
        &input,
        serde_json::to_vec(&[empty]).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let args = |out: &Path| {
        vec![
            out.to_string_lossy().into_owned(),
            "--input".into(),
            input.to_string_lossy().into_owned(),
        ]
    };
    let good = dir.join("valid");
    run(&args(&good))?;
    if good.join("objects.git").exists()
        || !good.join("queue-conflicts.json").is_file()
        || !good.join("queue-pages.json").is_file()
    {
        return Err("expected only durable evidence after an empty complete census".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&good)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err("private queue evidence is accessible to other accounts".into());
        }
    }
    fs::write(&input, b"[]").map_err(|e| e.to_string())?;
    let bad = dir.join("invalid");
    if run(&args(&bad)).is_ok()
        || bad.join("objects.git").exists()
        || bad.join("queue-conflicts.json").exists()
    {
        return Err(
            "malformed replay retained a scratch store or reported a completed comparison".into(),
        );
    }
    if run(&args(&good)).is_ok() || !good.join("queue-conflicts.json").is_file() {
        return Err("existing output directory was reused or changed".into());
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

#[test]
fn directory_conflicts_preserve_status_and_true_history() -> Result<(), String> {
    let dir = crate::tests::temp_dir("queue-dir-conflicts");
    let sweep = Sweep {
        objects: &dir,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    sweep.checked("git", &["init", "--bare", "--template=", "."], None)?;
    sweep.checked(
        "git",
        &["fast-import", "--quiet"],
        Some(
            br#"commit refs/heads/original
mark :1
committer Fixture <fixture@example.invalid> 1 +0000
data 4
base
M 100644 inline z/b
data 1
b
M 100644 inline z/c
data 1
c

commit refs/heads/base
mark :2
committer Fixture <fixture@example.invalid> 2 +0000
data 4
move
from :1
R z/b y/b
R z/c w/c

commit refs/heads/pr
committer Fixture <fixture@example.invalid> 3 +0000
data 3
add
from :1
M 100644 inline z/d
data 1
d

commit refs/heads/file
mark :4
committer Fixture <fixture@example.invalid> 4 +0000
data 4
file
from :1
M 100644 inline tools
data 1
f

commit refs/heads/later
committer Fixture <fixture@example.invalid> 5 +0000
data 5
later
from :4
M 100644 inline unrelated
data 1
u

commit refs/heads/directory
committer Fixture <fixture@example.invalid> 6 +0000
data 9
directory
from :1
M 100644 inline tools/man
data 1
m

commit refs/heads/occupied
committer Fixture <fixture@example.invalid> 7 +0000
data 8
occupied
from refs/heads/later
M 100644 inline tools~observed-main
data 1
x

commit refs/heads/raw-name-main
committer Fixture <fixture@example.invalid> 8 +0000
data 3
raw
from :1
M 100644 inline "odd\377"
data 1
a

commit refs/heads/raw-name-head
committer Fixture <fixture@example.invalid> 9 +0000
data 3
raw
from :1
M 100644 inline "odd\377"
data 1
b

done
"#,
        ),
    )?;
    let rev = |name: &str| -> Result<String, String> {
        String::from_utf8(sweep.checked("git", &["rev-parse", name], None)?)
            .map(|s| s.trim().to_owned())
            .map_err(|e| e.to_string())
    };
    let base = rev("base")?;
    let mut split = [row(&rev("pr")?)];
    sweep.classify(&base, &mut split);
    sweep.boundaries(&base, &mut split)?;
    let [split] = &split;
    if !matches!(&split.result,Outcome::Conflict{paths,unattributed_reason:Some(why)} if paths.is_empty() && why.contains("without individual"))
    {
        return Err("directory rename split lost Git's conflict without a path inventory".into());
    }
    let later = rev("later")?;
    let mut collision = [row(&rev("directory")?)];
    sweep.classify(&later, &mut collision);
    sweep.boundaries(&later, &mut collision)?;
    let [collision] = &collision;
    let Outcome::Conflict { paths, .. } = &collision.result else {
        return Err("missing collision".into());
    };
    let edge = paths
        .get("tools~observed-main")
        .and_then(Option::as_ref)
        .ok_or("missing stable collision boundary")?;
    if edge.introduced_at != rev("file")? || edge.first_parent != rev("original")? {
        return Err("unrelated first-parent commit credited for a continuing collision".into());
    }
    for (base, head, reason) in [
        ("occupied", "directory", "ambiguous"),
        ("raw-name-main", "raw-name-head", "non-UTF-8"),
    ] {
        let mut rows = [row(&rev(head)?)];
        let base = rev(base)?;
        sweep.classify(&base, &mut rows);
        sweep.attribute_history(&base, &mut rows)?;
        let [case] = rows;
        if !matches!(case.result, Outcome::Conflict{paths,unattributed_reason:Some(why)} if paths.values().all(Option::is_none) && why.contains(reason))
        {
            return Err(format!("missing conflict uncertainty for {head}: {reason}"));
        }
    }
    let store = dir.join("replay");
    fs::create_dir(&store).map_err(|e| e.to_string())?;
    let replay = Sweep {
        objects: &store,
        deadline: sweep.deadline,
    };
    let available = Row {
        pr: 2,
        ..row(&rev("directory")?)
    };
    let mut batch = [row(&"f".repeat(40)), available];
    replay.prepare(&later, &batch, dir.to_str().ok_or("non-UTF8 fixture")?)?;
    replay.classify(&later, &mut batch);
    let [missing, available] = &batch;
    if !matches!(missing.result, Outcome::Unexamined { .. })
        || !matches!(available.result, Outcome::Conflict { .. })
    {
        return Err("missing remote SHA lost independently fetchable head".into());
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

#[test]
fn configured_external_driver_is_ignored_by_actual_comparison() -> Result<(), String> {
    let dir = crate::tests::temp_dir("queue-driver-control");
    let file = dir.join("fixture.gitconfig");
    fs::write(&file, b"[merge \"riprQueueFixture\"]\n driver = true\n")
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args=vec!["--exact".into(), "reports::merge_queue::conflicts::tests::first_parent_transition_tracks_reintroduced_conflict_and_never_credits_a_prefix".into()];
    let result = capture_bytes_in_dir_with_input_env_budget(
        &exe,
        &args,
        (&dir, None),
        (
            &[(
                "GIT_CONFIG_GLOBAL",
                file.to_str().ok_or("non-UTF8 fixture")?,
            )],
            &[],
        ),
        ByteCaptureBudget {
            timeout: Duration::from_secs(110),
            stdout_bytes: 65536,
            stderr_bytes: 65536,
        },
        "queue configured driver regression",
    )?;
    if !result.status.is_some_and(|s| s.success())
        || !String::from_utf8_lossy(&result.stdout).contains("1 passed")
    {
        return Err(format!(
            "configured fixture driver bypassed isolated comparison: {}",
            String::from_utf8_lossy(&result.stdout)
        ));
    }
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}
