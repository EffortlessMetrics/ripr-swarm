//! Public controls for #3161's return-snapshot subject admission.
//! Native Insta oracle outcomes remain in the independently retained audit.

#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, OutputFormat, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub fn make_count(count: usize) -> usize {\n    count + 0\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    use expect_test::expect;\n    #[test]\n    fn snapshot_count() {\n        __BODY__\n    }\n}\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new(body: &str) -> Result<Self, String> {
        Self::with_source(SOURCE, body)
    }

    fn with_source(source: &str, body: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-return-snapshot-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir_all(fixture.0.join("src/snapshots"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname=\"return_snapshot_admission\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n[dev-dependencies]\ninsta={version=\"=1.49.0\",features=[\"json\"]}\nexpect-test=\"1\"\n",
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(source.matches("__BODY__").count(), 1);
        std::fs::write(
            fixture.0.join("src/lib.rs"),
            source.replace("__BODY__", body),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture
                .0
                .join("src/snapshots/return_snapshot_admission__tests__snapshot_count.snap"),
            "---\nsource: src/lib.rs\nexpression: actual\n---\n2\n",
        )
        .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn seam(&self) -> Result<Value, String> {
        let output = ripr::app::repo_seam_inventory_input(CheckInput {
            root: self.0.clone(),
            ..CheckInput::default()
        });
        let report: Value =
            serde_json::from_str(&render_check(&output, &OutputFormat::RepoExposureJson)?)
                .map_err(|error| error.to_string())?;
        assert_eq!(report["run_status"], "complete");
        let matching: Vec<_> = report["seams"]
            .as_array()
            .ok_or("missing seams")?
            .iter()
            .filter(|seam| {
                seam["kind"] == "return_value" && seam["owner"] == "src/lib.rs::make_count"
            })
            .collect();
        assert_eq!(matching.len(), 1, "inspect the actual owner's return");
        Ok(matching[0].clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn weak(body: &str) -> Result<(), String> {
    assert_weak(Fixture::new(body)?.seam()?)
}

fn assert_weak(seam: Value) -> Result<(), String> {
    assert_eq!(seam["evidence"]["reach"], "yes");
    assert_eq!(seam["evidence"]["activate"], "yes");
    assert_eq!(
        seam["evidence"]["observe"], "yes",
        "retain raw snapshot evidence"
    );
    assert_eq!(seam["grip_class"], "weakly_gripped");
    assert_eq!(seam["evidence"]["discriminate"], "weak");
    assert_ne!(
        seam["evidence_record"]["canonical_item"]["gap_state"],
        "already_observed"
    );
    assert_ne!(
        seam["evidence_record"]["canonical_item"]["recommended_repair"],
        "No new RIPR action; current static evidence already observes this seam."
    );
    Ok(())
}

fn strong(body: &str) -> Result<(), String> {
    let seam = Fixture::new(body)?.seam()?;
    assert_eq!(seam["grip_class"], "strongly_gripped");
    assert_eq!(seam["evidence"]["discriminate"], "yes");
    Ok(())
}

#[test]
fn unrelated_snapshot_cannot_close_the_owner_return() -> Result<(), String> {
    weak(
        "let actual = make_count(2);\n        let other = 2usize;\n        insta::assert_json_snapshot!(other);",
    )
}

#[test]
fn direct_owner_result_snapshots_keep_their_grade() -> Result<(), String> {
    for assertion in [
        "insta::assert_json_snapshot!(actual);",
        "insta::assert_snapshot!(actual, @\"2\");",
        "insta::assert_debug_snapshot!(\"named\", &actual);",
        "insta::assert_debug_snapshot!(\"count,snapshot\", &actual);",
        "insta::assert_debug_snapshot!(r#\"count, (snapshot)\"#, &actual,);",
        "insta::assert_json_snapshot!(make_count(2));",
        "insta::assert_json_snapshot!(crate::make_count(2));",
    ] {
        strong(&format!("let actual = make_count(2);\n        {assertion}"))?;
    }
    strong("let actual = make_count(2); insta::assert_json_snapshot!(actual);")?;
    strong("let actual=make_count(2);insta::assert_json_snapshot!(actual);")?;
    strong(
        "use insta::assert_snapshot;\n        let actual = make_count(2);\n        assert_snapshot!(actual, @\"2\");",
    )?;
    strong(
        "use insta::assert_snapshot;\n        let actual = make_count(2);\n        assert_snapshot!(make_count(2), @\"2\");",
    )?;
    strong(
        "use insta::{assert_snapshot};\n        let actual = make_count(2);\n        assert_snapshot!(make_count(2), @\"2\");",
    )?;
    strong("let actual = crate::make_count(2);\n        insta::assert_json_snapshot!(actual);")
}

#[test]
fn independent_literal_and_expect_oracles_keep_their_grade() -> Result<(), String> {
    for assertion in [
        "assert_eq!(actual, 2);",
        "expect![[\"2\"]].assert_eq(&actual.to_string());",
    ] {
        strong(&format!(
            "let actual = make_count(2);\n        let other = 2usize;\n        insta::assert_json_snapshot!(other);\n        {assertion}"
        ))?;
    }
    Ok(())
}

#[test]
fn snapshot_names_and_expected_text_cannot_supply_the_subject() -> Result<(), String> {
    for assertion in [
        "insta::assert_json_snapshot!(\"actual\", other);",
        "insta::assert_snapshot!(other, @\"actual make_count(2)\");",
        "insta::assert_json_snapshot!((actual, other).1);",
        "insta::assert_json_snapshot!({ let actual = other; actual });",
    ] {
        weak(&format!(
            "let actual = make_count(2);\n        let other = 2usize;\n        {assertion}"
        ))?;
    }
    Ok(())
}

#[test]
fn changed_or_escaped_result_bindings_cannot_supply_snapshot_credit() -> Result<(), String> {
    for body in [
        "let actual=2usize;insta::assert_json_snapshot!(actual);let actual=make_count(2);",
        "let actual = make_count(2);\n        let actual = 2usize;\n        insta::assert_json_snapshot!(actual);",
        "let mut actual = make_count(2);\n        actual = 2;\n        insta::assert_json_snapshot!(actual);",
        "let mut actual = make_count(2);\n        let _ = &mut actual;\n        insta::assert_json_snapshot!(actual);",
        "let actual = make_count(2);\n        inspect(&actual);\n        insta::assert_json_snapshot!(actual);",
        "let actual = make_count(2);\n        rewrite!();\n        insta::assert_json_snapshot!(actual);",
    ] {
        weak(body)?;
    }
    Ok(())
}

#[test]
fn macro_operand_control_flow_cannot_supply_snapshot_credit() -> Result<(), String> {
    strong(
        "let _actual = make_count(2);\n        insta::assert_snapshot!(make_count({ 2 }), @\"2\");",
    )?;
    for operand in [
        "make_count({ return; 2 })",
        "crate::make_count({ return; 2 })",
        "make_count(rewrite!())",
    ] {
        weak(&format!(
            "let _actual = make_count(2);\n        insta::assert_snapshot!({operand}, @\"2\");"
        ))?;
    }
    Ok(())
}

#[test]
fn immutable_alias_cannot_hide_interior_result_mutation() -> Result<(), String> {
    let source = SOURCE.replace(
        "-> usize {\n    count + 0",
        "-> std::cell::Cell<usize> {\n    std::cell::Cell::new(count + 0)",
    );
    assert_weak(Fixture::with_source(&source, "let actual = make_count(2);\n        let alias = &actual;\n        alias.set(2);\n        insta::assert_compact_debug_snapshot!(actual, @\"Cell { value: 2 }\");")?.seam()?)
}

#[test]
fn inline_string_commas_keep_direct_snapshot_credit() -> Result<(), String> {
    let source = SOURCE.replace(
        "-> usize {\n    count + 0",
        "-> String {\n    (count + 0).to_string() + \",done\"",
    );
    for assertion in [
        "insta::assert_snapshot!(actual, @\"2,done\");",
        "insta::assert_snapshot!(actual, @r#\"2,done\"#);",
    ] {
        let seam = Fixture::with_source(
            &source,
            &format!("let actual = make_count(2);\n        {assertion}"),
        )?
        .seam()?;
        assert_eq!(seam["grip_class"], "strongly_gripped");
        assert_eq!(seam["evidence"]["discriminate"], "yes");
    }
    Ok(())
}

#[test]
fn local_snapshot_lookalikes_cannot_borrow_the_operand_exception() -> Result<(), String> {
    weak(
        "use opaque_snapshots as insta;\n        use insta::assert_snapshot;\n        let actual = make_count(2);\n        assert_snapshot!(make_count(2), @\"2\");",
    )?;
    weak(
        "macro_rules! define_snapshot { () => { macro_rules! assert_snapshot { ($ignored:expr, @$expected:literal) => {}; } }; }\n        define_snapshot!();\n        let actual = make_count(2);\n        assert_snapshot!(make_count(2), @\"2\");",
    )?;
    weak(
        "macro_rules! assert_snapshot { ($($tt:tt)*) => {} }\n        let actual = make_count(2);\n        assert_snapshot!(actual);",
    )?;
    weak(
        "macro_rules! ignored { ($($tt:tt)*) => {} }\n        use ignored as assert_snapshot;\n        let actual = make_count(2);\n        assert_snapshot!(make_count(2), @\"2\");",
    )?;
    weak(
        "macro_rules! ignored { ($($tt:tt)*) => {} }\n        mod insta { pub(crate) use ignored as assert_snapshot; }\n        let actual = make_count(2);\n        insta::assert_snapshot!(make_count(2), @\"2\");",
    )
}

#[test]
fn snapshot_imports_cannot_borrow_another_modules_same_line() -> Result<(), String> {
    for import in [
        "use opaque_snapshots::assert_snapshot;",
        "use opaque_snapshots::assert_snapshot as r#assert_snapshot;",
    ] {
        let source = SOURCE.replace("mod tests {\n    use super::*;", &format!("mod unused {{ use insta::assert_snapshot; }} mod tests {{ {import}\n    use super::*;"));
        assert_weak(
            Fixture::with_source(
                &source,
                "let actual = make_count(2);\n        assert_snapshot!(make_count(2), @\"2\");",
            )?
            .seam()?,
        )?;
    }
    Ok(())
}

#[test]
fn snapshot_dependency_substitution_cannot_supply_eager_oracle_credit() -> Result<(), String> {
    let fixture = Fixture::new(
        "let actual = make_count(2);\n        insta::assert_snapshot!(make_count(2), @\"2\");",
    )?;
    let manifest = fixture.0.join("Cargo.toml");
    let contents = std::fs::read_to_string(&manifest).map_err(|error| error.to_string())?;
    let wrong = contents.replace(
        "insta={version=\"=1.49.0\",features=[\"json\"]}",
        "insta={package=\"opaque_snapshots\",path=\"../opaque-snapshot-dependency\"}",
    );
    assert_ne!(contents, wrong);
    std::fs::write(manifest, wrong).map_err(|error| error.to_string())?;
    assert_weak(fixture.seam()?)
}
