//! Whole-subject controls for the closed asserted equality helper (#13181).
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ORIGINAL: &str = include_str!(
    "../../../fixtures/rust-verdict-corpus/subjects/authored-mined-wire/src/lib.rs.txt"
);
const DIFF: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/cases/mined-bool-property-read-u32.diff");
const MANIFEST: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/subjects/authored-mined-wire/Cargo.toml");
const OWNER: &str = "u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])";
const COMPARISON: &str = "read_u32(&value.to_be_bytes()) == value";

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-asserted-bool-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn postimage(source: &str) -> Result<String, String> {
    if source.matches(OWNER).count() != 1 {
        return Err("original owner replacement must be unique".into());
    }
    Ok(source.replacen(OWNER, &format!("{OWNER} | 0"), 1))
}
fn finding(source: &str, repo: bool) -> Result<Value, String> {
    finding_with_proc_macro(source, repo, false)
}

fn finding_with_proc_macro(source: &str, repo: bool, local_macro: bool) -> Result<Value, String> {
    finding_with_manifest(source, repo, local_macro, "")
}

fn finding_with_manifest(
    source: &str,
    repo: bool,
    local_macro: bool,
    library_settings: &str,
) -> Result<Value, String> {
    finding_with_stimulus(
        &postimage(source)?,
        DIFF,
        repo,
        local_macro,
        library_settings,
    )
}

fn finding_with_stimulus(
    source_postimage: &str,
    diff_text: &str,
    repo: bool,
    local_macro: bool,
    library_settings: &str,
) -> Result<Value, String> {
    let scratch = Scratch::new()?;
    let mut manifest = MANIFEST.to_owned();
    manifest.push_str(library_settings);
    if local_macro {
        manifest.push_str("\n[dev-dependencies]\nshadow={path=\"shadow\"}\n[workspace]\nmembers=[\"shadow\"]\nresolver=\"2\"\n");
        std::fs::create_dir_all(scratch.0.join("shadow/src")).map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("shadow/Cargo.toml"), "[package]\nname=\"shadow\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\nproc-macro=true\n").map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("shadow/src/lib.rs"), r#"extern crate proc_macro;
use proc_macro::TokenStream;
fn expansion() -> TokenStream {
    "const read_u32: fn(&[u8]) -> u32 = |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);".parse().unwrap_or_else(|_| TokenStream::new())
}
#[proc_macro_derive(Shadow)] pub fn shadow(_: TokenStream) -> TokenStream { expansion() }
#[proc_macro_derive(Debug)] pub fn debug(_: TokenStream) -> TokenStream { expansion() }
"#).map_err(|error| error.to_string())?;
    }
    std::fs::write(scratch.0.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source_postimage)
        .map_err(|error| error.to_string())?;
    let diff = scratch.0.join("case.diff");
    std::fs::write(&diff, diff_text).map_err(|error| error.to_string())?;
    let input = CheckInput {
        root: scratch.0.clone(),
        diff_file: (!repo).then_some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    };
    let report = if repo {
        ripr::app::check_workspace_repo(input)?
    } else {
        check_workspace(input)?
    };
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let found: Vec<_> = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|finding| {
            finding["probe"]["family"] == "return_value"
                && finding["probe"]["line"] == 26
                && finding["probe"]["expression"]
                    .as_str()
                    .is_some_and(|expression| {
                        expression.starts_with("u32::from_be_bytes")
                            || expression.starts_with("u32::from_le_bytes")
                    })
        })
        .collect();
    let [finding] = found.as_slice() else {
        return Err(format!(
            "expected one exact read_u32 return finding: {json}"
        ));
    };
    Ok((*finding).clone())
}

