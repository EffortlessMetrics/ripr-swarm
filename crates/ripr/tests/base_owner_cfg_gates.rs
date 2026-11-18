//! Public-API acceptance for #14952: a `cfg` gate that lives outside a `mod`
//! declaration (`include!`, a `pub use` re-export, a macro-emitted `mod`)
//! must keep the shared base-owner identity from crediting the changed
//! function on both whole-body tail paths that rely on it: the constant
//! tail (#14852, `true` to `false`) and the unary tail (#14815, `-x` to `x`).
//!
//! In each gated shape the tests run a stand-in (`fake::stub`, re-exported
//! as `ready` under `cfg(test)`) that returns the new value on both sides,
//! so `cargo test` cannot tell the versions apart. The ungated controls
//! (the issue's `pos_plain`, `pos_unrelated_mod`, `pos_unrelated_gated`)
//! keep their credit.

use ripr::domain::StageState;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(files: &[(&str, String)], patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-base-owner-cfg-gates-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"cfg-gates\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// One of the two whole-body tail readings that use the shared identity.
#[derive(Clone, Copy, Debug)]
enum TailPath {
    /// #14852: `true` replaced by `false`.
    Constant,
    /// #14815: `-x` replaced by `x`.
    Unary,
}

impl TailPath {
    fn signature(self) -> &'static str {
        match self {
            Self::Constant => "pub fn ready() -> bool {",
            Self::Unary => "pub fn ready(x: i32) -> i32 {",
        }
    }

    fn stub_signature(self) -> &'static str {
        match self {
            Self::Constant => "pub fn stub() -> bool {",
            Self::Unary => "pub fn stub(x: i32) -> i32 {",
        }
    }

    fn old_tail(self) -> &'static str {
        match self {
            Self::Constant => "true",
            Self::Unary => "-x",
        }
    }

    fn new_tail(self) -> &'static str {
        match self {
            Self::Constant => "false",
            Self::Unary => "x",
        }
    }

    /// An exact pin of the new value, where the two versions differ.
    fn pin(self) -> &'static str {
        match self {
            Self::Constant => "assert_eq!(ready(), false);",
            Self::Unary => "assert_eq!(ready(5), 5);",
        }
    }

    /// The changed owner's file: the new tail on line 2.
    fn owner(self) -> String {
        format!("{}\n    {}\n}}\n", self.signature(), self.new_tail())
    }

    /// The test-only stand-in, returning the new value on both sides.
    fn stub(self) -> String {
        format!("{}\n    {}\n}}\n", self.stub_signature(), self.new_tail())
    }

    /// The diff that changes line 2 of `file` from the old tail to the new.
    fn patch(self, file: &str) -> String {
        format!(
            "diff --git a/{file} b/{file}
index 1111111..2222222 100644
--- a/{file}
+++ b/{file}
@@ -1,3 +1,3 @@
 {signature}
-    {old}
+    {new}
 }}
",
            signature = self.signature(),
            old = self.old_tail(),
            new = self.new_tail(),
        )
    }
}

/// `src/lib.rs`: `items`, then a test module pinning `ready` through
/// `import` (`use super::*` unless a case names the owner directly).
fn lib_with(items: &str, import: &str, path: TailPath) -> String {
    format!(
        "{items}
#[cfg(test)]
mod tests {{
    {import}
    #[test]
    fn pins() {{
        {pin}
    }}
}}
",
        pin = path.pin()
    )
}

