//! Public-API controls for per-byte slice counter boundaries (#5346).
//!
//! A boundary on a counter that grows once per input byte is pinned only when
//! the tests assert the outcome of both literal-input calls that straddle it:
//! the longer call must assert the value the owner returns when the predicate
//! fires and the shorter call something else. One asserted side, or equal
//! outcomes (an earlier `?` exit that never reaches the counter), is not a pin.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "#[derive(Debug, PartialEq)]
pub enum ChunkError {
    TooLong,
    BadDigit,
}

pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {
    let mut size: u64 = 0;
    let mut count = 0;
    for &b in hex {
        count += 1;
        if CONDITION {
            return Err(ChunkError::TooLong);
        }
        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;
        size = (size << 4) | u64::from(digit);
    }
    Ok(size)
}
";

fn source(condition: &str, assertions: &str) -> String {
    format!(
        "{}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn chunk_sizes() {{\n{assertions}\n    }}\n}}\n",
        OWNER.replace("CONDITION", condition)
    )
}

/// A diff rewriting `count > 16` to `16 < count` on the line after
/// `count += 1;`.
fn diff() -> String {
    let line = OWNER
        .lines()
        .position(|text| text.contains("CONDITION"))
        .map_or(0, |index| index + 1);
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{start},3 +{start},3 @@
         count += 1;
-        if count > 16 {{
+        if 16 < count {{
             return Err(ChunkError::TooLong);
",
        start = line - 1
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-slice-counter-boundary-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"slice-counter\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn class_for(condition: &str, assertions: &str) -> Result<ExposureClass, String> {
    predicate_class(&source(condition, assertions), &diff())
}

fn predicate_class(lib: &str, patch: &str) -> Result<ExposureClass, String> {
    let findings = TempRepo::create(lib, patch)?.findings()?;
    let mut predicate = findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate);
    match (predicate.next(), predicate.next()) {
        (Some(finding), None) => Ok(finding.class.clone()),
        other => Err(format!(
            "premise: exactly one predicate finding, got {other:?} in {findings:?}"
        )),
    }
}

fn rewritten_predicate_class(
    lib: &str,
    before: &str,
    after: &str,
) -> Result<ExposureClass, String> {
    let needle = format!("if {after} {{");
    let lines = lib
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim() == needle)
        .collect::<Vec<_>>();
    let [(index, text)] = lines.as_slice() else {
        return Err(format!("expected one rewritten predicate: {lines:?}"));
    };
    let line = index + 1;
    let old = text.replace(after, before);
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{old}\n+{text}\n"
    );
    predicate_class(lib, &patch)
}

const FF16: &str = "        assert_eq!(chunk_size(&[b'f'; 16]), Ok(u64::MAX));";
const FF17: &str = "        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));";

#[test]
fn asserting_both_adjacent_lengths_exposes_the_counter_boundary() -> Result<(), String> {
    let assertions = format!("{FF16}\n{FF17}");
    assert_eq!(
        class_for("16 < count", &assertions)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn one_asserted_side_or_equal_outcomes_are_not_a_pin() -> Result<(), String> {
    for (label, assertions) in [
        (
            "only the 17-byte call asserted",
            format!("        assert!(chunk_size(&[b'f'; 16]).is_ok());\n{FF17}"),
        ),
        (
            "only the 16-byte call asserted",
            format!("{FF16}\n        let _ = chunk_size(&[b'f'; 17]);"),
        ),
        (
            "BadDigit at both lengths (the loop exits at the first element)",
            "        assert_eq!(chunk_size(&[b'z'; 16]), Err(ChunkError::BadDigit));
        assert_eq!(chunk_size(&[b'z'; 17]), Err(ChunkError::BadDigit));"
                .to_string(),
        ),
        (
            "different input content at 16 and 17 (count >= 16 passes both)",
            "        assert_eq!(chunk_size(&[b'z'; 16]), Err(ChunkError::BadDigit));
        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));"
                .to_string(),
        ),
        (
            "off-boundary lengths 15 and 17",
            "        assert_eq!(chunk_size(&[b'f'; 15]), Ok(u64::MAX >> 4));
        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));"
                .to_string(),
        ),
    ] {
        assert_ne!(
            class_for("16 < count", &assertions)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    Ok(())
}

/// The same owner without a type definition in its file, so the reference
/// count is established: `owner_reference_count` declines any file that
/// defines an enum, struct or impl.
const OWNER_PLAIN: &str = "pub fn chunk_size(hex: &[u8]) -> Result<u64, &'static str> {
    let mut size: u64 = 0;
    let mut count = 0;
    for &b in hex {
        count += 1;
        if 16 < count {
            return Err(\"too long\");
        }
        let digit = (b as char).to_digit(16).ok_or(\"bad digit\")?;
        size = (size << 4) | u64::from(digit);
    }
    Ok(size)
}
";

fn plain_class(assertions: &str, extra_reference: &str) -> Result<ExposureClass, String> {
    let lib = format!(
        "{OWNER_PLAIN}{extra_reference}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn chunk_sizes() {{\n{assertions}\n    }}\n}}\n"
    );
    rewritten_predicate_class(&lib, "count > 16", "16 < count")
}

const PLAIN_PINS: &str = "        assert_eq!(chunk_size(&[b'f'; 16]), Ok(u64::MAX));
        assert_eq!(chunk_size(&[b'f'; 17]), Err(\"too long\"));";

#[test]
fn an_established_reference_count_still_credits_the_pinned_boundary() -> Result<(), String> {
    assert_eq!(plain_class(PLAIN_PINS, "")?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_established_count_that_disagrees_with_the_admitted_calls_refuses() -> Result<(), String> {
    // A function-pointer use is a reference the two admitted calls do not cover.
    let alias = "\npub const SIZER: fn(&[u8]) -> Result<u64, &'static str> = chunk_size;\n";
    assert_ne!(plain_class(PLAIN_PINS, alias)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_later_fallback_return_keeps_the_boundary_below_exposed() -> Result<(), String> {
    let honest = source("16 < count", &format!("{FF16}\n{FF17}"));
    for masked in [
        honest.replace(
            "        let digit",
            "        if count == 17 { return Err(ChunkError::TooLong); }\n        let digit",
        ),
        honest.replace(
            "    Ok(size)",
            "    if count == 17 { Err(ChunkError::TooLong) } else { Ok(size) }",
        ),
        honest.replace("ChunkError::BadDigit)?", "ChunkError::TooLong)?"),
        honest
            .replace(
                "        let digit",
                "        let fallback = ChunkError::TooLong;\n        let digit",
            )
            .replace("ChunkError::BadDigit)?", "fallback)?"),
        format!(
            "{}\nenum OtherError {{ BadDigit }}\nimpl From<OtherError> for ChunkError {{ fn from(_: OtherError) -> Self {{ Self::TooLong }} }}\n",
            honest.replace("ChunkError::BadDigit)?", "OtherError::BadDigit)?")
        ),
    ] {
        assert_ne!(masked, honest, "premise: masking mutation applied");
        assert_ne!(predicate_class(&masked, &diff())?, ExposureClass::Exposed);
    }
    assert_eq!(
        class_for("16 < count", &format!("{FF16}\n{FF17}"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_shadowed_success_constructor_keeps_the_boundary_below_exposed() -> Result<(), String> {
    let assertions = "        assert_eq!(chunk_size(&[b'f'; 16]), Ok(16));\n        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));";
    let lib = format!(
        "{}\nfn Ok(n: u64) -> Result<u64, ChunkError> {{ if n == 17 {{ Err(ChunkError::TooLong) }} else {{ std::result::Result::Ok(n) }} }}\n",
        source("count >= 17", assertions)
            .replace("    Ok(size)", "    Ok(count)")
            .replace(
                "    use super::*;",
                "    use super::*;\n    use std::result::Result::{Ok, Err};"
            )
    );
    assert_ne!(
        rewritten_predicate_class(&lib, "count > 17", "count >= 17")?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// A derive expands in the owner's module and can supply an otherwise invisible
/// Result constructor. The local proc-macro dependency is real Rust source;
/// native paired witnesses are retained separately from this analyzer oracle.
#[test]
fn a_procedural_derive_cannot_supply_an_unchecked_result_constructor() -> Result<(), String> {
    let assertions = "        assert_eq!(chunk_size(&[b'f'; 16]), Ok(16));\n        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));";
    let base = source("count >= 17", assertions)
        .replace("    Ok(size)", "    Ok(count)")
        .replace(
            "    use super::*;",
            "    use super::*;\n    use std::result::Result::{Ok, Err};",
        );
    for producer in [
        "#[derive(ctor_macros::InjectOk)] struct Generated;",
        "use ctor_macros::InjectOk as Debug; #[derive(Debug)] struct Generated;",
    ] {
        // Avoid a conflicting alias on the error enum's builtin Debug derive.
        let lib = format!(
            "{}\n{producer}\n",
            base.replace("#[derive(Debug, PartialEq)]", "#[derive(PartialEq)]")
        );
        // Standard Debug on the error is retained by an explicit implementation.
        let lib = format!(
            "{lib}\nimpl std::fmt::Debug for ChunkError {{ fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{ f.write_str(\"ChunkError\") }} }}\n"
        );
        let line = lib
            .lines()
            .position(|line| line.contains("if count >= 17"))
            .ok_or("missing predicate")?
            + 1;
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-        if count > 17 {{\n+        if count >= 17 {{\n"
        );
        let repo = TempRepo::create(&lib, &patch)?;
        std::fs::create_dir_all(repo.root.join("ctor-macros/src")).map_err(|e| e.to_string())?;
        std::fs::write(repo.root.join("ctor-macros/Cargo.toml"), "[package]\nname='ctor-macros'\nversion='0.0.0'\nedition='2024'\n[lib]\nproc-macro=true\n").map_err(|e| e.to_string())?;
        std::fs::write(repo.root.join("ctor-macros/src/lib.rs"), "extern crate proc_macro;\n#[proc_macro_derive(InjectOk)]\npub fn inject_ok(_: proc_macro::TokenStream) -> proc_macro::TokenStream { \"fn Ok(n:u64)->Result<u64,ChunkError>{if n==17{Err(ChunkError::TooLong)}else{std::result::Result::Ok(n)}}\".parse().unwrap_or_default() }\n").map_err(|e| e.to_string())?;
        std::fs::write(repo.root.join("Cargo.toml"), "[package]\nname='slice-counter'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nctor-macros={path='ctor-macros'}\n[workspace]\n").map_err(|e| e.to_string())?;
        let findings = repo.findings()?;
        let predicates = findings
            .iter()
            .filter(|finding| finding.probe.family == ProbeFamily::Predicate)
            .collect::<Vec<_>>();
        assert_eq!(predicates.len(), 1, "{findings:?}");
        assert_ne!(predicates[0].class, ExposureClass::Exposed, "{findings:?}");
    }
    assert_eq!(
        class_for("16 < count", &format!("{FF16}\n{FF17}"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn early_error_equality_cannot_pin_the_counter_boundary() -> Result<(), String> {
    let equality =
        "\nimpl PartialEq for ChunkError { fn eq(&self, _other: &Self) -> bool { true } }\n";
    for (byte, rebound, expected) in [
        ("b'z'", false, ExposureClass::InfectionUnknown),
        ("b'f'", true, ExposureClass::InfectionUnknown),
        ("b'f'", false, ExposureClass::Exposed),
    ] {
        let short = if byte == "b'z'" || rebound {
            "Err(ChunkError::BadDigit)"
        } else {
            "Ok(u64::MAX)"
        };
        let assertions = format!(
            "        assert_eq!(chunk_size(&[{byte}; 16]), {short});\n        assert_eq!(chunk_size(&[{byte}; 17]), Err(ChunkError::TooLong));"
        );
        let mut lib = source("count >= 17", &assertions)
            .replace("#[derive(Debug, PartialEq)]", "#[derive(Debug)]");
        lib.push_str(equality);
        if rebound {
            lib = lib.replace(
                "        let digit",
                "        let b = b'z';\n        let digit",
            );
        }
        assert_eq!(
            rewritten_predicate_class(&lib, "count > 17", "count >= 17")?,
            expected,
            "byte={byte}, rebound={rebound}"
        );
    }
    Ok(())
}

#[test]
fn counter_mutations_cannot_supply_the_length_boundary() -> Result<(), String> {
    for statement in [
        "(count,) = (count + count / 16,);",
        "(count) = count + count / 16;",
        "[count] = [count + count / 16];",
        "count.clone_from(&(count + count / 16));",
        "let ref mut alias = count; *alias += *alias / 16;",
        "let alias = &mut (count); *alias += *alias / 16;",
    ] {
        let lib = source("count >= 17", &format!("{FF16}\n{FF17}")).replace(
            "        let digit",
            &format!("        {statement}\n        let digit"),
        );
        assert_ne!(
            rewritten_predicate_class(&lib, "count > 17", "count >= 17")?,
            ExposureClass::Exposed,
            "{statement}"
        );
    }
    assert_eq!(
        class_for("16 < count", &format!("{FF16}\n{FF17}"))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn counter_width_must_represent_the_predicate_and_every_input() -> Result<(), String> {
    for (ty, short, long, expected) in [
        ("i8", 127, 128, ExposureClass::InfectionUnknown),
        ("u8", 255, 256, ExposureClass::InfectionUnknown),
        ("i8", 126, 127, ExposureClass::Exposed),
        ("u8", 254, 255, ExposureClass::Exposed),
        ("usize", 255, 256, ExposureClass::Exposed),
    ] {
        let overflowing = expected != ExposureClass::Exposed;
        let short_result = if overflowing {
            "Err(ChunkError::BadDigit)"
        } else {
            "Ok(u64::MAX)"
        };
        let assertions = format!(
            "        assert_eq!(chunk_size(&[b'f'; {short}]), {short_result});\n        assert_eq!(chunk_size(&[b'f'; {long}]), Err(ChunkError::TooLong));"
        );
        let mut lib = source(&format!("count >= {long}"), &assertions)
            .replace("let mut count = 0", &format!("let mut count: {ty} = 0"));
        if overflowing {
            lib = format!(
                "#![allow(overflowing_literals)]\n{}",
                lib.replace("#[derive(Debug, PartialEq)]", "#[derive(Debug)]")
            );
            lib.push_str(
                "\nimpl PartialEq for ChunkError { fn eq(&self, _: &Self) -> bool { true } }\n",
            );
        }
        assert_eq!(
            rewritten_predicate_class(
                &lib,
                &format!("count > {long}"),
                &format!("count >= {long}")
            )?,
            expected,
            "type={ty}, length={long}"
        );
    }
    Ok(())
}
