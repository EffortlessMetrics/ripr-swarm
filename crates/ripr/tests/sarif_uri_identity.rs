//! Public SARIF renderer controls for native UTF-8 filename identity.
//! The fixture obtains a real finding before exercising its retained location.
#![cfg(feature = "lang-rust")]
use ripr::{CheckInput, CheckOutput, ExposureClass, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct FixtureDir(PathBuf);
impl FixtureDir {
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        std::fs::create_dir_all(&root)?;
        let path = root.join(format!(
            "ripr-sarif-uri-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> Result<(FixtureDir, CheckOutput), Box<dyn std::error::Error>> {
    let dir = FixtureDir::new()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/a"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"uri_probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn accepts(value: i32) -> bool {\n    value > 0\n}\n",
    )?;
    std::fs::write(
        root.join("change.patch"),
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn accepts(value: i32) -> bool {\n-    value >= 0\n+    value > 0\n }\n",
    )?;
    let mut output = check_workspace(CheckInput {
        root: root.to_path_buf(),
        diff_file: Some(root.join("change.patch")),
        ..CheckInput::default()
    })?;
    assert!(
        !output.findings.is_empty(),
        "setup must produce an actual finding"
    );
    output.findings.truncate(1);
    output.findings[0].class = ExposureClass::WeaklyExposed;
    Ok((dir, output))
}

fn artifact(
    output: &mut CheckOutput,
    path: &Path,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    output.findings[0].probe.location.file = output.root.join(path);
    let text = render_check(output, &OutputFormat::Sarif)?;
    let document: Value = serde_json::from_str(&text)?;
    assert_eq!(document["version"], "2.1.0");
    let results = document["runs"][0]["results"]
        .as_array()
        .ok_or("missing results")?;
    assert_eq!(
        results.len(),
        1,
        "setup requires exactly one emitted result"
    );
    let uri = results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
        .as_str()
        .ok_or("missing URI")?
        .to_owned();
    let fingerprint = results[0]["fingerprints"]["riprFingerprintV1"]
        .as_str()
        .ok_or("missing fingerprint")?
        .to_owned();
    assert_eq!(
        results[0]["fingerprints"],
        results[0]["partialFingerprints"]
    );
    Ok((uri, fingerprint))
}

#[test]
fn ordinary_escaping_controls() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, mut output) = fixture()?;
    for (path, expected) in [
        ("src/plain.rs", "src/plain.rs"),
        ("./src/plain.rs", "src/plain.rs"),
        ("../src/plain.rs", "../src/plain.rs"),
        ("src/a%2Fb.rs", "src/a%252Fb.rs"),
        ("src/space #?.rs", "src/space%20%23%3F.rs"),
        ("src/dé🦀.rs", "src/d%C3%A9%F0%9F%A6%80.rs"),
        ("src/de\u{301}.rs", "src/de%CC%81.rs"),
        ("scheme:name.rs", "scheme%3Aname.rs"),
        ("src/quote\"line\n.rs", "src/quote%22line%0A.rs"),
    ] {
        let (uri, fingerprint) = artifact(&mut output, Path::new(path))?;
        assert_eq!(uri, expected, "{path:?}");
        assert_eq!(
            fingerprint.rsplit('|').next(),
            Some(path.trim_start_matches("./"))
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn native_backslash_does_not_alias_directory() -> Result<(), Box<dyn std::error::Error>> {
    let (dir, mut output) = fixture()?;
    let literal = Path::new(r"src/a\b.rs");
    let nested = Path::new("src/a/b.rs");
    std::fs::write(dir.path().join(literal), "literal filename")?;
    std::fs::write(dir.path().join(nested), "nested filename")?;
    assert_ne!(
        std::fs::read(dir.path().join(literal))?,
        std::fs::read(dir.path().join(nested))?
    );
    let (literal_uri, literal_fingerprint) = artifact(&mut output, literal)?;
    let (nested_uri, nested_fingerprint) = artifact(&mut output, nested)?;
    println!("native literal URI={literal_uri}; nested URI={nested_uri}");
    assert_eq!(literal_uri, "src/a%5Cb.rs");
    assert_eq!(nested_uri, "src/a/b.rs");
    assert_ne!(literal_uri, nested_uri);
    assert_ne!(literal_fingerprint, nested_fingerprint);
    assert_eq!(literal_fingerprint.rsplit('|').next(), Some(r"src/a\b.rs"));
    // Drive-looking text remains a native Unix name, not a Windows path.
    let drive_looking = r"C:\path\to\a%b.rs";
    let (uri, fingerprint) = artifact(&mut output, Path::new(drive_looking))?;
    assert_eq!(uri, "C%3A%5Cpath%5Cto%5Ca%25b.rs");
    assert_eq!(fingerprint.rsplit('|').next(), Some(drive_looking));
    Ok(())
}

#[cfg(windows)]
#[test]
fn native_windows_separators_are_slashes() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, mut output) = fixture()?;
    let (uri, fingerprint) = artifact(&mut output, Path::new(r".\src\a b%2F.rs"))?;
    assert_eq!(uri, "src/a%20b%252F.rs");
    assert_eq!(fingerprint.rsplit('|').next(), Some("src/a b%2F.rs"));
    Ok(())
}

/// #16271: check JSON and SARIF give the same policy disposition for two
/// distinct native files, `src/a/b.rs` and a file literally named `a\b.rs`
/// under `src/`. Both findings exist before policy; a `src/a/**` glob selects
/// only the nested file and a `finding_id` entry selects only the literal one,
/// keyed by each finding's identity, and the two SARIF URIs stay distinct.
#[cfg(unix)]
#[test]
fn suppression_policy_keeps_native_backslash_file_distinct()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = FixtureDir::new()?;
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/a"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"uri_probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    let body = "pub fn accepts(value: i32) -> bool {\n    value > 0\n}\n";
    let nested = "src/a/b.rs";
    let literal = r"src/a\b.rs";
    std::fs::write(root.join(nested), body)?;
    std::fs::write(root.join(literal), body)?;
    let hunk = "@@ -1,3 +1,3 @@\n pub fn accepts(value: i32) -> bool {\n-    value >= 0\n+    value > 0\n }\n";
    let mut patch = String::new();
    for file in [nested, literal] {
        patch.push_str(&format!(
            "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n{hunk}"
        ));
    }
    std::fs::write(root.join("change.patch"), patch)?;
    let run = |policy: Option<&str>| -> Result<CheckOutput, Box<dyn std::error::Error>> {
        let suppression_policy = match policy {
            Some(text) => {
                std::fs::write(root.join("policy.toml"), text)?;
                Some(PathBuf::from("policy.toml"))
            }
            None => None,
        };
        Ok(check_workspace(CheckInput {
            root: root.to_path_buf(),
            diff_file: Some(root.join("change.patch")),
            suppression_policy,
            ..CheckInput::default()
        })?)
    };
    // Each renderer is read independently: finding id -> (suppressed, the
    // path it emits). JSON carries `probe.file`; SARIF carries the URI, and a
    // same-line alert folds other probes into `properties.folded_results`
    // (#11219), which share the kept result's URI and suppression marking.
    type Emitted = std::collections::BTreeMap<String, (bool, String)>;
    let json_of = |output: &CheckOutput| -> Result<Emitted, Box<dyn std::error::Error>> {
        let json: Value = serde_json::from_str(&render_check(output, &OutputFormat::Json)?)?;
        let mut emitted = Emitted::new();
        for finding in json["findings"].as_array().ok_or("missing findings")? {
            let id = finding["id"].as_str().ok_or("missing id")?.to_owned();
            let file = finding["probe"]["file"]
                .as_str()
                .ok_or("missing probe.file")?;
            let previous = emitted.insert(id, (finding["suppressed"] == true, file.to_owned()));
            assert!(previous.is_none(), "duplicate JSON finding id");
        }
        Ok(emitted)
    };
    let sarif_of = |output: &CheckOutput| -> Result<Emitted, Box<dyn std::error::Error>> {
        let sarif: Value = serde_json::from_str(&render_check(output, &OutputFormat::Sarif)?)?;
        let mut emitted = Emitted::new();
        for result in sarif["runs"][0]["results"]
            .as_array()
            .ok_or("missing results")?
        {
            let id = result["properties"]["finding_id"]
                .as_str()
                .ok_or("missing finding_id")?;
            let uri = result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
                .as_str()
                .ok_or("missing URI")?;
            let suppressed = result.get("suppressions").is_some();
            let folded = result["properties"]["folded_results"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|folded| folded["finding_id"].as_str());
            for member in std::iter::once(id).chain(folded) {
                let previous = emitted.insert(member.to_owned(), (suppressed, uri.to_owned()));
                assert!(previous.is_none(), "duplicate SARIF finding id {member}");
            }
        }
        Ok(emitted)
    };
    let ids_in = |output: &CheckOutput, wanted: &str| -> Result<Vec<String>, String> {
        let ids: Vec<String> = output
            .findings
            .iter()
            .filter(|finding| finding.probe.location.file.ends_with(wanted))
            .map(|finding| finding.id.clone())
            .collect();
        if ids.is_empty() {
            Err(format!("setup must produce a finding in {wanted}"))
        } else {
            Ok(ids)
        }
    };

    let baseline = run(None)?;
    let nested_ids = ids_in(&baseline, nested)?;
    let literal_ids = ids_in(&baseline, literal)?;
    assert_eq!(
        baseline.findings.len(),
        nested_ids.len() + literal_ids.len()
    );
    // Every phase: both renderers emit exactly the baseline ids, each at its
    // own file, and only `suppressed(id)` ids carry the policy marking.
    let check_phase = |label: &str,
                       output: &CheckOutput,
                       suppressed: &dyn Fn(&str) -> bool|
     -> Result<(), Box<dyn std::error::Error>> {
        let json = json_of(output)?;
        let sarif = sarif_of(output)?;
        for (renderer, emitted) in [("json", &json), ("sarif", &sarif)] {
            let ids: Vec<&String> = emitted.keys().collect();
            let mut expected: Vec<&String> = nested_ids.iter().chain(&literal_ids).collect();
            expected.sort();
            assert_eq!(
                ids, expected,
                "{label}: {renderer} must emit every baseline finding"
            );
        }
        for (ids, json_path, uri) in [
            // JSON percent-encodes the literal backslash in `probe.file`.
            (&nested_ids, "/src/a/b.rs", "src/a/b.rs"),
            (&literal_ids, "/src/a%5Cb.rs", "src/a%5Cb.rs"),
        ] {
            for id in ids {
                let want = suppressed(id);
                assert_eq!(json[id].0, want, "{label}: JSON suppressed for {id}");
                assert!(
                    json[id].1.ends_with(json_path),
                    "{label}: JSON file for {id}: {}",
                    json[id].1
                );
                assert_eq!(sarif[id], (want, uri.to_owned()), "{label}: SARIF for {id}");
            }
        }
        Ok(())
    };
    check_phase("no policy", &baseline, &|_| false)?;

    let entry = |selector: &str| {
        format!(
            "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\n{selector}\nreason = \"paired native filename proof\"\nowner = \"repo-owner\"\n"
        )
    };
    // The glob selects every nested finding and no literal one.
    let glob = run(Some(&entry("path = \"src/a/**\"")))?;
    check_phase("glob", &glob, &|id| {
        nested_ids.iter().any(|nested| nested == id)
    })?;
    // The complementary selection: exactly one chosen literal finding by id.
    let chosen = literal_ids[0].clone();
    let by_id = run(Some(&entry(&format!("finding_id = \"{chosen}\""))))?;
    let chosen_marked = json_of(&by_id)?.get(&chosen).map(|entry| entry.0);
    assert_eq!(
        chosen_marked,
        Some(true),
        "the chosen id must be present and suppressed"
    );
    check_phase("finding_id", &by_id, &|id| id == chosen)?;
    Ok(())
}
