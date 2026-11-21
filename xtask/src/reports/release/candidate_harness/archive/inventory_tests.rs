//! Archive inventory acceptance for committed and publish-stage packages
//! (#15204): every ordinary entry is its committed blob or, under the stripped
//! source root, exactly the inline-test strip of it; a dirty Cargo VCS record
//! requires the marker naming the admitted commit.
use super::{
    ArchiveRules, archive_inventory, owned_cargo_config, package_args, packages_of_manifest,
    qualified_command,
};
use crate::publish_stage::{PUBLISH_STAGE_MARKER, stage_workspace, strip_inline_test_modules};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER_SHA: &str = "89abcdef0123456789abcdef0123456789abcdef";
const MANIFEST: &str = "[package]\nname = \"ripr\"\nversion = \"0.11.0\"\nedition = \"2024\"\n";
const LIB: &str = "pub fn keep() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn one() {\n        assert_eq!(super::keep(), 1);\n    }\n}\n";
const PLAIN: &str = "pub fn plain() {}\n";
const BUILD: &str = "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn one() {}\n}\n";
const README: &str = "readme\n";

fn committed() -> BTreeMap<String, Vec<u8>> {
    [
        ("crates/ripr/Cargo.toml", MANIFEST),
        ("crates/ripr/src/lib.rs", LIB),
        ("crates/ripr/src/plain.rs", PLAIN),
        ("crates/ripr/build.rs", BUILD),
        ("crates/ripr/README.md", README),
    ]
    .into_iter()
    .map(|(path, body)| (path.to_string(), body.as_bytes().to_vec()))
    .collect()
}

fn rules(blobs: &BTreeMap<String, Vec<u8>>) -> ArchiveRules<'_> {
    ArchiveRules {
        package_name: "ripr",
        version: "0.11.0",
        source_sha: SHA,
        package_prefix: "crates/ripr/",
        blobs,
    }
}

fn stripped(source: &str) -> Result<String, String> {
    let stripped = strip_inline_test_modules(source)?;
    if stripped.modules == 0 {
        return Err("fixture strip removed nothing".to_string());
    }
    Ok(stripped.text)
}

fn vcs(dirty: Option<serde_json::Value>) -> String {
    let mut git = serde_json::json!({ "sha1": SHA });
    if let (Some(object), Some(dirty)) = (git.as_object_mut(), dirty) {
        object.insert("dirty".to_string(), dirty);
    }
    serde_json::json!({ "git": git, "path_in_vcs": "crates/ripr" }).to_string()
}

/// The entries `cargo package` writes for a publish stage of [`SHA`].
fn staged_entries() -> Result<BTreeMap<String, String>, String> {
    Ok([
        ("Cargo.toml", MANIFEST.to_string()),
        ("Cargo.toml.orig", MANIFEST.to_string()),
        ("Cargo.lock", "version = 4\n".to_string()),
        (
            ".cargo_vcs_info.json",
            vcs(Some(serde_json::Value::Bool(true))),
        ),
        (PUBLISH_STAGE_MARKER, format!("{SHA}\n")),
        ("src/lib.rs", stripped(LIB)?),
        ("src/plain.rs", PLAIN.to_string()),
        ("build.rs", BUILD.to_string()),
        ("README.md", README.to_string()),
    ]
    .into_iter()
    .map(|(path, body)| (path.to_string(), body))
    .collect())
}

/// The entries of a committed (unstaged) package of [`SHA`].
fn committed_entries() -> Result<BTreeMap<String, String>, String> {
    let mut entries = staged_entries()?;
    entries.remove(PUBLISH_STAGE_MARKER);
    entries.insert("src/lib.rs".to_string(), LIB.to_string());
    entries.insert(".cargo_vcs_info.json".to_string(), vcs(None));
    Ok(entries)
}

fn crate_bytes(entries: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for (path, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, format!("ripr-0.11.0/{path}"), body.as_bytes())
            .map_err(|error| format!("fixture archive entry: {error}"))?;
    }
    builder
        .into_inner()
        .and_then(flate2::write::GzEncoder::finish)
        .map_err(|error| format!("fixture archive: {error}"))
}

fn inventory_of(entries: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    let blobs = committed();
    archive_inventory(&crate_bytes(entries)?, &rules(&blobs))
        .map(|inventory| inventory.into_keys().collect())
}

fn refused(entries: &BTreeMap<String, String>, expected: &str) -> Result<(), String> {
    match inventory_of(entries) {
        Ok(paths) => Err(format!(
            "archive was accepted, expected `{expected}`: {paths:?}"
        )),
        Err(error) if error.contains(expected) => Ok(()),
        Err(error) => Err(format!("expected `{expected}`, got: {error}")),
    }
}

#[test]
fn a_publish_stage_archive_of_the_admitted_commit_is_accepted() -> Result<(), String> {
    let paths = inventory_of(&staged_entries()?)?;
    if !paths.iter().any(|path| path == PUBLISH_STAGE_MARKER) || paths.len() != 9 {
        return Err(format!("unexpected inventory: {paths:?}"));
    }
    Ok(())
}

