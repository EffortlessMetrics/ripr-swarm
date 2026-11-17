//! Public-API controls for a prefix `-`/`!` deleted from or added to a
//! returned name (#14815, cargo-mutants' UnaryOperator genre).
//!
//! When the owner's whole body is the one-line tail `a` (was `-a` or `!a`),
//! the change reaches the owner's return. An exact pin at an input where the
//! two versions differ reads `exposed` and a bare call reads
//! `reachable_unrevealed`. `-a` and `a` agree at `0`, so an assertion at `0`,
//! or at an argument ripr cannot fold to a literal (a local, a loop row, a
//! constant), is never `exposed`. Neither is a diff whose old owner held more
//! than the replaced line.

use ripr::domain::StageState;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` holding `owner` and one test whose body is `test_body`.
fn source(owner: &str, test_body: &str) -> String {
    format!(
        "{owner}

#[cfg(test)]
mod tests {{
    use super::*;

    const ZERO: i32 = 0;

    #[test]
    fn checks_owner() {{
        {test_body}
    }}
}}
"
    )
}

/// The owner `fn f(a: TY) -> TY { TAIL }` on lines 1-3.
fn owner(ty: &str, tail: &str) -> String {
    format!("pub fn f(a: {ty}) -> {ty} {{\n    {tail}\n}}")
}

/// A diff replacing the tail line 2 of [`owner`].
fn tail_diff(ty: &str, removed: &str, added: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn f(a: {ty}) -> {ty} {{
-    {removed}
+    {added}
 }}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        Self::create_files(&[("src/lib.rs", lib)], patch)
    }

    /// A crate holding each `(path, text)` file, changed by `patch`.
    fn create_files(files: &[(&str, &str)], patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unary-tail-return-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"unary-tail\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        for (path, text) in files {
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))?;
        }
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

/// The one return-value finding for the changed tail `tail` on `line`.
fn tail_finding(findings: &[Finding], line: usize, tail: &str) -> Result<Finding, String> {
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.line == line
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` on line {line}, got {other:?} in {findings:?}"
        )),
    }
}

/// The class of the changed tail's finding when the owner's tail changes from
/// `removed` to `added` and the test body is `test_body`.
fn tail_class(
    ty: &str,
    removed: &str,
    added: &str,
    test_body: &str,
) -> Result<(ExposureClass, Finding), String> {
    let findings = TempRepo::create(
        &source(&owner(ty, added), test_body),
        &tail_diff(ty, removed, added),
    )?
    .findings()?;
    let finding = tail_finding(&findings, 2, added)?;
    Ok((finding.class.clone(), finding))
}

