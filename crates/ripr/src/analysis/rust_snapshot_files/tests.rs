use super::*;

fn names(test: &str, attrs: &[&str], body: &str, oracle: &str, file: &str) -> Option<Vec<String>> {
    let attrs: Vec<String> = attrs.iter().map(|attr| attr.to_string()).collect();
    expected_snapshot_names(test, &attrs, body, oracle, file)
}

const FILE: &str = "use insta::assert_json_snapshot;\n";

#[test]
fn unnamed_assertion_names_the_function_and_its_test_stripped_form() {
    let body = "fn test_record() { insta::assert_json_snapshot!(record.items); }";
    assert_eq!(
        names(
            "test_record",
            &["#[test]"],
            body,
            "insta::assert_json_snapshot!(record.items);",
            FILE
        ),
        Some(vec!["test_record".to_string(), "record".to_string()])
    );
}

#[test]
fn string_named_assertion_names_the_literal() {
    let body = "fn t() { insta::assert_snapshot!(\"report\", render()); }";
    assert_eq!(
        names(
            "t",
            &["#[test]"],
            body,
            "insta::assert_snapshot!(\"report\", render());",
            FILE
        ),
        Some(vec!["report".to_string()])
    );
}

#[test]
fn an_unqualified_macro_needs_insta_in_the_file() {
    let body = "fn t() { assert_snapshot!(render()); }";
    assert!(names("t", &["#[test]"], body, "assert_snapshot!(render());", FILE).is_some());
    assert!(names("t", &["#[test]"], body, "assert_snapshot!(render());", "").is_none());
    let other = "fn t() { other::assert_snapshot!(render()); }";
    assert!(
        names(
            "t",
            &["#[test]"],
            other,
            "other::assert_snapshot!(render());",
            FILE
        )
        .is_none()
    );
}

#[test]
fn inline_repeated_redirected_and_parameterized_shapes_are_unresolved() {
    let one = "fn t() { insta::assert_snapshot!(v); }";
    let call = "insta::assert_snapshot!(v);";
    // Inline snapshots carry their expected value.
    for inline in [
        "insta::assert_snapshot!(v, @\"2\");",
        "insta::assert_snapshot!(v, @r#\"2\"#);",
    ] {
        assert_eq!(
            names("t", &["#[test]"], inline, inline, FILE),
            None,
            "{inline}"
        );
    }
    // A second assertion advances insta's counter.
    let two = "fn t() { insta::assert_snapshot!(a); insta::assert_snapshot!(b); }";
    assert_eq!(names("t", &["#[test]"], two, call, FILE), None);
    // The assertion is not in this test's body (a helper's).
    assert_eq!(
        names("t", &["#[test]"], "fn t() { helper(); }", call, FILE),
        None
    );
    // Snapshot redirection or globbing in the file.
    for redirect in [
        "settings.set_snapshot_path(\"x\");",
        "insta::glob!(\"in/*\", |p| {});",
        "std::env::var(\"INSTA_UPDATE\")",
    ] {
        assert_eq!(
            names("t", &["#[test]"], one, call, &format!("{FILE}{redirect}")),
            None
        );
    }
    // Case-driven tests write one snapshot per case.
    assert_eq!(
        names("t", &["#[rstest]", "#[case(1)]"], one, call, FILE),
        None
    );
    // Three arguments, or a non-literal name, are not a shape resolved here.
    for odd in [
        "insta::assert_snapshot!(\"n\", v, \"dbg\");",
        "insta::assert_snapshot!(name, v);",
        "insta::assert_snapshot!(\"a/b\", v);",
        "insta::assert_snapshot!();",
    ] {
        let body = format!("fn t() {{ {odd} }}");
        assert_eq!(names("t", &["#[test]"], &body, odd, FILE), None, "{odd}");
    }
}

#[test]
fn arguments_split_only_at_top_level_outside_strings() {
    assert_eq!(
        split_top_level("\"a,b\", f(x, y), [1, 2],"),
        Some(vec![
            "\"a,b\"".to_string(),
            "f(x, y)".to_string(),
            "[1, 2]".to_string()
        ])
    );
    assert_eq!(split_top_level("a, ,b"), None);
    assert_eq!(split_top_level("a)"), None);
    let call = snapshot_call("insta::assert_snapshot!(f(')'), \"x\");", FILE);
    assert_eq!(call.map(|call| call.arguments.len()), Some(2));
}

#[test]
fn stem_matching_is_lenient_about_prefix_counter_and_suffix() {
    for stem in [
        "record",
        "krate__tests__record",
        "krate__tests__record-2",
        "krate__tests__record@linux",
        "krate__tests__record-3@linux",
    ] {
        assert!(stem_names_snapshot(stem, "record"), "{stem}");
    }
    for stem in [
        "krate__tests__other",
        "krate__tests__my_record",
        "records",
        "krate__recordx",
    ] {
        assert!(!stem_names_snapshot(stem, "record"), "{stem}");
    }
}

fn scratch(label: &str) -> Result<std::path::PathBuf, String> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-snap-listing-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

#[test]
fn listing_sees_snapshots_outside_skipped_directories_and_moves_its_identity() -> Result<(), String>
{
    let root = scratch("identity")?;
    std::fs::create_dir_all(root.join("src/snapshots")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("target/debug")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("target/debug/ignored.snap"), "x").map_err(|e| e.to_string())?;
    let empty = SnapshotListing::read(&root);
    assert!(empty.files.is_empty());
    assert!(!empty.holds_any(&["record".to_string()]));

    std::fs::write(root.join("src/snapshots/krate__tests__record.snap"), "x")
        .map_err(|e| e.to_string())?;
    let present = SnapshotListing::read(&root);
    assert!(present.holds_any(&["record".to_string()]));
    assert!(!present.holds_any(&["other".to_string()]));
    assert_ne!(empty.identity(), present.identity());

    std::fs::remove_file(root.join("src/snapshots/krate__tests__record.snap"))
        .map_err(|e| e.to_string())?;
    assert_eq!(empty.identity(), SnapshotListing::read(&root).identity());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn an_untracked_snapshot_is_not_one_ci_sees() -> Result<(), String> {
    let listing = SnapshotListing {
        files: vec![ListedSnapshot {
            path: "src/snapshots/krate__tests__record.snap".to_string(),
            tracked: Some(false),
        }],
        unknown: false,
    };
    assert!(!listing.holds_any(&["record".to_string()]));
    let unlisted = SnapshotListing {
        files: vec![ListedSnapshot {
            path: "src/snapshots/krate__tests__record.snap".to_string(),
            tracked: None,
        }],
        unknown: false,
    };
    assert!(unlisted.holds_any(&["record".to_string()]));
    assert_ne!(listing.identity(), unlisted.identity());
    Ok(())
}