/// The class and finding of the changed tail in `owner_file`, with `items`
/// in `src/lib.rs` and the test module's `import`, plus `src/fake.rs` (the
/// stand-in) and `src/other.rs`.
fn tail_class(
    path: TailPath,
    owner_file: &str,
    items: &str,
    import: &str,
) -> Result<(ExposureClass, Finding), String> {
    let files = [
        ("src/lib.rs", lib_with(items, import, path)),
        (owner_file, path.owner()),
        ("src/fake.rs", path.stub()),
        (
            "src/other.rs",
            "pub fn g(x: i32) -> i32 {\n    x\n}\n".to_string(),
        ),
    ];
    let findings = TempRepo::create(&files, &path.patch(owner_file))?.findings()?;
    let tail = path.new_tail();
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.file.ends_with(owner_file)
            && finding.probe.location.line == 2
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok((finding.class.clone(), finding.clone())),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` in {owner_file}, got {other:?} in {findings:?}"
        )),
    }
}

/// The test module's import of everything its parent names.
const GLOB: &str = "use super::*;";

/// The test-only stand-in exported under the owner's name.
const STAND_IN: &str = "#[cfg(test)]\nmod fake;\n#[cfg(test)]\npub use fake::stub as ready;\n";

/// The issue's four gated shapes: `(name, owner file, lib.rs items)`.
fn gated_shapes() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "inc_cfg",
            "src/real.rs",
            format!("#[cfg(not(test))]\ninclude!(\"real.rs\");\n{STAND_IN}"),
        ),
        (
            "inc_mod",
            "src/real_impl.rs",
            format!(
                "#[cfg(not(test))]\nmod real {{\n    include!(\"real_impl.rs\");\n}}\n#[cfg(not(test))]\npub use real::ready;\n{STAND_IN}"
            ),
        ),
        (
            "cfg_use",
            "src/real.rs",
            format!("mod real;\n#[cfg(not(test))]\npub use real::ready;\n{STAND_IN}"),
        ),
        (
            "macro_mod_decl",
            "src/real.rs",
            format!(
                "macro_rules! declare {{\n    ($m:ident) => {{\n        #[cfg(not(test))]\n        mod $m;\n    }};\n}}\ndeclare!(real);\n#[cfg(not(test))]\npub use real::ready;\n{STAND_IN}"
            ),
        ),
    ]
}

/// #14952 acceptance: each gated shape reads `propagation_unknown` (no
/// propagation credit) on both tail paths. Before the shared identity read
/// these gates, all four read `weakly_exposed` with a complete propagation
/// witness on the constant-tail path; the unary-tail path already refused
/// them through its own same-name, rename, `include!` and `macro_rules!`
/// checks, and now refuses them through the identity as well.
#[test]
fn gated_shapes_outside_mod_declarations_read_propagation_unknown() -> Result<(), String> {
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        for (name, owner_file, items) in gated_shapes() {
            let (class, finding) = tail_class(path, owner_file, &items, GLOB)?;
            if class != ExposureClass::PropagationUnknown
                || finding.ripr.propagate.state == StageState::Yes
            {
                wrong.push(format!(
                    "{path:?} {name}: {class:?} {:?}",
                    finding.ripr.propagate
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// #14952 controls: the ungated declaration (`pos_plain`), an unrelated
/// gated module (`pos_unrelated_mod`) and a gated re-export of another name
/// (`pos_unrelated_gated`) keep their credit on both paths. Through the
/// test module's direct import of `crate::real::ready` the exact pin reads
/// `exposed`; through an ungated `pub use real::ready` re-export it reads
/// `weakly_exposed` (reveal's reading of a re-exported owner, unchanged
/// here), with the propagation witness credited either way.
#[test]
fn ungated_controls_keep_their_credit_on_both_tail_paths() -> Result<(), String> {
    const DIRECT: &str = "use crate::real::ready;";
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        for (name, items, import, expected) in [
            ("pos_plain", "mod real;\n", DIRECT, ExposureClass::Exposed),
            (
                "pos_unrelated_mod",
                "mod real;\n#[cfg(test)]\nmod other;\n",
                DIRECT,
                ExposureClass::Exposed,
            ),
            (
                "pos_unrelated_gated",
                "mod real;\nmod other;\n#[cfg(not(test))]\npub use other::g;\n",
                DIRECT,
                ExposureClass::Exposed,
            ),
            (
                "pos_plain re-export",
                "mod real;\npub use real::ready;\n",
                GLOB,
                ExposureClass::WeaklyExposed,
            ),
            (
                "pos_unrelated_mod re-export",
                "mod real;\npub use real::ready;\n#[cfg(test)]\nmod other;\n",
                GLOB,
                ExposureClass::WeaklyExposed,
            ),
            (
                "pos_unrelated_gated re-export",
                "mod real;\npub use real::ready;\nmod other;\n#[cfg(not(test))]\npub use other::g;\n",
                GLOB,
                ExposureClass::WeaklyExposed,
            ),
        ] {
            let (class, finding) = tail_class(path, "src/real.rs", items, import)?;
            if class != expected || finding.ripr.propagate.state != StageState::Yes {
                wrong.push(format!(
                    "{path:?} {name}: {class:?} {:?}",
                    finding.ripr.propagate
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}
