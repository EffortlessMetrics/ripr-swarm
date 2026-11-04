use super::identity::{
    collect_identity_inputs_for_commit, git_rev_parse, git_source_tree, git_toplevel,
    tree_clean_for_subject,
};
use super::model::{
    IdentityInputs, REHEARSAL_PLAN_SCHEMA_VERSION, REHEARSAL_RECEIPT_KIND, RehearsalComponent,
    RehearsalIdentity, RehearsalPlan, RehearsalReceipt,
};

const USAGE: &str = "usage: cargo xtask readiness-rehearsal --sha <rev> --plan [--json]";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let (sha, json) = parse_args(args)?;
    let root = git_toplevel()?;
    let source_sha = git_rev_parse(&sha)?;
    if !tree_clean_for_subject(&source_sha)? {
        return Err(
            "rehearsal planning requires the clean current HEAD, including untracked files"
                .to_string(),
        );
    }
    let source_tree = git_source_tree(&source_sha)?;
    let inputs = collect_identity_inputs_for_commit(&root, &source_sha)?;
    let receipt = plan_receipt(
        source_sha.clone(),
        source_tree,
        inputs,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )?;
    // Recheck after reads; this is an observation, not a lock against concurrent writers.
    if !tree_clean_for_subject(&source_sha)? {
        return Err("rehearsal source changed while planning".to_string());
    }
    let output = if json {
        serde_json::to_string_pretty(&receipt).map_err(|e| format!("encode rehearsal plan: {e}"))?
    } else {
        format!(
            "No-publish rehearsal plan\nSource: {}\nTree: {}\nHost: {}/{}\nProfile: plan_only\nAll components: not_run\nWrites/cache roots: none\nTools: git (read-only)\nNo build, install, network, tag, version, signing, credential, candidate-freeze, deployment or release operation.\nNo package/install or release readiness established.\nUse --json for the complete receipt and read inventory.",
            receipt.source_sha,
            receipt
                .plan
                .as_ref()
                .map(|p| p.source_tree.as_str())
                .unwrap_or(""),
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
    };
    println!("{output}");
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(String, bool), String> {
    let mut sha = None;
    let mut plan = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--sha" if sha.is_none() => {
                let value = args
                    .get(index + 1)
                    .filter(|v| !v.is_empty() && !v.starts_with('-'))
                    .ok_or_else(|| format!("missing --sha value; {USAGE}"))?;
                sha = Some(value.clone());
                index += 2;
            }
            "--plan" if !plan => {
                plan = true;
                index += 1;
            }
            "--json" if !json => {
                json = true;
                index += 1;
            }
            other => {
                return Err(format!(
                    "unsupported or repeated argument `{other}`; {USAGE}"
                ));
            }
        }
    }
    if !plan {
        return Err(format!(
            "execution is unavailable; --plan is required; {USAGE}"
        ));
    }
    Ok((
        sha.ok_or_else(|| format!("explicit --sha is required; {USAGE}"))?,
        json,
    ))
}

pub(super) fn plan_receipt(
    source_sha: String,
    source_tree: String,
    inputs: IdentityInputs,
    host_os: &str,
    host_arch: &str,
) -> Result<RehearsalReceipt, String> {
    let component = || {
        Some(RehearsalComponent {
            status: "not_run".to_string(),
            sha256: None,
            limitation: Some("plan only; owning component producer has not executed".to_string()),
        })
    };
    let receipt = RehearsalReceipt {
        schema_version: REHEARSAL_PLAN_SCHEMA_VERSION.to_string(),
        kind: REHEARSAL_RECEIPT_KIND.to_string(),
        source_sha,
        tree_clean: true,
        identity: RehearsalIdentity {
            cargo_lock_digest: inputs.cargo_lock_digest.ok_or("missing Cargo.lock identity")?,
            package_lock_digest: Some(inputs.package_lock_digest.ok_or("missing package-lock identity")?),
            rust_toolchain_digest: inputs.rust_toolchain_digest.ok_or("missing rust-toolchain identity")?,
            crate_version: inputs.crate_version.ok_or("missing crate version")?,
            extension_version: Some(inputs.extension_version.ok_or("missing extension version")?),
        },
        plan: Some(RehearsalPlan {
            source_tree, host_os: host_os.to_string(), host_arch: host_arch.to_string(),
            profile: "plan_only".to_string(),
            reads: ["git HEAD/tree/status (including untracked files)", "git configuration key names (clean/process filter refusal)", "git repository-wide index flags and modes", "committed tree and raw tracked file hashes without filters", "tracked file and directory metadata without following symlinks", "Cargo.toml", "crates/ripr/Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "editors/vscode/package.json", "editors/vscode/package-lock.json"].map(str::to_string).to_vec(),
            writes: Vec::new(), cache_roots: Vec::new(), tools: vec!["git (read-only)".to_string()],
            prohibited_operations: ["build", "install", "network", "registry/marketplace publication", "tag", "release", "deployment", "version change", "signing", "credentials", "candidate freeze"].map(str::to_string).to_vec(),
            tags_created: Vec::new(), versions_changed: false, candidate_frozen: false,
            release_cut: false, release_authority: false, signing_performed: false,
            credentials_accessed: false, deployments_performed: false,
        }),
        crate_package: component(), installed_binary: component(),
        // Matrix/platform selection belongs to the existing component owners and #4850.
        server_artifacts: Vec::new(), vsix: component(), install_smokes: Vec::new(),
        dependency_policy: component(), sbom: component(), provenance_plan: component(),
        limitations: vec!["No component executed. Server/platform and install-smoke rows are not populated; an empty list establishes no coverage. #4850 owns integrated receipt fan-in.".to_string()],
        non_claims: vec!["not package/install readiness, qualification, candidate freeze or release authority".to_string()],
        published_channels: Vec::new(),
    };
    let text = serde_json::to_string(&receipt).map_err(|e| format!("encode plan receipt: {e}"))?;
    super::rehearsal::parse_rehearsal_receipt(&text)
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn rehearsal_plan_arguments_fail_closed() {
        for args in [
            vec![],
            vec!["--plan"],
            vec!["--sha", "HEAD"],
            vec!["--sha", "--plan"],
            vec!["--sha", "HEAD", "--plan", "--plan"],
            vec!["--sha", "HEAD", "--plan", "--sha", "HEAD"],
            vec!["--sha", "HEAD", "--plan", "--no-publish"],
            vec!["--sha", "HEAD", "--plan", "--execute"],
        ] {
            assert!(parse_args(&args.into_iter().map(str::to_string).collect::<Vec<_>>()).is_err());
        }
        assert_eq!(
            parse_args(&["--sha", "HEAD", "--plan", "--json"].map(str::to_string)),
            Ok(("HEAD".to_string(), true))
        );
    }
}
