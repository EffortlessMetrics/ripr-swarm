use super::*;

struct SourceRoot(PathBuf);
impl Drop for SourceRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        Path::new("git"),
        &args,
        root,
        &[],
        &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
        std::time::Duration::from_secs(30),
        "direct-manifest source control",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "source fixture Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

#[test]
fn direct_source_admission_refuses_moved_ref_wrong_tree_origin_and_same_version_substitution()
-> Result<(), String> {
    use crate::reports::release::candidate_harness::{AdmittedSource, QualificationInput};
    let mut fixture = Fixture::new()?;
    let source_path = fixture.root.with_extension("source");
    std::fs::create_dir(&source_path).map_err(|error| error.to_string())?;
    let source = SourceRoot(source_path);
    std::fs::create_dir_all(source.0.join("crates/ripr")).map_err(|error| error.to_string())?;
    for (path, contents, field) in [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/ripr\"]\n[workspace.package]\nversion = \"0.11.0\"\n",
            "workspace_manifest_sha256",
        ),
        (
            "crates/ripr/Cargo.toml",
            "[package]\nname = \"ripr\"\nversion.workspace = true\n",
            "package_manifest_sha256",
        ),
        ("Cargo.lock", "version = 4\n", "lock_sha256"),
    ] {
        std::fs::write(source.0.join(path), contents).map_err(|error| error.to_string())?;
        fixture.document["candidate"]["package"][field] = json!(digest(contents.as_bytes()));
    }
    git(&source.0, &["init", "--quiet", "--template="])?;
    for (key, value) in [
        ("user.name", "RIPR fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(&source.0, &["config", "--local", key, value])?;
    }
    git(
        &source.0,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
        ],
    )?;
    git(&source.0, &["-c", "core.hooksPath=", "add", "."])?;
    git(
        &source.0,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "--quiet",
            "-m",
            "fixture source",
        ],
    )?;
    // Retain only the real boundary commit object; mark it shallow so this
    // tiny Git fixture needs no historical trees or network access.
    let boundary_path = source.0.join(".git/last-integrated.commit");
    std::fs::write(
        &boundary_path,
        include_bytes!("fixtures/last-integrated.commit"),
    )
    .map_err(|error| error.to_string())?;
    let boundary = git(
        &source.0,
        &[
            "hash-object",
            "-w",
            "-t",
            "commit",
            // Relative to git's cwd: the canonical fixture root is verbatim
            // (`\\?\`) on Windows, which Git for Windows cannot open (#9660).
            ".git/last-integrated.commit",
        ],
    )?;
    if boundary.trim() != LAST_INTEGRATED {
        return Err("retained boundary object changed".to_string());
    }
    std::fs::write(
        source.0.join(".git/shallow"),
        format!("{LAST_INTEGRATED}\n"),
    )
    .map_err(|error| error.to_string())?;
    let fixture_tree = git(&source.0, &["rev-parse", "HEAD^{tree}"])?;
    let child = git(
        &source.0,
        &[
            "commit-tree",
            fixture_tree.trim(),
            "-p",
            LAST_INTEGRATED,
            "-m",
            "fixture source",
        ],
    )?;
    let side = git(
        &source.0,
        &[
            "commit-tree",
            fixture_tree.trim(),
            "-p",
            child.trim(),
            "-m",
            "side",
        ],
    )?;
    let main = git(
        &source.0,
        &[
            "commit-tree",
            fixture_tree.trim(),
            "-p",
            child.trim(),
            "-m",
            "main",
        ],
    )?;
    let merged = git(
        &source.0,
        &[
            "commit-tree",
            fixture_tree.trim(),
            "-p",
            main.trim(),
            "-p",
            side.trim(),
            "-m",
            "merged",
        ],
    )?;
    git(&source.0, &["reset", "--quiet", "--hard", merged.trim()])?;
    let sha = git(&source.0, &["rev-parse", "HEAD"])?.trim().to_string();
    let tree = git(&source.0, &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_string();
    let reference = format!("refs/tags/ripr-release-0.11.0-{sha}");
    git(&source.0, &["update-ref", &reference, &sha])?;
    fixture.document["candidate"]["sha"] = json!(sha);
    fixture.document["candidate"]["tree"] = json!(tree);
    fixture.document["candidate"]["ref"] = json!(reference);
    for name in ["selected_claims", "denominator", "audit"] {
        fixture.document["prerequisites"][name]["acceptance"]["candidate_sha"] = json!(sha);
        fixture.document["prerequisites"][name]["acceptance"]["candidate_tree"] = json!(tree);
    }
    let readback = format!("{sha}\n");
    std::fs::write(fixture.root.join("pin-remote.sha"), &readback)
        .map_err(|error| error.to_string())?;
    fixture.document["pin"]["remote_ref_readback"]["sha256"] = json!(digest(readback.as_bytes()));
    let range = format!("{LAST_INTEGRATED}..{sha}");
    let all = git(
        &source.0,
        &["rev-list", "--topo-order", "--reverse", &range],
    )?;
    let first = git(
        &source.0,
        &["rev-list", "--first-parent", "--reverse", &range],
    )?;
    if all.lines().count() != 4 || first != format!("{}\n{}\n{sha}\n", child.trim(), main.trim()) {
        return Err(
            "merge fixture did not distinguish all-reachable and first-parent history".to_string(),
        );
    }
    fixture.document["range"]["all_reachable_count"] = json!(4);
    fixture.document["range"]["first_parent_count"] = json!(3);
    fixture.document["range"]["all_reachable_sha256"] = json!(digest(all.as_bytes()));
    fixture.document["range"]["first_parent_sha256"] = json!(digest(first.as_bytes()));
    // #4006 item 6: the manifest carries the #2768 ledger's range_touched
    // inventory for the same base..candidate range, and the retained packet
    // bytes are rewritten to a real-shaped minimal ledger carrying the same
    // typed identity, so the custody chain and the typed comparison are both
    // real. The identity values themselves are produced by the shared
    // authority over a named path set; computing them from real capture is
    // proven by the release-ledger tests (SPEC-0301), not re-derived here.
    let recipe = crate::reports::release_ledger::range_touched_recipe();
    let carried = crate::reports::path_inventory::path_inventory(
        &[
            "crates/ripr/src/lib.rs".to_string(),
            "docs/lead-with-recovery.md".to_string(),
        ],
        crate::reports::release_ledger::RANGE_TOUCHED_SOURCE,
    );
    if carried.path_record_recipe != recipe {
        return Err("fixture identity must carry the ledger recipe".to_string());
    }
    let ledger_packet = serde_json::to_vec_pretty(&json!({
        "schema_version": "0.2",
        "kind": "release_ledger",
        "source": {"base_sha": LAST_INTEGRATED, "cut_sha": sha,
            "cut_tree_sha": tree, "main_sha": sha,
            "repository": "EffortlessMetrics/ripr-swarm"},
        "path_inventories": {"record_recipe": recipe, "range_touched": {
            "path_record_count": carried.path_record_count,
            "path_record_sha256": carried.path_record_sha256,
            "path_record_recipe": recipe}}
    }))
    .map_err(|error| error.to_string())?;
    std::fs::write(fixture.root.join("denominator.json"), &ledger_packet)
        .map_err(|error| error.to_string())?;
    let denominator_digest = json!(digest(&ledger_packet));
    fixture.document["prerequisites"]["denominator"]["packet"]["sha256"] =
        denominator_digest.clone();
    fixture.document["prerequisites"]["denominator"]["acceptance"]["reviewed_packet_sha256"] =
        denominator_digest;
    fixture.document["range"]["path_record_count"] = json!(carried.path_record_count as u64);
    fixture.document["range"]["path_record_sha256"] =
        json!(carried.path_record_sha256.trim_start_matches("sha256:"));
    fixture.document["range"]["path_record_recipe"] = json!(recipe);
    let approved = fixture.write()?;
    let input = QualificationInput::new(
        fixture.root.clone(),
        source.0.clone(),
        PathBuf::from("manifest.json"),
    )?
    .with_approved_manifest_digest(approved)?;
    let admitted = AdmittedSource::admit(&input, "0.11.0")?;
    admitted.revalidate()?;
    for field in [
        "all_reachable_count",
        "first_parent_count",
        "all_reachable_sha256",
        "first_parent_sha256",
    ] {
        let original = fixture.document["range"][field].clone();
        fixture.document["range"][field] = if field == "first_parent_count" {
            json!(2) // Plausible and within all-reachable count; reach the Git comparison.
        } else if field == "all_reachable_count" {
            json!(5)
        } else {
            json!("0".repeat(64))
        };
        let substituted = QualificationInput::new(
            fixture.root.clone(),
            source.0.clone(),
            PathBuf::from("manifest.json"),
        )?
        .with_approved_manifest_digest(fixture.write()?)?;
        match AdmittedSource::admit(&substituted, "0.11.0") {
            Err(error) if error.contains("Git range counts or ordered digests differ") => (),
            Err(error) => return Err(format!("wrong {field} discriminator: {error}")),
            Ok(_) => return Err(format!("wrong actual Git range admitted: {field}")),
        }
        fixture.document["range"][field] = original;
    }
    fixture.write()?;
    admitted.revalidate()?;
    let original_order = fixture.document["range"]["first_parent_sha256"].clone();
    let wrong_order = first
        .lines()
        .rev()
        .map(|sha| format!("{sha}\n"))
        .collect::<String>();
    fixture.document["range"]["first_parent_sha256"] = json!(digest(wrong_order.as_bytes()));
    let wrong_order_input = QualificationInput::new(
        fixture.root.clone(),
        source.0.clone(),
        PathBuf::from("manifest.json"),
    )?
    .with_approved_manifest_digest(fixture.write()?)?;
    if AdmittedSource::admit(&wrong_order_input, "0.11.0").is_ok() {
        return Err("count-consistent wrong-order range admitted".to_string());
    }
    fixture.document["range"]["first_parent_sha256"] = original_order;
    fixture.write()?;
    admitted.revalidate()?;
    // #4006 item 6 discriminators. Manifest side: a substituted digest and a
    // count that only matches another inventory must each fail the typed
    // comparison against the retained packet.
    let substitute_input = |fixture: &Fixture| {
        QualificationInput::new(
            fixture.root.clone(),
            source.0.clone(),
            PathBuf::from("manifest.json"),
        )
        .and_then(|input| input.with_approved_manifest_digest(fixture.write()?))
    };
    for (field, wrong) in [
        (
            "path_record_sha256",
            json!(digest(carried.path_record_sha256.as_bytes())),
        ),
        ("path_record_count", json!(carried.path_record_count + 1)),
    ] {
        let original = fixture.document["range"][field].clone();
        fixture.document["range"][field] = wrong;
        let substituted = substitute_input(&fixture)?;
        match AdmittedSource::admit(&substituted, "0.11.0") {
            Err(error) if error.contains("path-record inventory differs") => (),
            Err(error) => return Err(format!("wrong {field} discriminator: {error}")),
            Ok(_) => return Err(format!("wrong path-record inventory admitted: {field}")),
        }
        fixture.document["range"][field] = original;
    }
    fixture.write()?;
    admitted.revalidate()?;
    // An LF-joined digest of the same paths must not admit (it cannot
    // represent a path containing a newline); the substituted digest above
    // already proves any non-carried digest refuses, so the LF-framing spell
    // is exercised with a different digest value rather than a boundary-tree
    // diff this shallow fixture cannot read. The NUL-versus-LF partition
    // control itself is proven by the release-ledger and source-promotion
    // tests over the shared authority.
    let original_digest = fixture.document["range"]["path_record_sha256"].clone();
    fixture.document["range"]["path_record_sha256"] =
        json!(digest(b"git diff --name-only; LF-joined lines"));
    let substituted = substitute_input(&fixture)?;
    match AdmittedSource::admit(&substituted, "0.11.0") {
        Err(error) if error.contains("path-record inventory differs") => (),
        Err(error) => return Err(format!("LF-joined digest discriminator: {error}")),
        Ok(_) => return Err("an LF-joined path digest was admitted".to_string()),
    }
    fixture.document["range"]["path_record_sha256"] = original_digest;
    fixture.write()?;
    admitted.revalidate()?;
    let wrong_recipe = fixture.document["range"]["path_record_recipe"].clone();
    fixture.document["range"]["path_record_recipe"] =
        json!("git diff --name-only; LF-joined lines; SHA-256");
    let substituted = substitute_input(&fixture)?;
    match AdmittedSource::admit(&substituted, "0.11.0") {
        Err(error) if error.contains("NUL-record authority recipe") => (),
        Err(error) => return Err(format!("wrong recipe discriminator: {error}")),
        Ok(_) => return Err("a foreign path-record recipe was admitted".to_string()),
    }
    fixture.document["range"]["path_record_recipe"] = wrong_recipe;
    fixture.write()?;
    admitted.revalidate()?;
    // Packet side: the retained #2768 bytes carry the identity, so tampering
    // the packet's own digest, dropping the inventory, or naming another
    // base/cut must refuse even though the manifest itself still agrees.
    let mut packet: serde_json::Value =
        serde_json::from_slice(&ledger_packet).map_err(|error| error.to_string())?;
    let restore_packet = |fixture: &mut Fixture,
                          packet: &serde_json::Value|
     -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(packet).map_err(|error| error.to_string())?;
        std::fs::write(fixture.root.join("denominator.json"), &bytes)
            .map_err(|error| error.to_string())?;
        let packet_digest = json!(digest(&bytes));
        fixture.document["prerequisites"]["denominator"]["packet"]["sha256"] =
            packet_digest.clone();
        fixture.document["prerequisites"]["denominator"]["acceptance"]["reviewed_packet_sha256"] =
            packet_digest;
        Ok(())
    };
    packet["path_inventories"]["range_touched"]["path_record_sha256"] =
        json!(format!("sha256:{}", "0".repeat(64)));
    restore_packet(&mut fixture, &packet)?;
    let substituted = substitute_input(&fixture)?;
    match AdmittedSource::admit(&substituted, "0.11.0") {
        Err(error) if error.contains("path-record inventory differs") => (),
        Err(error) => return Err(format!("tampered packet digest discriminator: {error}")),
        Ok(_) => return Err("a tampered #2768 packet digest was admitted".to_string()),
    }
    packet["path_inventories"] = serde_json::Value::Null;
    restore_packet(&mut fixture, &packet)?;
    let substituted = substitute_input(&fixture)?;
    match AdmittedSource::admit(&substituted, "0.11.0") {
        Err(error) if error.contains("missing is not empty") => (),
        Err(error) => return Err(format!("missing-inventory discriminator: {error}")),
        Ok(_) => return Err("a packet without the inventory was admitted".to_string()),
    }
    packet["source"]["cut_sha"] = json!("f".repeat(40));
    restore_packet(&mut fixture, &packet)?;
    let substituted = substitute_input(&fixture)?;
    match AdmittedSource::admit(&substituted, "0.11.0") {
        Err(error) if error.contains("covers another cut") => (),
        Err(error) => return Err(format!("other-cut discriminator: {error}")),
        Ok(_) => return Err("a packet naming another cut was admitted".to_string()),
    }
    packet["source"]["cut_sha"] = json!(sha);
    packet["source"]["base_sha"] = json!("e".repeat(40));
    restore_packet(&mut fixture, &packet)?;
    let substituted = substitute_input(&fixture)?;
    match AdmittedSource::admit(&substituted, "0.11.0") {
        Err(error) if error.contains("covers another range base") => (),
        Err(error) => return Err(format!("other-base discriminator: {error}")),
        Ok(_) => return Err("a packet naming another base was admitted".to_string()),
    }
    restore_packet(
        &mut fixture,
        &serde_json::from_slice(&ledger_packet).map_err(|error| error.to_string())?,
    )?;
    fixture.write()?;
    admitted.revalidate()?;
    fixture.write()?;
    admitted.revalidate()?;
    git(
        &source.0,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "same version other candidate",
        ],
    )?;
    let other = git(&source.0, &["rev-parse", "HEAD"])?.trim().to_string();
    if admitted.revalidate().is_ok() {
        return Err("same-version other HEAD admitted".to_string());
    }
    git(&source.0, &["checkout", "--quiet", "--detach", &sha])?;
    admitted.revalidate()?;
    git(&source.0, &["update-ref", &reference, &other])?;
    if admitted.revalidate().is_ok() {
        return Err("moved candidate ref admitted".to_string());
    }
    git(&source.0, &["update-ref", &reference, &sha])?;
    admitted.revalidate()?;
    git(
        &source.0,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/EffortlessMetrics/ripr.git",
        ],
    )?;
    if admitted.revalidate().is_ok() {
        return Err("foreign source origin admitted".to_string());
    }
    git(
        &source.0,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
        ],
    )?;
    admitted.revalidate()?;
    for origin in [
        "https://github.com/EffortlessMetrics/ripr-swarm",
        "git@github.com:EffortlessMetrics/ripr-swarm.git",
    ] {
        git(&source.0, &["remote", "set-url", "origin", origin])?;
        admitted.revalidate()?;
    }
    git(
        &source.0,
        &[
            "remote",
            "set-url",
            "origin",
            "ssh://git@github.com/EffortlessMetrics/ripr-swarm.git",
        ],
    )?;
    if admitted.revalidate().is_ok() {
        return Err("undocumented origin transport admitted".to_string());
    }
    // A repository-local rewrite must not launder a foreign origin into an
    // approved one: admission judges the configured URL, not Git's rewrite.
    git(
        &source.0,
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.invalid/foreign.git",
        ],
    )?;
    git(
        &source.0,
        &[
            "config",
            "--local",
            "url.https://github.com/EffortlessMetrics/ripr-swarm.git.insteadOf",
            "https://example.invalid/foreign.git",
        ],
    )?;
    if admitted.revalidate().is_ok() {
        return Err("rewritten foreign origin admitted".to_string());
    }
    git(
        &source.0,
        &[
            "config",
            "--local",
            "--unset",
            "url.https://github.com/EffortlessMetrics/ripr-swarm.git.insteadOf",
        ],
    )?;
    // A second origin value is ambiguous even when the first is approved.
    git(
        &source.0,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
        ],
    )?;
    // Count-before-trim: an empty extra value must not hide behind trimming.
    let approved = "https://github.com/EffortlessMetrics/ripr-swarm.git";
    for (first, second) in [
        (approved, "https://example.invalid/second.git"),
        (approved, ""),
        ("", approved),
    ] {
        git(
            &source.0,
            &["config", "--local", "--unset-all", "remote.origin.url"],
        )?;
        git(
            &source.0,
            &["config", "--local", "remote.origin.url", first],
        )?;
        git(
            &source.0,
            &["config", "--local", "--add", "remote.origin.url", second],
        )?;
        if admitted.revalidate().is_ok() {
            return Err(format!(
                "multi-valued source origin admitted: {first:?} + {second:?}"
            ));
        }
    }
    git(
        &source.0,
        &["config", "--local", "--unset-all", "remote.origin.url"],
    )?;
    git(
        &source.0,
        &[
            "config",
            "--local",
            "remote.origin.url",
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
        ],
    )?;
    git(
        &source.0,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/EffortlessMetrics/ripr-swarm.git",
        ],
    )?;
    admitted.revalidate()?;
    fixture.document["candidate"]["tree"] = json!("f".repeat(40));
    for name in ["selected_claims", "denominator", "audit"] {
        fixture.document["prerequisites"][name]["acceptance"]["candidate_tree"] =
            json!("f".repeat(40));
    }
    let wrong_tree_digest = fixture.write()?;
    let wrong_tree = QualificationInput::new(
        fixture.root.clone(),
        source.0.clone(),
        PathBuf::from("manifest.json"),
    )?
    .with_approved_manifest_digest(wrong_tree_digest)?;
    if AdmittedSource::admit(&wrong_tree, "0.11.0").is_ok() {
        return Err(
            "accepted-looking wrong-tree declaration replaced the actual Git tree".to_string(),
        );
    }
    Ok(())
}