#[test]
fn an_exact_pin_where_the_versions_differ_exposes_a_deleted_or_added_minus() -> Result<(), String> {
    for (removed, added, test_body) in [
        ("-a", "a", "assert_eq!(f(5), 5);"),
        ("a", "-a", "assert_eq!(f(5), -5);"),
    ] {
        let (class, finding) = tail_class("i32", removed, added, test_body)?;
        assert_eq!(
            class,
            ExposureClass::Exposed,
            "{removed} -> {added}: {finding:?}"
        );
        assert!(
            finding
                .flow_sinks
                .iter()
                .any(|sink| sink.kind == ripr::domain::FlowSinkKind::ReturnValue
                    && sink.text == added),
            "the sink is the owner's return, named by the raw tail: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn an_exact_pin_exposes_a_deleted_or_added_negation() -> Result<(), String> {
    for (removed, added, test_body) in [
        ("!a", "a", "assert_eq!(f(true), true);"),
        ("a", "!a", "assert_eq!(f(true), false);"),
        // `!a` differs from `a` at every input, so even an argument ripr
        // cannot fold reaches the change: the old version returns `false`.
        ("!a", "a", "let d = true;\n        assert!(f(d));"),
    ] {
        let (class, finding) = tail_class("bool", removed, added, test_body)?;
        assert_eq!(
            class,
            ExposureClass::Exposed,
            "{removed} -> {added} / {test_body}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn a_bare_call_reads_reachable_unrevealed() -> Result<(), String> {
    for (ty, removed, added, test_body) in [
        ("i32", "-a", "a", "let _ = f(5);"),
        ("bool", "!a", "a", "let _ = f(true);"),
    ] {
        let (class, finding) = tail_class(ty, removed, added, test_body)?;
        assert_eq!(
            class,
            ExposureClass::ReachableUnrevealed,
            "{removed} -> {added}: {finding:?}"
        );
    }
    Ok(())
}

/// `-a` and `a` are equal at `0`: a pin there cannot tell them apart.
#[test]
fn a_pin_where_minus_and_plain_agree_is_weakly_exposed() -> Result<(), String> {
    for (removed, added) in [("-a", "a"), ("a", "-a")] {
        let (class, finding) = tail_class("i32", removed, added, "assert_eq!(f(0), 0);")?;
        assert_eq!(
            class,
            ExposureClass::WeaklyExposed,
            "{removed} -> {added}: {finding:?}"
        );
    }
    Ok(())
}

/// The fold reads only literal arguments, so a local, a loop row or a
/// constant holding `0` must not let the pin read as separating.
#[test]
fn an_unfolded_argument_holding_zero_is_not_exposed() -> Result<(), String> {
    for test_body in [
        "let d = 0;\n        assert_eq!(f(d), 0);",
        "for (x, want) in [(0, 0)] {\n            assert_eq!(f(x), want);\n        }",
        "assert_eq!(f(ZERO), 0);",
    ] {
        let (class, finding) = tail_class("i32", "-a", "a", test_body)?;
        assert!(
            !matches!(class, ExposureClass::Exposed),
            "{test_body}: {finding:?}"
        );
    }
    Ok(())
}

/// The removed `-a` is not the whole old owner: the old owner also returned
/// early at `a == 5`, so `f(5) == 5` holds on both versions. Pairing the
/// removed tail with the added one must not stand in for the old function.
#[test]
fn an_old_owner_with_more_than_the_replaced_line_is_not_exposed() -> Result<(), String> {
    let lib = source(
        "pub fn f(a: i32) -> i32 {\n    // c\n    a\n}",
        "assert_eq!(f(5), 5);",
    );
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
 pub fn f(a: i32) -> i32 {
-    if a == 5 { return 5; }
-    -a
+    // c
+    a
 }
";
    let findings = TempRepo::create(&lib, patch)?.findings()?;
    let finding = tail_finding(&findings, 3, "a")?;
    assert!(
        !matches!(finding.class, ExposureClass::Exposed),
        "{finding:?}"
    );
    // The early return deleted in the same block as the replaced tail: the
    // block is uneven, so nothing pairs with `a`.
    let lib = source(
        "pub fn f(a: i32) -> i32 {\n    a\n}",
        "assert_eq!(f(5), 5);",
    );
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,3 @@
 pub fn f(a: i32) -> i32 {
-    if a == 5 { return 5; }
-    -a
+    a
 }
";
    let findings = TempRepo::create(&lib, patch)?.findings()?;
    let finding = tail_finding(&findings, 2, "a")?;
    assert!(
        !matches!(finding.class, ExposureClass::Exposed),
        "{finding:?}"
    );
    // The early return deleted in its own block, apart from an even `-a` to
    // `a` replacement: the tail pairs with `-a` exactly, and only the other
    // removed line in the owner shows the old owner held more.
    let lib = source(
        "pub fn f(a: i32) -> i32 {\n    // c\n    a\n}",
        "assert_eq!(f(5), 5);",
    );
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,4 @@
 pub fn f(a: i32) -> i32 {
-    if a == 5 { return 5; }
     // c
-    -a
+    a
 }
";
    let findings = TempRepo::create(&lib, patch)?.findings()?;
    let finding = tail_finding(&findings, 3, "a")?;
    assert!(
        !matches!(finding.class, ExposureClass::Exposed),
        "{finding:?}"
    );
    Ok(())
}

/// A float's `-0.0 == 0.0` and `NaN` are outside the integer fold, so a
/// float tail keeps the ordinary reading and is never credited here.
#[test]
fn a_float_tail_is_not_exposed() -> Result<(), String> {
    let (class, finding) = tail_class("f64", "-a", "a", "assert_eq!(f(5.0), 5.0);")?;
    assert!(!matches!(class, ExposureClass::Exposed), "{finding:?}");
    Ok(())
}

/// The `return_value` finding for the tail `x` on `line` of a whole-file
/// `lib` changed by `patch`, which must not read `exposed`.
fn assert_tail_not_exposed(lib: &str, patch: &str, line: usize) -> Result<(), String> {
    let findings = TempRepo::create(lib, patch)?.findings()?;
    let finding = tail_finding(&findings, line, "x")?;
    assert!(
        !matches!(finding.class, ExposureClass::Exposed),
        "{finding:?}"
    );
    Ok(())
}

/// #14815 review F1: swapping two modules that each hold a `fn f` pairs the
/// unchanged header across them, so the removed `-x` inside the current
/// `b::f` span came from `a::f`. `b::f` returned `x` on both sides.
#[test]
fn a_same_name_function_swapped_between_modules_is_not_exposed() -> Result<(), String> {
    let lib = "pub mod b {
    pub fn f(x: i32) -> i32 {
        x
    }
}

pub mod a {
    pub fn f(x: i32) -> i32 {
        -x
    }
}

#[cfg(test)]
mod tests_b {
    use crate::b::f;
    #[test]
    fn pins_b() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,12 +1,12 @@
-pub mod a {
+pub mod b {
     pub fn f(x: i32) -> i32 {
-        -x
+        x
     }
 }
 
-pub mod b {
+pub mod a {
     pub fn f(x: i32) -> i32 {
-        x
+        -x
     }
 }
 
";
    assert_tail_not_exposed(lib, patch, 3)
}

/// Swapping which of two same-name functions is compiled (`#[cfg]` moved
/// between them) leaves the compiled `f` returning `x` on both sides.
#[test]
fn a_cfg_swapped_same_name_function_is_not_exposed() -> Result<(), String> {
    let lib = "#[cfg(not(any()))]
pub fn f(x: i32) -> i32 {
    x
}

#[cfg(any())]
pub fn f(x: i32) -> i32 {
    -x
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,11 +1,11 @@
-#[cfg(any())]
+#[cfg(not(any()))]
 pub fn f(x: i32) -> i32 {
-    -x
+    x
 }
 
-#[cfg(not(any()))]
+#[cfg(any())]
 pub fn f(x: i32) -> i32 {
-    x
+    -x
 }
 
";
    assert_tail_not_exposed(lib, patch, 3)
}

/// The same swap through the enclosing module's `#[cfg]`.
#[test]
fn an_ancestor_cfg_swap_is_not_exposed() -> Result<(), String> {
    let lib = "#[cfg(not(any()))]
pub mod m {
    pub fn f(x: i32) -> i32 {
        x
    }
}

#[cfg(any())]
pub mod m {
    pub fn f(x: i32) -> i32 {
        -x
    }
}

#[cfg(test)]
mod tests {
    use crate::m::f;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,14 +1,14 @@
-#[cfg(any())]
+#[cfg(not(any()))]
 pub mod m {
     pub fn f(x: i32) -> i32 {
-        -x
+        x
     }
 }
 
-#[cfg(not(any()))]
+#[cfg(any())]
 pub mod m {
     pub fn f(x: i32) -> i32 {
-        x
+        -x
     }
 }
 
";
    assert_tail_not_exposed(lib, patch, 4)
}

/// The changed `f` is compiled only outside tests; the tests call a
/// `#[cfg(test)]` twin that returns `x` on both sides.
#[test]
fn a_cfg_test_twin_is_not_exposed() -> Result<(), String> {
    let lib = "#[cfg(not(test))]
pub fn f(x: i32) -> i32 {
    x
}

#[cfg(test)]
pub fn f(x: i32) -> i32 {
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,6 +1,6 @@
 #[cfg(not(test))]
 pub fn f(x: i32) -> i32 {
-    -x
+    x
 }
 
 #[cfg(test)]
";
    assert_tail_not_exposed(lib, patch, 3)
}

/// The only `f` moved to another module: the test's path names a function
/// that did not exist on the base side, so the old body is not this owner's.
#[test]
fn a_renamed_enclosing_module_is_not_exposed() -> Result<(), String> {
    let lib = "pub mod b {
    pub fn f(x: i32) -> i32 {
        x
    }
}

#[cfg(test)]
mod tests {
    use crate::b::f;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,5 @@
-pub mod a {
+pub mod b {
     pub fn f(x: i32) -> i32 {
-        -x
+        x
     }
 }
";
    assert_tail_not_exposed(lib, patch, 3)
}

/// The `return_value` findings for each changed tail `(file, tail)` on `line`
/// of a multi-file crate, none of which may read `exposed` or
/// `weakly_exposed` or credit propagation to the return.
fn assert_file_tails_not_exposed(
    files: &[(&str, &str)],
    patch: &str,
    tails: &[(&str, &str)],
    line: usize,
) -> Result<(), String> {
    let findings = TempRepo::create_files(files, patch)?.findings()?;
    for (file, tail) in tails {
        let in_file: Vec<Finding> = findings
            .iter()
            .filter(|finding| finding.probe.location.file.ends_with(file))
            .cloned()
            .collect();
        let finding = tail_finding(&in_file, line, tail)?;
        // The test runs a different function than the changed one, so the
        // unary-tail reading must refuse it: no return-value sink credited
        // and no exposure credit. (Before the identity check read the module
        // declarations, these read `weakly_exposed` with the sink credited.)
        assert!(
            !matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ) && finding.ripr.propagate.state != StageState::Yes,
            "{file}: {finding:?}"
        );
    }
    Ok(())
}

/// #14852 review: the changed `f` lives in a module the parent declares only
/// outside tests; the tests call a `#[cfg(test)]` module's twin returning `x`
/// on both sides. The gate sits on the `mod` declaration in another file.
#[test]
fn a_cfg_gated_module_declaration_twin_is_not_exposed() -> Result<(), String> {
    let lib = "#[cfg(not(test))]
mod real;
#[cfg(not(test))]
pub use real::f;

#[cfg(test)]
mod fake;
#[cfg(test)]
pub use fake::f;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let real = "pub fn f(x: i32) -> i32 {\n    x\n}\n";
    let fake = "pub fn f(x: i32) -> i32 {\n    x\n}\n";
    let patch = "diff --git a/src/real.rs b/src/real.rs
index 1111111..2222222 100644
--- a/src/real.rs
+++ b/src/real.rs
@@ -1,3 +1,3 @@
 pub fn f(x: i32) -> i32 {
-    -x
+    x
 }
";
    assert_file_tails_not_exposed(
        &[
            ("src/lib.rs", lib),
            ("src/real.rs", real),
            ("src/fake.rs", fake),
        ],
        patch,
        &[("src/real.rs", "x")],
        2,
    )
}

/// #14852 review: swapping two `#[path]` module files moves each `f` under
/// the other module name, so `a::f` returns `-x` and `b::f` returns `x` on
/// both sides while each file's tail flips. The test pins `b::f`, which did
/// not change.
#[test]
fn a_swapped_module_path_attribute_is_not_exposed() -> Result<(), String> {
    let lib = "#[path = \"y.rs\"]
pub mod a;
#[path = \"x.rs\"]
pub mod b;

#[cfg(test)]
mod tests {
    use crate::b::f;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let x = "pub fn f(x: i32) -> i32 {\n    x\n}\n";
    let y = "pub fn f(x: i32) -> i32 {\n    -x\n}\n";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
-#[path = \"x.rs\"]
+#[path = \"y.rs\"]
 pub mod a;
-#[path = \"y.rs\"]
+#[path = \"x.rs\"]
 pub mod b;
diff --git a/src/x.rs b/src/x.rs
index 3333333..4444444 100644
--- a/src/x.rs
+++ b/src/x.rs
@@ -1,3 +1,3 @@
 pub fn f(x: i32) -> i32 {
-    -x
+    x
 }
diff --git a/src/y.rs b/src/y.rs
index 5555555..6666666 100644
--- a/src/y.rs
+++ b/src/y.rs
@@ -1,3 +1,3 @@
 pub fn f(x: i32) -> i32 {
-    x
+    -x
 }
";
    assert_file_tails_not_exposed(
        &[("src/lib.rs", lib), ("src/x.rs", x), ("src/y.rs", y)],
        patch,
        &[("src/x.rs", "x"), ("src/y.rs", "-x")],
        2,
    )
}

/// #14952 shape: an in-file `cfg` on the `pub use` that picks which module's
/// `f` the tests run. The changed `real::f` is compiled for tests but never
/// called by them; `fake::f` returns `x` on both sides.
#[test]
fn a_cfg_gated_reexport_twin_is_not_exposed() -> Result<(), String> {
    let lib = "pub mod real {
    pub fn f(x: i32) -> i32 {
        x
    }
}

pub mod fake {
    pub fn f(x: i32) -> i32 {
        x
    }
}

#[cfg(not(test))]
pub use real::f;
#[cfg(test)]
pub use fake::f;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,5 @@
 pub mod real {
     pub fn f(x: i32) -> i32 {
-        -x
+        x
     }
 }
";
    assert_file_tails_not_exposed(&[("src/lib.rs", lib)], patch, &[("src/lib.rs", "x")], 3)
}

/// #14952 shape: `include!` under `cfg` picks which file's `f` exists; the
/// tests run the `fake.rs` twin returning `x` on both sides.
#[test]
fn a_cfg_gated_include_twin_is_not_exposed() -> Result<(), String> {
    let lib = "#[cfg(not(test))]
include!(\"real.rs\");
#[cfg(test)]
include!(\"fake.rs\");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let real = "pub fn f(x: i32) -> i32 {\n    x\n}\n";
    let fake = "pub fn f(x: i32) -> i32 {\n    x\n}\n";
    let patch = "diff --git a/src/real.rs b/src/real.rs
index 1111111..2222222 100644
--- a/src/real.rs
+++ b/src/real.rs
@@ -1,3 +1,3 @@
 pub fn f(x: i32) -> i32 {
-    -x
+    x
 }
";
    assert_file_tails_not_exposed(
        &[
            ("src/lib.rs", lib),
            ("src/real.rs", real),
            ("src/fake.rs", fake),
        ],
        patch,
        &[("src/real.rs", "x")],
        2,
    )
}

/// #14952 shape: a `macro_rules!` emits the non-test `mod real;`, and an
/// inline `#[cfg(test)]` twin of the module returns `x` on both sides.
#[test]
fn a_macro_emitted_module_twin_is_not_exposed() -> Result<(), String> {
    let lib = "macro_rules! declare {
    ($name:ident) => {
        #[cfg(not(test))]
        pub mod $name;
    };
}
declare!(real);

#[cfg(test)]
pub mod real {
    pub fn f(x: i32) -> i32 {
        x
    }
}

#[cfg(test)]
mod tests {
    use crate::real::f;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let real = "pub fn f(x: i32) -> i32 {\n    x\n}\n";
    let patch = "diff --git a/src/real.rs b/src/real.rs
index 1111111..2222222 100644
--- a/src/real.rs
+++ b/src/real.rs
@@ -1,3 +1,3 @@
 pub fn f(x: i32) -> i32 {
-    -x
+    x
 }
";
    assert_file_tails_not_exposed(
        &[("src/lib.rs", lib), ("src/real.rs", real)],
        patch,
        &[("src/real.rs", "x")],
        2,
    )
}

/// An inner attribute on the owner's block can compile out the tail this
/// reading compares (#14852 companion finding), so a body holding any `#`
/// is never credited.
#[test]
fn an_attribute_inside_the_owner_body_is_not_exposed() -> Result<(), String> {
    let lib = "pub fn f(x: i32) -> i32 {
    #![cfg_attr(any(), rustfmt::skip)]
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins() {
        assert_eq!(f(5), 5);
    }
}
";
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
 pub fn f(x: i32) -> i32 {
     #![cfg_attr(any(), rustfmt::skip)]
-    -x
+    x
 }
";
    let findings = TempRepo::create(lib, patch)?.findings()?;
    let finding = tail_finding(&findings, 3, "x")?;
    assert!(
        !matches!(
            finding.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ) && finding.ripr.propagate.state != StageState::Yes,
        "{finding:?}"
    );
    Ok(())
}