#[test]
fn disabled_library_targets_withhold_the_new_helper_observer() -> Result<(), String> {
    // Keep the custom-harness program native-valid. The source is identical
    // across settings, including the ordinary-harness positive.
    let source = format!("{ORIGINAL}\nfn main() {{}}\n");
    for repo in [false, true] {
        let positive = finding_with_manifest(&source, repo, false, "")?;
        assert!(
            positive
                .to_string()
                .contains("asserted_u32_helper_comparison")
        );
        assert_eq!(
            positive["observed_values"]
                .as_array()
                .ok_or("positive observed values")?
                .iter()
                .filter(|value| value.to_string().contains("bytes = ["))
                .count(),
            4,
            "{positive}"
        );
        for settings in ["\n[lib]\nharness=false\n", "\n[lib]\ntest=false\n"] {
            let finding = finding_with_manifest(&source, repo, false, settings)?;
            assert!(
                !finding
                    .to_string()
                    .contains("asserted_u32_helper_comparison"),
                "repo={repo}, {settings}: {finding}"
            );
            assert_ne!(finding["classification"], "exposed", "{finding}");
            assert!(
                finding["observed_values"]
                    .as_array()
                    .ok_or("disabled target observed values")?
                    .iter()
                    .all(|value| !value.to_string().contains("bytes = [")),
                "disabled target must not lend owner byte rows: {finding}"
            );
        }
    }
    Ok(())
}

#[test]
fn non_ascii_owner_names_cannot_borrow_the_helper_receipt() -> Result<(), String> {
    let source = ORIGINAL.replace("read_u32", "read_é");
    let wrong_return = OWNER.replace("from_be_bytes", "from_le_bytes");
    let wrong_owner = source.replacen(OWNER, &wrong_return, 1);
    let shadowed = wrong_owner.replacen(
        "    use super::*;",
        "    use super::*; macro_rules! shadow { () => { fn read_é(b: &[u8])->u32 { u32::from_be_bytes([b[0],b[1],b[2],b[3]]) } }; } shadow!();",
        1,
    );
    let shadow_diff = DIFF.replace("read_u32", "read_é").replace(
        &format!("+    {OWNER} | 0"),
        &format!("+    {wrong_return}"),
    );
    for repo in [false, true] {
        let plain = finding(&source, repo)?;
        let shadow = finding_with_stimulus(&shadowed, &shadow_diff, repo, false, "")?;
        for finding in [plain, shadow] {
            assert!(
                !finding
                    .to_string()
                    .contains("asserted_u32_helper_comparison"),
                "repo={repo}: {finding}"
            );
            assert!(
                finding["observed_values"]
                    .as_array()
                    .ok_or("unsupported owner observed values")?
                    .iter()
                    .all(|value| !value.to_string().contains("bytes = [")),
                "unsupported owner must not lend helper byte rows: {finding}"
            );
        }
    }
    Ok(())
}

#[test]
fn full_original_asserted_comparison_observes_the_changed_owner_return() -> Result<(), String> {
    assert_eq!(ORIGINAL.len(), 3136);
    for repo in [false, true] {
        let finding = finding(ORIGINAL, repo)?;
        assert_eq!(
            finding["classification"], "exposed",
            "repo={repo}: {finding}"
        );
        assert_eq!(finding["ripr"]["propagate"]["state"], "yes", "{finding}");
        assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
        assert!(
            finding
                .to_string()
                .contains("asserted_u32_helper_comparison"),
            "{finding}"
        );
        assert!(
            finding["related_tests"]
                .as_array()
                .ok_or("related tests")?
                .iter()
                .any(|test| {
                    test["name"] == "u32_reads_what_was_written"
                        && test["relation_reason"] == "helper_owner_call"
                }),
            "{finding}"
        );
    }
    Ok(())
}

