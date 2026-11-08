// #13602: the return-value stub that `ripr agent stub` writes asserts on the
// owner call inline, so once its placeholders are filled the next `check`
// confirms the assertion observes the changed value instead of reporting
// `miss=observation_unconfirmed` on the stub's own test.
use super::{run_ripr, unique_temp_workspace};

const BEFORE: &str = "pub fn find_sku<'a>(skus: &'a [&'a str], name: &str) -> Option<&'a str> {\n    skus.iter().copied().find(|s| s.eq_ignore_ascii_case(name))\n}\n";

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn find_sku<'a>(skus: &'a [&'a str], name: &str) -> Option<&'a str> {\n-    skus.iter().copied().find(|s| s.eq_ignore_ascii_case(name))\n+    skus.iter().copied().find(|s| s.eq(&name))\n }\n";

#[test]
fn a_filled_return_value_stub_is_credited_with_observing_the_value() -> Result<(), String> {
    let root = unique_temp_workspace("stub-closes-own-gap");
    let result = (|| {
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"stub_closes_own_gap\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .map_err(|error| error.to_string())?;
        let after = BEFORE.replace("s.eq_ignore_ascii_case(name)", "s.eq(&name)");
        std::fs::write(root.join("src/lib.rs"), &after).map_err(|error| error.to_string())?;
        let patch = root.join("change.patch");
        std::fs::write(&patch, PATCH).map_err(|error| error.to_string())?;
        let root_arg = root.to_string_lossy().into_owned();
        let patch_arg = patch.to_string_lossy().into_owned();

        let stub = run_ripr(&[
            "agent",
            "stub",
            "--root",
            &root_arg,
            "--at",
            "src/lib.rs:2",
            "--kind",
            "return_value",
            "--write",
        ]);
        let stub_err = String::from_utf8_lossy(&stub.stderr).into_owned();
        assert!(stub.status.success(), "stub --write failed: {stub_err}");
        let written =
            std::fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
        assert!(
            written.contains("assert_eq!(find_sku(skus, name), expected);"),
            "the stub must assert on the owner call inline: {written}"
        );
        assert!(!written.contains("let actual"), "{written}");

        // Fill the placeholders the way an agent would.
        let filled = fill_placeholders(&written)?;
        std::fs::write(root.join("src/lib.rs"), &filled).map_err(|error| error.to_string())?;

        let check = run_ripr(&[
            "check", "--root", &root_arg, "--diff", &patch_arg, "--quiet", "--format", "agent",
        ]);
        let out = String::from_utf8_lossy(&check.stdout).into_owned();
        let err = String::from_utf8_lossy(&check.stderr).into_owned();
        assert!(
            out.contains("find_sku_return_value_discriminator"),
            "the filled stub must be the related test: {out}\n{err}"
        );
        assert!(
            !out.contains("miss=observation_unconfirmed"),
            "the stub's own assertion must be credited: {out}"
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&root);
    result
}

/// Replace each `todo!(...)` placeholder in declaration order with a value that
/// pins the case-insensitive behavior, and drop any lint allowance that only
/// exists while the placeholders diverge.
fn fill_placeholders(written: &str) -> Result<String, String> {
    let values = [
        "&[\"Widget\", \"Gadget\"]",
        "\"widget\"",
        "Some(\"Widget\")",
    ];
    let mut out = String::new();
    let mut filled = 0;
    for line in written.lines() {
        if line.trim_start().starts_with("#[allow(unreachable_code)]") {
            continue;
        }
        let mut line = line.to_string();
        if let Some(start) = line.find("todo!(") {
            let end = line[start..]
                .find(");")
                .map(|offset| start + offset + 1)
                .ok_or_else(|| format!("unterminated placeholder: {line}"))?;
            let value = values
                .get(filled)
                .ok_or_else(|| format!("more placeholders than expected: {written}"))?;
            line.replace_range(start..end, value);
            filled += 1;
        }
        out.push_str(&line);
        out.push('\n');
    }
    if filled != values.len() {
        return Err(format!(
            "expected {} placeholders, filled {filled}: {written}",
            values.len()
        ));
    }
    Ok(out)
}
