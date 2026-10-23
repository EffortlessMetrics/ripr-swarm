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
        assert!(fingerprint.contains(&format!("|{}|", path.trim_start_matches("./"))));
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
    assert!(literal_fingerprint.contains(r"|src/a\b.rs|"));
    // Drive-looking text remains a native Unix name, not a Windows path.
    let (uri, fingerprint) = artifact(&mut output, Path::new(r"C:\src\a%b.rs"))?;
    assert_eq!(uri, "C%3A%5Csrc%5Ca%25b.rs");
    assert!(fingerprint.contains(r"|C:\src\a%b.rs|"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn native_windows_separators_are_slashes() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, mut output) = fixture()?;
    let (uri, fingerprint) = artifact(&mut output, Path::new(r".\src\a b%2F.rs"))?;
    assert_eq!(uri, "src/a%20b%252F.rs");
    assert!(fingerprint.contains("|src/a b%2F.rs|"));
    Ok(())
}
