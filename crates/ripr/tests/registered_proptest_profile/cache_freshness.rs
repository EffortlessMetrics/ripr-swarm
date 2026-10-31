//! Offline provider failures must not reuse stale repository classifications.
//! The compact summary exposes aggregate grip counts, not diagnostic prose.
use super::*;

fn compact(fixture: &Fixture, label: &str, cache: &Path) -> Result<(Value, String), String> {
    let root = fixture.0.to_str().ok_or("fixture root is not UTF-8")?;
    let output = run(
        fixture,
        OsStr::new(env!("CARGO_BIN_EXE_ripr")),
        &[
            "check",
            "--root",
            root,
            "--mode",
            "fast",
            "--format",
            "repo-exposure-summary-json",
            "--quiet",
        ]
        .map(OsStr::new),
        label,
        None,
        true,
        Some(cache),
    )?;
    if !output.status.success() {
        return Err(format!(
            "{label}: compact CLI failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    assert_eq!(value["format"], "repo-exposure-summary-json", "{value}");
    Ok((value, String::from_utf8_lossy(&output.stderr).into_owned()))
}

#[test]
fn full_and_compact_warm_caches_follow_provider_failure_and_recovery() -> Result<(), String> {
    let mut fixture = Fixture::new()?;
    let full_cache = fixture.0.join("target/full-provider-cache");
    let compact_cache = fixture.0.join("target/compact-provider-cache");
    let (healthy, _) = report(&fixture, "full-healthy", None, Some(&full_cache))?;
    let healthy_boundary = boundary(&healthy)?;
    assert_eq!(
        healthy_boundary["grip_class"], "strongly_gripped",
        "{healthy}"
    );
    let (warm, trace) = report(&fixture, "full-warm", None, Some(&full_cache))?;
    assert_eq!(boundary(&warm)?, healthy_boundary);
    assert!(
        trace.contains("cache_hit"),
        "full cache was not warm: {trace}"
    );

    let (compact_healthy, _) = compact(&fixture, "compact-healthy", &compact_cache)?;
    let (compact_warm, trace) = compact(&fixture, "compact-warm", &compact_cache)?;
    assert_eq!(compact_warm["metrics"], compact_healthy["metrics"]);
    assert!(
        trace.contains("compact_cache_hit"),
        "compact cache was not warm: {trace}"
    );
    assert!(
        compact_healthy["metrics"]["grip_class"]["strongly_gripped"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "{compact_healthy}"
    );

    // This is an environment transition, not the separate same-home archive
    // withholding proof. Preserve both original repository caches throughout.
    let empty_home = fixture.0.join("target/empty-repo-provider-home");
    std::fs::create_dir_all(&empty_home).map_err(|error| error.to_string())?;
    fixture.1.cargo_home = Some(empty_home);
    let unavailable = finding(&fixture, "repo-provider-unavailable-diff", None)?;
    assert_eq!(
        unavailable["classification"], "no_static_path",
        "{unavailable}"
    );
    assert!(
        unavailable["recommended_next_step"]
            .as_str()
            .is_some_and(|text| text.contains(
                "cargo metadata --offline --locked --format-version 1 --manifest-path Cargo.toml"
            )),
        "{unavailable}"
    );

    let (failed, _) = report(
        &fixture,
        "full-provider-failed-warm",
        None,
        Some(&full_cache),
    )?;
    let (failed_cold, _) = report(
        &fixture,
        "full-provider-failed-cold",
        None,
        Some(&fixture.0.join("target/full-independent")),
    )?;
    let failed_boundary = boundary(&failed)?;
    assert_eq!(failed_boundary, boundary(&failed_cold)?);
    let rendered = failed_boundary.to_string();
    assert!(
        rendered.contains("offline provider verification:"),
        "{rendered}"
    );
    assert!(
        rendered.contains(
            "cargo metadata --offline --locked --format-version 1 --manifest-path Cargo.toml"
        ),
        "{rendered}"
    );
    assert!(!rendered.contains("zerocopy"), "{rendered}");
    assert_eq!(
        failed_boundary["evidence_record"]["actionability"]["class"], "static_limitation",
        "{failed_boundary}"
    );
    assert_ne!(
        failed_boundary["grip_class"], "strongly_gripped",
        "{failed}"
    );
    assert_eq!(failed_boundary["related_tests_total"], 0, "{failed}");

    let (compact_failed, _) = compact(&fixture, "compact-provider-failed-warm", &compact_cache)?;
    let (compact_failed_cold, _) = compact(
        &fixture,
        "compact-provider-failed-cold",
        &fixture.0.join("target/compact-independent"),
    )?;
    assert_eq!(compact_failed["metrics"], compact_failed_cold["metrics"]);
    assert_ne!(
        compact_failed["metrics"]["grip_class"], compact_healthy["metrics"]["grip_class"],
        "{compact_failed}"
    );

    fixture.1.cargo_home = None;
    let (restored, _) = report(&fixture, "full-provider-restored", None, Some(&full_cache))?;
    assert_eq!(boundary(&restored)?, healthy_boundary);
    assert!(
        !boundary(&restored)?
            .to_string()
            .contains("offline provider verification:")
    );
    let (compact_restored, _) = compact(&fixture, "compact-provider-restored", &compact_cache)?;
    assert_eq!(compact_restored["metrics"], compact_healthy["metrics"]);
    Ok(())
}
