//! Public pipeline and rendered-output contract for RIPR-SPEC-0153 (#11545).
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str = "[workspace]\n[package]\nname='stimulus'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nquickcheck='1'\n";
const SOURCE: &str = "use quickcheck::Arbitrary;\n#[derive(Clone, Debug)]\nstruct Pair(u8, u8);\nimpl quickcheck::Arbitrary for Pair {\n    fn arbitrary(g: &mut quickcheck::Gen) -> Self {\n        let a = u8::arbitrary(g);\n        Pair(a, a.wrapping_add(2))\n    }\n}\n";

struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-generator-rendered-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("Cargo.toml"), MANIFEST).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        let (line, after) = source
            .lines()
            .enumerate()
            .find(|(_, s)| s.contains("wrapping_add(2)"))
            .ok_or("missing changed expression")?;
        let line = line + 1;
        let before = after.replace("wrapping_add(2)", "wrapping_add(1)");
        std::fs::write(fixture.0.join("diff.patch"), format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{before}\n+{after}\n"
        )).map_err(|e| e.to_string())?;
        Ok(fixture)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn rendered(source: &str, generator: bool) -> Result<(), String> {
    let fixture = Fixture::new(source)?;
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    assert!(
        !output.findings.is_empty(),
        "changed subject remains visible"
    );
    let expected = if generator {
        ExposureClass::StaticUnknown
    } else {
        ExposureClass::NoStaticPath
    };
    assert!(output.findings.iter().all(|f| f.class == expected));
    for format in [OutputFormat::Json, OutputFormat::HumanFull] {
        let text = render_check(&output, &format)?;
        assert!(text.contains(expected.as_str()), "{text}");
        assert_eq!(
            text.contains("test-input generator changes test stimulus"),
            generator,
            "{text}"
        );
        if generator {
            assert!(!text.contains("No reachable test oracle"), "{text}");
            assert!(
                output
                    .findings
                    .iter()
                    .all(|f| f.canonical_gap.is_none() && f.related_tests.is_empty())
            );
        } else {
            assert!(
                output.findings.iter().all(|f| f.canonical_gap.is_some()),
                "production findings retain canonical gaps"
            );
        }
    }
    Ok(())
}

#[test]
fn verified_generator_remains_visible_in_json_and_human() -> Result<(), String> {
    rendered(SOURCE, true)
}

#[test]
fn lookalike_and_ordinary_arbitrary_keep_production_gap_in_rendered_output() -> Result<(), String> {
    let lookalike = SOURCE
        .replace(
            "use quickcheck::Arbitrary;",
            "trait Arbitrary { fn arbitrary(g: &mut quickcheck::Gen) -> Self; }",
        )
        .replace("impl quickcheck::Arbitrary", "impl Arbitrary")
        .replace("u8::arbitrary(g)", "0u8");
    rendered(&lookalike, false)?;
    rendered(
        "pub fn arbitrary(a: u8) -> u8 {\n    a.wrapping_add(2)\n}\n",
        false,
    )
}
