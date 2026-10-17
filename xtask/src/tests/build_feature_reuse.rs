//! Cargo, rather than manifest spelling, owns dependency feature resolution.

use std::collections::{BTreeMap, BTreeSet};

type FeatureVariants = BTreeMap<String, BTreeSet<String>>;

fn feature_variants(rows: &str) -> Result<FeatureVariants, String> {
    let mut variants = FeatureVariants::new();
    for row in rows.lines() {
        let row = row.strip_suffix(" (*)").unwrap_or(row);
        // A repository path in Cargo's package display can itself contain '|'.
        let (package, features) = row
            .rsplit_once('|')
            .ok_or_else(|| format!("Cargo feature row has no separator: {row}"))?;
        if package.is_empty() {
            return Err("Cargo feature row has no package identity".to_owned());
        }
        variants
            .entry(package.to_owned())
            .or_default()
            .insert(features.to_owned());
    }
    Ok(variants)
}

#[cfg(target_os = "linux")]
fn cargo_features(root: &std::path::Path, package: &str) -> Result<FeatureVariants, String> {
    use std::time::Duration;
    let args = [
        "--color",
        "never",
        "tree",
        "--locked",
        "--offline",
        "--package",
        package,
        "--edges",
        "normal,build",
        "--prefix",
        "none",
        "--format",
        "{p}|{f}",
    ]
    .map(str::to_owned);
    let measured = crate::run::capture_output_measured(
        env!("CARGO"),
        &args,
        Some(root),
        // CI forces color; the explicit machine-output option must override it.
        &[("CARGO_TERM_COLOR", "always")],
        Duration::from_secs(30),
        "offline dependency feature inventory",
    )?;
    let output = measured.output;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "Cargo feature inventory for {package} failed: timed_out={}, status={:?}, stderr={}",
            output.timed_out, output.status, output.stderr
        ));
    }
    let variants = feature_variants(&output.stdout)?;
    if !variants.keys().any(|name| name.starts_with("ripr v")) || variants.len() < 2 {
        return Err(format!(
            "Cargo feature inventory for {package} did not contain the product dependency graph"
        ));
    }
    Ok(variants)
}

#[test]
#[cfg(target_os = "linux")]
fn linux_cli_and_xtask_share_product_dependency_features() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask manifest has no workspace parent")?;
    let product = cargo_features(root, "ripr")?;
    let tool = cargo_features(root, "xtask")?;
    let differences: Vec<_> = product
        .iter()
        .filter(|(package, features)| tool.get(*package) != Some(*features))
        .map(|(package, features)| (package, features, tool.get(package)))
        .collect();
    assert!(
        differences.is_empty(),
        "ordinary CLI -> xtask changes product dependency feature variants and can rebuild ripr: {differences:#?}"
    );
    Ok(())
}

#[test]
fn inventory_preserves_versions_and_multiple_feature_variants() -> Result<(), String> {
    let variants = feature_variants(
        "flags v1|\nflags v1|std\nflags v1|std (*)\nflags v2|serde,std\nripr v1 (path|with-pipe)|default\n",
    )?;
    assert_eq!(variants.len(), 3);
    assert_eq!(
        variants.get("flags v1"),
        Some(&BTreeSet::from([String::new(), "std".to_owned()]))
    );
    assert_eq!(
        variants.get("flags v2"),
        Some(&BTreeSet::from(["serde,std".to_owned()]))
    );
    assert!(variants.contains_key("ripr v1 (path|with-pipe)"));
    assert_eq!(
        feature_variants("malformed").unwrap_err(),
        "Cargo feature row has no separator: malformed"
    );
    assert_eq!(
        feature_variants("|std").unwrap_err(),
        "Cargo feature row has no package identity"
    );
    Ok(())
}