#[test]
fn a_committed_archive_without_stage_is_still_accepted() -> Result<(), String> {
    inventory_of(&committed_entries()?)?;
    let mut clean = committed_entries()?;
    clean.insert(
        ".cargo_vcs_info.json".to_string(),
        vcs(Some(serde_json::Value::Bool(false))),
    );
    inventory_of(&clean).map(|_| ())
}

#[test]
fn a_hand_edited_stripped_source_file_is_refused_with_marker_and_record_intact()
-> Result<(), String> {
    let mut entries = staged_entries()?;
    let edited = format!("{}pub fn injected() {{}}\n", stripped(LIB)?);
    entries.insert("src/lib.rs".to_string(), edited);
    refused(
        &entries,
        "packaged ordinary entry is not the committed source blob: src/lib.rs",
    )
}

#[test]
fn an_edited_or_extra_non_source_entry_is_refused() -> Result<(), String> {
    let mut edited = staged_entries()?;
    edited.insert("README.md".to_string(), "changed\n".to_string());
    refused(
        &edited,
        "packaged ordinary entry is not the committed source blob: README.md",
    )?;
    let mut extra = staged_entries()?;
    extra.insert("src/extra.rs".to_string(), PLAIN.to_string());
    refused(
        &extra,
        "packaged ordinary entry is not the committed source blob: src/extra.rs",
    )
}

#[test]
fn the_strip_is_accepted_only_under_the_stripped_source_root() -> Result<(), String> {
    // build.rs parses and strips like any source file, but the publish stage
    // never strips it, so a stripped build.rs is not an approved transform.
    let mut entries = staged_entries()?;
    entries.insert("build.rs".to_string(), stripped(BUILD)?);
    refused(
        &entries,
        "packaged ordinary entry is not the committed source blob: build.rs",
    )
}

#[test]
fn a_marker_naming_another_commit_is_refused() -> Result<(), String> {
    for marker in [format!("{OTHER_SHA}\n"), SHA.to_string(), String::new()] {
        let mut entries = staged_entries()?;
        entries.insert(PUBLISH_STAGE_MARKER.to_string(), marker);
        refused(
            &entries,
            "publish-stage marker does not name the admitted commit",
        )?;
    }
    Ok(())
}

#[test]
fn stage_evidence_is_accepted_only_all_together() -> Result<(), String> {
    let expected = "produced archive is not a committed or publish-stage package";
    // Dirty record without the marker.
    let mut no_marker = staged_entries()?;
    no_marker.remove(PUBLISH_STAGE_MARKER);
    refused(&no_marker, expected)?;
    // Dirty record and marker, nothing stripped.
    let mut no_strip = staged_entries()?;
    no_strip.insert("src/lib.rs".to_string(), LIB.to_string());
    refused(&no_strip, expected)?;
    // Marker and strip with a clean record.
    let mut clean_record = staged_entries()?;
    clean_record.insert(".cargo_vcs_info.json".to_string(), vcs(None));
    refused(&clean_record, expected)?;
    // A strip with neither marker nor dirty record.
    let mut bare_strip = committed_entries()?;
    bare_strip.insert("src/lib.rs".to_string(), stripped(LIB)?);
    refused(&bare_strip, expected)?;
    // Dirty with no stage at all.
    let mut dirty = committed_entries()?;
    dirty.insert(
        ".cargo_vcs_info.json".to_string(),
        vcs(Some(serde_json::Value::Bool(true))),
    );
    refused(&dirty, expected)?;
    // A non-boolean dirty field.
    let mut odd = staged_entries()?;
    odd.insert(
        ".cargo_vcs_info.json".to_string(),
        vcs(Some(serde_json::json!("true"))),
    );
    refused(
        &odd,
        "produced Cargo VCS identity differs from admitted source",
    )
}

#[test]
fn a_staged_archive_with_strippable_source_left_unstripped_is_refused() -> Result<(), String> {
    let mut entries = staged_entries()?;
    // Another strippable committed file, packaged unstripped beside the
    // stripped lib.rs: the stage strips every such file, so this is refused.
    entries.insert("src/more.rs".to_string(), LIB.to_string());
    let mut blobs = committed();
    blobs.insert(
        "crates/ripr/src/more.rs".to_string(),
        LIB.as_bytes().to_vec(),
    );
    match archive_inventory(&crate_bytes(&entries)?, &rules(&blobs)) {
        Err(error) if error.contains("unstripped strippable entries=1") => Ok(()),
        other => Err(format!(
            "expected refusal, got {:?}",
            other.map(|i| i.len())
        )),
    }
}