#[test]
fn discarded_replaced_lossy_shadowed_or_bypassed_results_get_no_comparison_receipt()
-> Result<(), String> {
    let controls = [
        (
            "discarded result",
            "assert!(reads_back(value));",
            "let _ = reads_back(value);",
        ),
        (
            "discarded comparison",
            COMPARISON,
            "let _ = read_u32(&value.to_be_bytes()) == value; true",
        ),
        (
            "lossy projection",
            COMPARISON,
            "(read_u32(&value.to_be_bytes()) & 0xff) == (value & 0xff)",
        ),
        (
            "owner result replaced",
            COMPARISON,
            "let _ = read_u32(&value.to_be_bytes()); value == value",
        ),
        (
            "rebound parameter",
            COMPARISON,
            "let value = value & 0xff; read_u32(&value.to_be_bytes()) == value",
        ),
        (
            "negated assertion",
            "assert!(reads_back(value));",
            "assert!(!reads_back(value));",
        ),
        (
            "conditional assertion",
            "assert!(reads_back(value));",
            "if false { assert!(reads_back(value)); }",
        ),
        (
            "early return",
            "assert!(reads_back(value));",
            "return; assert!(reads_back(value));",
        ),
        (
            "diagnostic exits",
            "assert!(reads_back(value));",
            "assert!(reads_back(value), \"{}\", { return; 0 });",
        ),
        (
            "replacement bool",
            "assert!(reads_back(value));",
            "let _ = reads_back(value); assert!(true);",
        ),
        (
            "shadow helper",
            "assert!(reads_back(value));",
            "let reads_back = |_: u32| true; assert!(reads_back(value));",
        ),
        (
            "no-op assert",
            "    use super::*;",
            "    use super::*; macro_rules! assert { ($($t:tt)*) => {}; }",
        ),
        (
            "removed assertion",
            "assert!(reads_back(value));",
            "#[cfg(any())] assert!(reads_back(value));",
        ),
        (
            "dynamic row exit",
            "[1, 0x0102_0304, 0xdead_beef, u32::MAX - 1]",
            "[1, { return; 2 }, 3, 4]",
        ),
    ];
    for (name, before, after) in controls {
        assert_eq!(ORIGINAL.matches(before).count(), 1, "{name}");
        let source = ORIGINAL.replacen(before, after, 1);
        let finding = finding(&source, false)?;
        assert_ne!(finding["classification"], "exposed", "{name}: {finding}");
        assert!(
            !finding
                .to_string()
                .contains("asserted_u32_helper_comparison"),
            "{name}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn primitive_and_owner_bindings_cannot_be_borrowed_from_names() -> Result<(), String> {
    for (name, insertion) in [
        ("owner const", "const read_u32: fn(&[u8]) -> u32 = |_| 0;"),
        (
            "macro-generated owner const",
            "macro_rules! shadow { () => { const read_u32: fn(&[u8]) -> u32 = |b| u32::from_be_bytes([b[0],b[1],b[2],b[3]]); }; } shadow!();",
        ),
        (
            "raw macro-generated owner const",
            "macro_rules! shadow { () => { const r#read_u32: fn(&[u8]) -> u32 = |b| u32::from_be_bytes([b[0],b[1],b[2],b[3]]); }; } shadow!();",
        ),
        ("forged primitive", "type u32 = std::primitive::u32;"),
        ("foreign conversion trait", "use std::primitive::u32;"),
    ] {
        let source = ORIGINAL.replacen(
            "    use super::*;",
            &format!("    use super::*; {insertion}"),
            1,
        );
        let finding = finding(&source, false)?;
        assert!(
            !finding
                .to_string()
                .contains("asserted_u32_helper_comparison"),
            "{name}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn unsupported_helper_keeps_an_independent_direct_assertion() -> Result<(), String> {
    let source = ORIGINAL.replace(COMPARISON, "let _ = read_u32(&value.to_be_bytes()); true")
        + "\n#[test] fn independent_u32_pin() { assert_eq!(read_u32(&[1, 2, 3, 4]), 0x0102_0304); }\n";
    let finding = finding(&source, false)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert!(
        !finding
            .to_string()
            .contains("asserted_u32_helper_comparison"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn comparison_of_another_converter_supplies_no_owner_authority() -> Result<(), String> {
    let source = ORIGINAL.replace(COMPARISON, "other_read_u32(&value.to_be_bytes()) == value")
        + "\nfn other_read_u32(_: &[u8]) -> u32 { 0 }\n";
    let finding = finding(&source, false)?;
    assert!(
        !finding
            .to_string()
            .contains("asserted_u32_helper_comparison"),
        "{finding}"
    );
    Ok(())
}

#[test]
fn sibling_proc_macro_expansions_cannot_supply_owner_identity() -> Result<(), String> {
    for inserted in [
        "#[derive(shadow::Shadow)] struct ShadowOwner;",
        "macro_rules! import_derive { () => { use shadow::Debug; }; } import_derive!(); #[derive(Debug)] struct ShadowOwner;",
    ] {
        let source = ORIGINAL.replacen(
            "    use super::*;",
            &format!("    use super::*; {inserted}"),
            1,
        );
        for repo in [false, true] {
            let finding = finding_with_proc_macro(&source, repo, true)?;
            assert!(
                !finding
                    .to_string()
                    .contains("asserted_u32_helper_comparison"),
                "{finding}"
            );
            assert_ne!(finding["classification"], "exposed", "{finding}");
        }
    }
    Ok(())
}
