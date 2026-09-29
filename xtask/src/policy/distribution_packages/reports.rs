use super::{DistributionOutcome, ARCHIVE_WORKFLOW_PATH, DISTRIBUTION_MANIFEST_PATH};

pub(super) fn distribution_json(outcome: &DistributionOutcome) -> Result<String, String> {
    let targets = outcome
        .manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .targets
                .iter()
                .map(|target| {
                    serde_json::json!({
                        "rust_target": target.rust_target,
                        "executable": target.executable,
                        "native_os": target.native_os,
                        "native_cpu": target.native_cpu,
                        "native_libc": target.native_libc,
                        "minimum_system": target.minimum_system,
                        "wheel_family": target.wheel_family,
                        "npm_os": target.npm_os,
                        "npm_cpu": target.npm_cpu,
                        "npm_libc": target.npm_libc,
                        "npm_package": target.npm_package,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let manifest = outcome.manifest.as_ref();
    let value = serde_json::json!({
        "check": "check-release-targets/distribution-packages",
        "status": if outcome.violations.is_empty() { "pass" } else { "fail" },
        "manifest": DISTRIBUTION_MANIFEST_PATH,
        "network_used": false,
        "identity": {
            "product": manifest.map(|manifest| manifest.product.as_str()),
            "cargo_package": manifest.map(|manifest| manifest.cargo_package.as_str()),
            "executable": manifest.map(|manifest| manifest.executable.as_str()),
            "pypi_distribution": manifest.map(|manifest| manifest.pypi_distribution.as_str()),
            "npm_launcher": manifest.map(|manifest| manifest.npm_launcher.as_str()),
        },
        "versions": {
            "cargo": outcome.cargo_version,
            "pypi": outcome.pypi_version,
            "npm": outcome.npm_version,
        },
        "release_features": manifest.map(|manifest| &manifest.release_features),
        "targets": targets,
        "archive_workflow_targets": outcome.workflow_targets.iter().map(|target| {
            serde_json::json!({
                "rust_target": target.rust_target,
                "executable": target.executable,
            })
        }).collect::<Vec<_>>(),
        "archive_workflow": ARCHIVE_WORKFLOW_PATH,
        "violations": outcome.violations,
        "non_claim": manifest.map(|manifest| manifest.non_claim.as_str()).unwrap_or(
            "Distribution input was unavailable; no package, version, target, compatibility, or publication claim is established."
        ),
    });
    let mut rendered = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("serialize distribution package report: {error}"))?;
    rendered.push('\n');
    Ok(rendered)
}

pub(super) fn distribution_markdown(outcome: &DistributionOutcome) -> String {
    let mut output = String::new();
    output.push_str("# Distribution package contract\n\n");
    output.push_str(&format!(
        "Status: **{}**\n\n",
        if outcome.violations.is_empty() {
            "pass"
        } else {
            "fail"
        }
    ));
    output.push_str(&format!(
        "Manifest: `{DISTRIBUTION_MANIFEST_PATH}`  \nCargo version: `{}`  \nPyPI version: `{}`  \nnpm version: `{}`\n\n",
        outcome.cargo_version.as_deref().unwrap_or("unavailable"),
        outcome.pypi_version.as_deref().unwrap_or("unavailable"),
        outcome.npm_version.as_deref().unwrap_or("unavailable"),
    ));

    if let Some(manifest) = &outcome.manifest {
        output.push_str("## Identity\n\n");
        output.push_str(&format!(
            "- Product / Cargo package / executable: `{}` / `{}` / `{}`\n- PyPI distribution: `{}`\n- npm launcher: `{}`\n- Release features: `{}`\n\n",
            manifest.product,
            manifest.cargo_package,
            manifest.executable,
            manifest.pypi_distribution,
            manifest.npm_launcher,
            manifest.release_features.join(", "),
        ));
        output.push_str("## Targets\n\n");
        output.push_str("| Rust target | Executable | Native platform | Minimum system | Wheel family | npm payload |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for target in &manifest.targets {
            output.push_str(&format!(
                "| `{}` | `{}` | `{} / {} / {}` | `{}` | `{}` | `{}` |\n",
                target.rust_target,
                target.executable,
                target.native_os,
                target.native_cpu,
                target.native_libc,
                target.minimum_system,
                target.wheel_family,
                target.npm_package,
            ));
        }
        output.push('\n');
        output.push_str(&format!("Non-claim: {}\n\n", manifest.non_claim));
    }

    if outcome.violations.is_empty() {
        output.push_str("No distribution identity, version, crate, target, or workflow drift found.\n");
    } else {
        output.push_str("## Violations\n\n");
        for violation in &outcome.violations {
            output.push_str(&format!("- {violation}\n"));
        }
    }
    output
}
