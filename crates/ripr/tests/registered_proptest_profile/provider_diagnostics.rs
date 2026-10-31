//! #10864: actual CLI diagnosis with no fabricated property authority.
//! Uses the existing child/process owner and original diff cache throughout.
use super::*;

const METADATA_COMMAND: &str =
    "cargo metadata --offline --locked --format-version 1 --manifest-path Cargo.toml";

fn action(finding: &Value) -> Result<&str, String> {
    finding["recommended_next_step"]
        .as_str()
        .ok_or_else(|| format!("missing next step: {finding}"))
}

fn require_withheld(finding: &Value) {
    assert_eq!(finding["classification"], "no_static_path", "{finding}");
    assert_eq!(finding["related_tests_total"], 0, "{finding}");
    for values in [
        &finding["related_tests"],
        &finding["observed_values"],
        &finding["activation"]["observed_values"],
    ] {
        assert_eq!(values.as_array().map(Vec::len), Some(0), "{finding}");
    }
    assert_eq!(finding["static_limit_kind"], "rust_macro_reach_unresolved");
    assert_eq!(finding["oracle_kind"], "unknown", "{finding}");
    assert_eq!(finding["oracle_strength"], "none", "{finding}");
    for stage in ["reach", "infect", "propagate"] {
        assert_eq!(finding["ripr"][stage]["state"], "unknown", "{finding}");
    }
}