#[test]
fn only_a_staged_package_is_allowed_dirty_and_path_dependencies_are_packaged() -> Result<(), String>
{
    let manifest = "[package]\nname = \"ripr\"\n\n[dependencies]\nripr-contract = { path = \"../ripr-contract\", version = \"=0.11.0\" }\nserde = \"1\"\n\n[target.'cfg(unix)'.build-dependencies]\nhelper = { package = \"ripr-helper\", path = \"../helper\" }\n\n[dev-dependencies]\nfixture = { path = \"../fixture\" }\n";
    let packages = packages_of_manifest(manifest.as_bytes(), "ripr")?;
    if packages != ["ripr", "ripr-contract", "ripr-helper"] {
        return Err(format!("unexpected packaged packages: {packages:?}"));
    }
    let path = Path::new("x");
    let staged = package_args(path, path, path, true, &packages);
    let checkout = package_args(path, path, path, false, &packages);
    if !staged.iter().any(|arg| arg == "--allow-dirty")
        || checkout.iter().any(|arg| arg == "--allow-dirty")
        || !checkout.iter().any(|arg| arg == "--locked")
    {
        return Err(format!("unexpected arguments: {staged:?} / {checkout:?}"));
    }
    Ok(())
}

/// Real Cargo output from a real publish stage passes the inventory, so the
/// rules above describe what `cargo package` actually writes.
#[test]
fn a_real_multi_package_publish_stage_archive_passes_the_inventory() -> Result<(), String> {
    let root = std::env::current_dir()
        .map_err(|error| error.to_string())?
        .join("target")
        .join(format!("inventory-stage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let result = real_stage_archive(&root);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn real_stage_archive(root: &Path) -> Result<(), String> {
    let io = |error: std::io::Error| error.to_string();
    let workspace = root.join("workspace");
    let ripr_manifest = format!(
        "[package]\nname = \"ripr\"\nversion = \"0.11.0\"\nedition = \"2024\"\nlicense = \"MIT\"\ndescription = \"fixture\"\ninclude = [\"/src/**/*.rs\", \"/Cargo.toml\", \"/{PUBLISH_STAGE_MARKER}\"]\n\n[dependencies]\nripr-contract = {{ path = \"../ripr-contract\", version = \"=0.11.0\" }}\n"
    );
    let files = [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/ripr\", \"crates/ripr-contract\"]\nresolver = \"2\"\n".to_string(),
        ),
        ("crates/ripr/Cargo.toml", ripr_manifest),
        ("crates/ripr/src/lib.rs", LIB.to_string()),
        ("crates/ripr/src/plain.rs", PLAIN.to_string()),
        (
            "crates/ripr-contract/Cargo.toml",
            "[package]\nname = \"ripr-contract\"\nversion = \"0.11.0\"\nedition = \"2024\"\nlicense = \"MIT\"\ndescription = \"fixture\"\n".to_string(),
        ),
        ("crates/ripr-contract/src/lib.rs", PLAIN.to_string()),
    ];
    for (path, body) in &files {
        let file = workspace.join(path);
        std::fs::create_dir_all(file.parent().ok_or("fixture parent")?).map_err(io)?;
        std::fs::write(&file, body).map_err(io)?;
    }
    let run = |program: &str, args: &[&str], cwd: &Path| {
        let args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        qualified_command(
            Path::new(program),
            &args,
            cwd,
            Duration::from_mins(5),
            "inventory fixture",
        )
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    run("cargo", &["generate-lockfile", "--offline"], &workspace)?;
    run("git", &["init", "-q"], &workspace)?;
    run("git", &["add", "-A"], &workspace)?;
    run(
        "git",
        &[
            "-c",
            "user.name=inventory-test",
            "-c",
            "user.email=inventory-test@example.invalid",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
        &workspace,
    )?;
    let sha = run("git", &["rev-parse", "HEAD"], &workspace)?;
    let stage = root.join("stage");
    let report = stage_workspace(&workspace, &stage)?;
    if report.commit != sha || report.stripped_files != 1 {
        return Err(format!("unexpected stage report: {report:?}"));
    }
    let target = root.join("target");
    // The producer's own argument and package derivation, so a missing
    // `--allow-dirty` or path dependency shows up here as a Cargo refusal.
    let packages = packages_of_manifest(files[1].1.as_bytes(), "ripr")?;
    if packages != ["ripr", "ripr-contract"] {
        return Err(format!("unexpected packaged packages: {packages:?}"));
    }
    let config = owned_cargo_config(root, "package", None)?;
    let args = package_args(&config, &stage.join("Cargo.toml"), &target, true, &packages);
    qualified_command(
        Path::new("cargo"),
        &args,
        &stage,
        Duration::from_mins(5),
        "inventory fixture package",
    )?;
    let bytes = std::fs::read(target.join("package/ripr-0.11.0.crate")).map_err(io)?;
    let mut blobs = BTreeMap::new();
    for (path, body) in &files {
        blobs.insert((*path).to_string(), body.as_bytes().to_vec());
    }
    let rules = ArchiveRules {
        package_name: "ripr",
        version: "0.11.0",
        source_sha: &sha,
        package_prefix: "crates/ripr/",
        blobs: &blobs,
    };
    let inventory = archive_inventory(&bytes, &rules)?;
    let lib = inventory
        .get("src/lib.rs")
        .ok_or("real archive lacks src/lib.rs")?;
    if lib.bytes != stripped(LIB)?.as_bytes() || !inventory.contains_key(PUBLISH_STAGE_MARKER) {
        return Err("real archive is not the stripped publish stage".to_string());
    }
    Ok(())
}
