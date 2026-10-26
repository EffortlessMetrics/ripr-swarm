use super::*;

fn check(xml: &str) -> Result<(u64, String), Vec<String>> {
    check_junit(xml, &BTreeSet::from(["bin one".to_string()]), "control")
}

const CASE: &str = "<testcase classname=\"bin\" name=\"one\"/>";

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