#[test]
fn offline_metadata_diagnosis_is_scoped_and_recovers_without_purging_diff_cache()
-> Result<(), String> {
    let mut fixture = Fixture::new()?;
    let healthy = finding(&fixture, "provider-healthy", None)?;
    assert_eq!(healthy["classification"], "exposed", "{healthy}");
    // A task-owned empty Cargo home cannot satisfy the locked registry graph.
    // The ordinary suite's prepared home is never modified or removed.
    let empty_home = fixture.0.join("target/empty-provider-home");
    std::fs::create_dir_all(&empty_home).map_err(|error| error.to_string())?;
    fixture.1.cargo_home = Some(empty_home);
    let refused = finding(&fixture, "provider-unavailable", None)?;
    require_withheld(&refused);
    let next = action(&refused)?.to_string();
    assert!(next.contains(METADATA_COMMAND), "{refused}");
    let evidence = refused["evidence"].as_array().ok_or("missing evidence")?;
    assert!(
        evidence.iter().filter_map(Value::as_str).any(|line| {
            line.starts_with("limitation_first_unresolved_edge: offline provider verification;")
                && line.contains("exited unsuccessfully")
        }),
        "{refused}"
    );
    assert_eq!(
        finding(&fixture, "provider-unavailable-repeat", None)?,
        refused
    );

    // The full Next step field must preserve the JSON action, apart from
    // #7756's existing single-terminal-period display rule for human fields.
    fixture.1.human = true;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let rendered = run(
        &fixture,
        executable.as_os_str(),
        &[
            OsStr::new("--exact"),
            OsStr::new("property_profile_probe_child"),
            OsStr::new("--nocapture"),
        ],
        "provider-human",
        None,
        true,
        None,
    )?;
    assert!(rendered.status.success(), "{rendered:?}");
    let human = String::from_utf8(rendered.stdout).map_err(|error| error.to_string())?;
    assert_eq!(human.matches(REPORT).count(), 1, "{human}");
    assert_eq!(human.matches(REPORT_END).count(), 1, "{human}");
    let rendered_next = human
        .split_once("\nNext step\n")
        .and_then(|(_, rest)| rest.split_once("\nDrill in:"))
        .map(|(step, _)| step)
        .ok_or_else(|| format!("missing complete Next step section: {human}"))?;
    let expected_next = next.strip_suffix('.').unwrap_or(&next);
    assert_eq!(
        rendered_next
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        expected_next
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        "JSON action differs from the complete human Next step field: {human}"
    );
    fixture.1.human = false;

    // A lexically earlier unsupported sibling must not hide the supported
    // invocation's observed failure or borrow its invocation pointer.
    let opaque_sibling = PROPERTY.replace("100i32", "100");
    assert_ne!(opaque_sibling, PROPERTY);
    fixture.write("tests/a_opaque.rs", &opaque_sibling)?;
    let siblings = finding(&fixture, "provider-opaque-sibling-first", None)?;
    require_withheld(&siblings);
    assert!(action(&siblings)?.contains(METADATA_COMMAND), "{siblings}");
    let pointers = siblings["evidence"]
        .as_array()
        .ok_or("missing sibling evidence")?
        .iter()
        .filter_map(Value::as_str)
        .filter(|line| line.starts_with("limitation_last_established_edge:"))
        .collect::<Vec<_>>();
    assert_eq!(pointers.len(), 1, "{siblings}");
    assert!(pointers[0].contains("tests/boundary.rs:"), "{siblings}");
    assert!(!pointers[0].contains("tests/a_opaque.rs:"), "{siblings}");
    std::fs::remove_file(fixture.0.join("tests/boundary.rs")).map_err(|error| error.to_string())?;
    let only_opaque = finding(&fixture, "provider-only-opaque-sibling", None)?;
    require_withheld(&only_opaque);
    assert!(
        !action(&only_opaque)?.contains(METADATA_COMMAND),
        "{only_opaque}"
    );
    fixture.write("tests/boundary.rs", PROPERTY)?;
    std::fs::remove_file(fixture.0.join("tests/a_opaque.rs")).map_err(|error| error.to_string())?;

    // Same failing environment, unsupported syntax: no borrowed diagnosis.
    let unsupported = PROPERTY.replace("100i32", "100");
    assert_ne!(
        unsupported, PROPERTY,
        "the unsupported stimulus must differ"
    );
    fixture.write("tests/boundary.rs", &unsupported)?;
    let opaque = finding(&fixture, "provider-unsupported", None)?;
    require_withheld(&opaque);
    assert!(!action(&opaque)?.contains(METADATA_COMMAND), "{opaque}");
    fixture.write("tests/boundary.rs", PROPERTY)?;

    // Refused provider/configuration premises are not native metadata errors,
    // even while this child would fail any attempted offline registry lookup.
    let checksum = "4b45fcc2344c680f5025fe57779faef368840d0bd1f42f216291f0dc4ace4744";
    let wrong_lock = LOCK.replace(checksum, &"0".repeat(checksum.len()));
    assert_ne!(
        wrong_lock, LOCK,
        "checksum stimulus must change the locked provider"
    );
    fixture.write("Cargo.lock", &wrong_lock)?;
    let wrong_provider = finding(&fixture, "provider-wrong-checksum", None)?;
    require_withheld(&wrong_provider);
    assert!(
        !action(&wrong_provider)?.contains(METADATA_COMMAND),
        "{wrong_provider}"
    );
    fixture.write("Cargo.lock", LOCK)?;

    std::fs::create_dir(fixture.0.join(".cargo")).map_err(|error| error.to_string())?;
    fixture.write(".cargo/config.toml", "[env]\nPROPTEST_CASES='0'\n")?;
    let unsupported_config = finding(&fixture, "provider-refused-config", None)?;
    require_withheld(&unsupported_config);
    assert!(
        !action(&unsupported_config)?.contains(METADATA_COMMAND),
        "{unsupported_config}"
    );
    std::fs::remove_file(fixture.0.join(".cargo/config.toml"))
        .map_err(|error| error.to_string())?;
    std::fs::remove_dir(fixture.0.join(".cargo")).map_err(|error| error.to_string())?;

    // An independent ordinary assertion stays useful despite missing proptest.
    fixture.write("tests/ordinary.rs", ORDINARY)?;
    let ordinary = finding(&fixture, "provider-ordinary", None)?;
    assert_eq!(ordinary["classification"], "exposed", "{ordinary}");
    std::fs::remove_file(fixture.0.join("tests/ordinary.rs")).map_err(|error| error.to_string())?;

    // Restore the original environment, NOT a new analysis cache or fixture.
    // This is environment restoration, not same-Cargo-home archive restoration;
    // the separately required installed missing-archive regression remains open.
    fixture.1.cargo_home = None;
    assert_eq!(finding(&fixture, "provider-restored", None)?, healthy);
    assert_eq!(
        finding(&fixture, "provider-restored-repeat", None)?,
        healthy
    );
    for (path, expected) in [
        ("src/lib.rs", SOURCE),
        ("tests/boundary.rs", PROPERTY),
        ("Cargo.toml", MANIFEST),
        ("Cargo.lock", LOCK),
        ("diff.patch", DIFF),
    ] {
        assert_eq!(
            std::fs::read_to_string(fixture.0.join(path)).map_err(|error| error.to_string())?,
            expected,
            "fixture input changed: {path}"
        );
    }
    Ok(())
}
