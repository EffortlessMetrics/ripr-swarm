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
    if !s.compare(&original, &head)?.is_empty()
        || !s.compare(&original, &sibling)?.is_empty()
        || s.compare(&head, &sibling)? != BTreeSet::from(["file name".to_owned()])
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
