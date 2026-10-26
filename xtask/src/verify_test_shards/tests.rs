use super::*;

fn check(xml: &str) -> Result<(u64, String), Vec<String>> {
    check_junit(xml, &BTreeSet::from(["bin one".to_string()]), "control")
}

const CASE: &str = "<testcase classname=\"bin\" name=\"one\"/>";

#[test]
fn context_assignments_are_singletons_even_when_values_agree() -> Result<(), String> {
    let options = Options {
        dir: PathBuf::new(),
        artifact: "control".to_string(),
        attempt: "1".to_string(),
        repository: "o/r".to_string(),
        sha: "a".repeat(40),
        run_id: "7".to_string(),
        shards: 4,
        out: PathBuf::new(),
    };
    let valid = format!(
        "repository=o/r\ncheckout_sha={}\nrun_id=7\nrun_attempt=1\nprofile=ci\nshard=2/4\ncommand=cargo nextest run --workspace --profile ci --partition count:2/4\nnextest_exit=0\n",
        options.sha
    );
    let problems = |text: &str| {
        let mut problems = Vec::new();
        check_context(text, &options, 2, 1, "control", &mut problems);
        problems
    };
    assert!(problems(&valid).is_empty());
    for key in ["shards", "verified_attempts"] {
        for text in [format!("{valid}{key}=1\n"), format!("{key}=1\n{valid}")] {
            assert!(
                problems(&text)
                    .iter()
                    .any(|why| why.contains(&format!("`{key}` is reserved")))
            );
        }
    }
    // Value text may contain '=', while verbose version metadata uses ':' and
    // can repeat across tools. Neither changes scalar key cardinality.
    assert!(
        problems(&format!(
            "{valid}extension_key=left=right\nrelease: 1.95\nrelease: 1.95\ncargo version detail=one\ncargo version detail=two\n"
        ))
        .is_empty()
    );
    for line in valid.lines().chain(["extension_key=left=right"]) {
        let (key, _) = line
            .split_once('=')
            .ok_or_else(|| format!("assignment fixture has no delimiter: {line}"))?;
        let seed = if key == "extension_key" {
            format!("{valid}{line}\n")
        } else {
            valid.clone()
        };
        for duplicate in [line.to_string(), format!("{key}=wrong"), format!(" {line}")] {
            for text in [
                format!("{duplicate}\n{seed}"),
                format!("{seed}{duplicate}\n"),
            ] {
                let found = problems(&text);
                assert!(
                    found
                        .iter()
                        .any(|why| why.contains(&format!("duplicate key `{key}`"))),
                    "duplicate {key} was not rejected: {text}: {found:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn suite_extraction_uses_validated_xml_boundaries() -> Result<(), String> {
    let suite = format!(
        "<testsuite\n tests='1' name='bin' note='a > b'><properties><property name='a' value='b'/></properties>{CASE}<system-out><![CDATA[<testsuite fake> & </testsuites>]]></system-out></testsuite>"
    );
    let xml = format!(
        "<?xml version='1.0'?><!-- <testsuite bogus> --><testsuites tests='1'><!-- <testsuite bogus> -->{suite}</testsuites><!-- </testsuites> -->"
    );
    let (count, body) = check(&xml).map_err(|problems| problems.join("; "))?;
    assert_eq!(count, 1);
    assert_eq!(
        body,
        format!("{suite}\n"),
        "suite payload must survive exactly"
    );
    let combined = format!("<testsuites tests='1'>{body}</testsuites>");
    assert_eq!(parse_junit(&combined)?.identities, ["bin one"]);
    Ok(())
}

#[test]
fn malformed_or_root_dependent_xml_is_rejected() {
    for xml in [
        format!(
            "<testsuites tests='1'><testsuite tests='1' tests='1'>{CASE}</testsuite></testsuites>"
        ),
        format!(
            "<testsuites tests='1'><testsuite tests='1' note='a & b'>{CASE}</testsuite></testsuites>"
        ),
        format!(
            "<testsuites xmlns='urn:other' tests='1'><testsuite tests='1'>{CASE}</testsuite></testsuites>"
        ),
        format!(
            "<testsuites xmlns:x='urn:other' tests='1'><testsuite tests='1' x:note='a'>{CASE}</testsuite></testsuites>"
        ),
        format!(
            "<testsuites xml:lang='en' tests='1'><testsuite tests='1'>{CASE}</testsuite></testsuites>"
        ),
        format!(
            "<!DOCTYPE testsuites [<!ENTITY fake 'one'>]><testsuites tests='1'><testsuite tests='1'>{CASE}</testsuite></testsuites>"
        ),
        format!("junk<testsuites tests='1'><testsuite tests='1'>{CASE}</testsuite></testsuites>"),
    ] {
        assert!(check(&xml).is_err(), "accepted {xml}");
    }
}

#[test]
fn common_receipt_hashes_must_equal_each_expected_input_in_order() {
    let expected: Vec<String> = ['1', '2', '3', '4']
        .map(|c| c.to_string().repeat(40))
        .to_vec();
    let context = |header: &str, hashes: &[String]| {
        format!(
            "cargo-nextest 0.9\nrustc 1.95\ncargo 1.95\n{header}\n{}\n",
            hashes.join("\n")
        )
    };
    let accepted = context(INPUT_HEADER, &expected);
    let problems = |text: &str| {
        let mut problems = Vec::new();
        check_environment(text, Some(&expected), "all shards", &mut problems);
        problems
    };
    assert!(problems(&accepted).is_empty());
    for index in 0..4 {
        let mut wrong = expected.clone();
        wrong[index] = "f".repeat(40);
        let found = problems(&context(INPUT_HEADER, &wrong));
        assert!(found.iter().any(|why| why.contains(INPUT_PATHS[index])));
    }
    let mut reversed = expected.clone();
    reversed.reverse();
    let mut extra = expected.clone();
    extra.push("f".repeat(40));
    for wrong in [
        context(INPUT_HEADER, &reversed),
        context(INPUT_HEADER, &expected[..3]),
        context(INPUT_HEADER, &extra),
        context("input_blobs (wrong order):", &expected),
        format!("{accepted}{INPUT_HEADER}\n{}\n", expected.join("\n")),
    ] {
        assert!(!problems(&wrong).is_empty(), "accepted {wrong}");
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    crate::run::run_output_owned_in(
        "git",
        &args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>(),
        root,
    )
    .map(|text| text.trim().to_string())
}

#[test]
fn expected_inputs_are_committed_blobs_not_dirty_files_or_consensus() -> Result<(), String> {
    let root = crate::tests::temp_dir("shard-input-commit");
    git(&root, &["init", "--quiet"])?;
    for (index, path) in INPUT_PATHS.iter().enumerate() {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(file, format!("input {index}\n")).map_err(|err| err.to_string())?;
    }
    let commit = |message: &str| -> Result<String, String> {
        git(&root, &["add", "."])?;
        git(
            &root,
            &[
                "-c",
                "user.name=Shard Test",
                "-c",
                "user.email=shard@example.invalid",
                "commit",
                "--quiet",
                "--no-gpg-sign",
                "-m",
                message,
            ],
        )?;
        git(&root, &["rev-parse", "HEAD"])
    };
    let first = commit("first")?;
    let old = expected_input_blobs(&root, &first)?;
    for (path, actual) in INPUT_PATHS.iter().zip(&old) {
        assert_eq!(
            actual,
            &git(&root, &["rev-parse", &format!("{first}:{path}")])?
        );
    }
    fs::write(root.join(INPUT_PATHS[1]), "changed config\n").map_err(|err| err.to_string())?;
    assert_eq!(expected_input_blobs(&root, &first)?, old);
    let second = commit("new config")?;
    let new = expected_input_blobs(&root, &second)?;
    assert_ne!(new[1], old[1]);
    let environment = format!(
        "cargo-nextest 0.9\nrustc 1.95\ncargo 1.95\n{INPUT_HEADER}\n{}\n",
        old.join("\n")
    );
    let mut problems = Vec::new();
    check_environment(&environment, Some(&new), "all shards", &mut problems);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains(INPUT_PATHS[1]));
    problems.clear();
    check_environment(
        &environment.replace(&old[1], &new[1]),
        Some(&new),
        "all shards",
        &mut problems,
    );
    assert!(problems.is_empty());
    git(&root, &["replace", &first, &second])?;
    assert_eq!(
        expected_input_blobs(&root, &first)?,
        old,
        "replacement refs are not immutable inputs"
    );
    let tree = git(&root, &["rev-parse", &format!("{second}^{{tree}}")])?;
    let refuses = |invalid: &str| -> Result<(), String> {
        if let Ok(blobs) = expected_input_blobs(&root, invalid) {
            return Err(format!(
                "resolved invalid checkout `{invalid}` to {blobs:?}"
            ));
        }
        Ok(())
    };
    // Invalid revisions are challenged while HEAD has every required input:
    // falling back to HEAD must not look like a missing-input rejection.
    for invalid in ["abc123".to_string(), "f".repeat(40), tree] {
        refuses(&invalid)?;
    }
    fs::remove_file(root.join(INPUT_PATHS[3])).map_err(|err| err.to_string())?;
    let missing = commit("missing workflow")?;
    refuses(&missing)?;
    fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
    Ok(())
}

fn attempt_blobs() -> Vec<String> {
    ['1', '2', '3', '4']
        .map(|c| c.to_string().repeat(40))
        .to_vec()
}

fn attempt_options(root: &Path, attempt: u32) -> Options {
    Options {
        dir: root.join("in"),
        artifact: "integration".to_string(),
        attempt: attempt.to_string(),
        repository: "o/r".to_string(),
        sha: "a".repeat(40),
        run_id: "7".to_string(),
        shards: 2,
        out: root.join("out"),
    }
}

fn put_attempt(root: &Path, shard: u32, attempt: u32) -> Result<PathBuf, String> {
    let dir = root
        .join("in")
        .join(format!("integration-nextest-shard-{shard}-{attempt}"));
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let put =
        |name: &str, text: String| fs::write(dir.join(name), text).map_err(|err| err.to_string());
    put(&format!("shard-{shard}.status"), "0\n".to_string())?;
    put(&format!("list-{shard}.txt"), format!("b{shard} case\n"))?;
    put(
        "junit.xml",
        format!(
            "<testsuites tests='1'><!-- <testsuite decoy> --><testsuite tests='1'><testcase classname='b{shard}' name='case'/><system-out><![CDATA[<testsuite retained>]]></system-out></testsuite></testsuites>"
        ),
    )?;
    put(
        "run-context.txt",
        format!(
            "repository=o/r\ncheckout_sha={}\nrun_id=7\nrun_attempt={attempt}\nprofile=ci\nshard={shard}/2\ncommand=cargo nextest run --workspace --profile ci --partition count:{shard}/2\ncargo-nextest 0.9\nrustc 1.95\ncargo 1.95\n{INPUT_HEADER}\n{}\nnextest_exit=0\n",
            "a".repeat(40),
            attempt_blobs().join("\n")
        ),
    )?;
    if shard == 1 {
        put("list-all.txt", "b1 case\nb2 case\n".to_string())?;
    }
    Ok(dir)
}

#[test]
fn attempt_selection_preserves_xml_hashes_and_honest_disclosure() -> Result<(), String> {
    for (label, verifying, selected, disclosure) in [
        ("mixed", 2, [1, 2], Some("1,2")),
        ("all-reused", 2, [1, 1], Some("1")),
        ("fresh", 2, [2, 2], None),
        ("selected-full-list", 3, [2, 3], Some("2,3")),
    ] {
        let root = crate::tests::temp_dir(&format!("receipt-integration-{label}"));
        for shard in 1..=2 {
            let old = put_attempt(&root, shard, 1)?;
            if selected[(shard - 1) as usize] > 1 {
                fs::write(old.join(format!("shard-{shard}.status")), "1\n")
                    .map_err(|err| err.to_string())?;
                if shard == 1 {
                    fs::write(old.join("list-all.txt"), "poisoned old workspace\n")
                        .map_err(|err| err.to_string())?;
                }
            }
            put_attempt(&root, shard, selected[(shard - 1) as usize])?;
        }
        let outcome = verify(&attempt_options(&root, verifying), Ok(attempt_blobs()));
        assert!(
            outcome.problems.is_empty(),
            "{label}: {:?}",
            outcome.problems
        );
        assert!(
            outcome
                .context
                .lines()
                .any(|line| line == format!("run_attempt={verifying}"))
        );
        let got: Vec<&str> = outcome
            .context
            .lines()
            .filter_map(|line| line.strip_prefix("verified_attempts="))
            .collect();
        assert_eq!(got, disclosure.into_iter().collect::<Vec<_>>(), "{label}");
        for (index, attempt) in selected.iter().enumerate() {
            assert!(
                outcome.summary.contains(&format!(
                    "shard {}: listed 1, junit tests 1 (attempt {attempt})",
                    index + 1
                )),
                "{}",
                outcome.summary
            );
        }
        assert_eq!(parse_junit(&outcome.junit)?.identities.len(), 2);
        assert_eq!(
            outcome
                .junit
                .matches("<![CDATA[<testsuite retained>]]>")
                .count(),
            2
        );
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[test]
fn invalid_latest_attempt_never_falls_back_to_older_green_evidence() -> Result<(), String> {
    for label in [
        "red",
        "missing-status",
        "wrong-attempt",
        "wrong-checkout",
        "wrong-run",
        "duplicate",
        "wrong-blob",
        "malformed-xml",
        "tool-mismatch",
        "wrong-list",
    ] {
        let root = crate::tests::temp_dir(&format!("receipt-latest-{label}"));
        put_attempt(&root, 1, 1)?;
        put_attempt(&root, 2, 1)?;
        let latest = put_attempt(&root, 2, 2)?;
        let context = latest.join("run-context.txt");
        let text = fs::read_to_string(&context).map_err(|err| err.to_string())?;
        match label {
            "red" => fs::write(latest.join("shard-2.status"), "1\n"),
            "missing-status" => fs::remove_file(latest.join("shard-2.status")),
            "wrong-attempt" => fs::write(&context, text.replace("run_attempt=2", "run_attempt=1")),
            "wrong-checkout" => fs::write(
                &context,
                text.replace(
                    &format!("checkout_sha={}", "a".repeat(40)),
                    &format!("checkout_sha={}", "b".repeat(40)),
                ),
            ),
            "wrong-run" => fs::write(&context, text.replace("run_id=7", "run_id=8")),
            "duplicate" => fs::write(&context, format!("checkout_sha=wrong\n{text}")),
            "wrong-blob" => fs::write(&context, text.replace(&attempt_blobs()[0], &"f".repeat(40))),
            "malformed-xml" => fs::write(latest.join("junit.xml"), "<testsuites>broken"),
            "tool-mismatch" => fs::write(&context, text.replace("rustc 1.95", "rustc different")),
            "wrong-list" => fs::write(latest.join("list-2.txt"), "forged subject\n"),
            _ => return Err(format!("unknown fixture variant: {label}")),
        }
        .map_err(|err| err.to_string())?;
        let outcome = verify(&attempt_options(&root, 2), Ok(attempt_blobs()));
        assert!(
            !outcome.problems.is_empty(),
            "{label} fell back to old green evidence"
        );
        assert!(
            outcome.summary.contains("attempt 2"),
            "{label}: {}",
            outcome.summary
        );
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[test]
fn aggregate_attempt_disclosure_cannot_come_from_source_shards() -> Result<(), String> {
    for selected in [[1, 1], [1, 2], [2, 2]] {
        let root = crate::tests::temp_dir("receipt-reserved-attempts");
        for (index, attempt) in selected.iter().enumerate() {
            let dir = put_attempt(&root, index as u32 + 1, *attempt)?;
            let path = dir.join("run-context.txt");
            let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
            fs::write(
                path,
                text.replace(
                    INPUT_HEADER,
                    &format!("verified_attempts=99\n{INPUT_HEADER}"),
                ),
            )
            .map_err(|err| err.to_string())?;
        }
        let outcome = verify(&attempt_options(&root, 2), Ok(attempt_blobs()));
        for shard in 1..=2 {
            assert!(
                outcome.problems.iter().any(|why| why.contains(&format!(
                    "shard {shard}: run-context key `verified_attempts` is reserved"
                ))),
                "{:?}",
                outcome.problems
            );
        }
        fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[test]
fn future_zero_and_non_attempt_artifacts_are_not_selected() -> Result<(), String> {
    let root = crate::tests::temp_dir("receipt-ignored-attempts");
    put_attempt(&root, 1, 1)?;
    put_attempt(&root, 2, 1)?;
    for suffix in ["3", "0", "not-an-attempt"] {
        fs::create_dir_all(
            root.join("in")
                .join(format!("integration-nextest-shard-2-{suffix}")),
        )
        .map_err(|err| err.to_string())?;
    }
    let outcome = verify(&attempt_options(&root, 2), Ok(attempt_blobs()));
    assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
    assert!(
        outcome
            .context
            .lines()
            .any(|line| line == "verified_attempts=1")
    );
    fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}
