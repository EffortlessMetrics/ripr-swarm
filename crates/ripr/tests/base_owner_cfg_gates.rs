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
            if let Some(parent) = root.join(path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
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
    tail_class_with(path, owner_file, items, import, &[])
}

/// [`tail_class`] with `extra` files beside the four it writes.
fn tail_class_with(
    path: TailPath,
    owner_file: &str,
    items: &str,
    import: &str,
    extra: &[(&str, &str)],
) -> Result<(ExposureClass, Finding), String> {
    let mut files = vec![
        ("src/lib.rs", lib_with(items, import, path)),
        (owner_file, path.owner()),
        ("src/fake.rs", path.stub()),
        (
            "src/other.rs",
            "pub fn g(x: i32) -> i32 {\n    x\n}\n".to_string(),
        ),
    ];
    files.extend(
        extra
            .iter()
            .map(|(file, text)| (*file, (*text).to_string())),
    );
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

/// One narrowed-gate case: `(name, owner file, lib.rs items, test-module
/// import, extra files, expected class on the constant and the unary tail
/// path)`. An expected `PropagationUnknown` is a refusal that withholds
/// propagation credit; any other class must carry the propagation witness.
/// A positive expects main's class on each path: the unary tail keeps its
/// own same-name, rename, `include!` and `macro_rules!` refusals, so on that
/// path main itself reads several of these shapes `propagation_unknown`.
type NarrowCase = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Vec<(&'static str, &'static str)>,
    [ExposureClass; 2],
);

fn check_narrow_cases(cases: Vec<NarrowCase>) -> Result<(), String> {
    let mut wrong = Vec::new();
    for (index, path) in [TailPath::Constant, TailPath::Unary]
        .into_iter()
        .enumerate()
    {
        for (name, owner_file, items, import, extra, classes) in &cases {
            let expected = &classes[index];
            let (class, finding) = tail_class_with(path, owner_file, items, import, extra)?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            let gated = *expected == ExposureClass::PropagationUnknown;
            if class != *expected || credited == gated {
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

/// The test module's direct import of the owner.
const DIRECT: &str = "use crate::real::ready;";

/// A macro emitting an out-of-line `mod $m;`, defined in another package.
const OTHER_PACKAGE_DECLARE: &str = "macro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m;\n    };\n}\n";

/// #14952 review R1: only a macro that may emit an out-of-line module (or an
/// `include!` or `#[path]`) gates, and only owners in the package defining
/// it unless it is exported. Positives (main's class restored): a macro
/// emitting only inline modules, and a non-exported `mod $m;` macro in
/// another package. Negatives (still gated): the same macro exported, the
/// same macro in the owner's own package, and a macro emitting `include!`
/// (newly gated: its body holds no `mod`).
#[test]
fn macro_gates_need_an_out_of_line_module_in_the_owners_package() -> Result<(), String> {
    const EXPORTED: &str = "#[macro_export]\nmacro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m;\n    };\n}\n";
    check_narrow_cases(vec![
        (
            "inline-only macro",
            "src/real.rs",
            "mod real;\nmacro_rules! wrap {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m {\n            pub fn helper() {}\n        }\n    };\n}\nwrap!(inner);\n",
            DIRECT,
            vec![],
            [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        ),
        (
            "mod-emitting macro in another package",
            "src/real.rs",
            "mod real;\n",
            DIRECT,
            vec![("other/src/lib.rs", OTHER_PACKAGE_DECLARE)],
            [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        ),
        (
            "exported mod-emitting macro in another package",
            "src/real.rs",
            "mod real;\n",
            DIRECT,
            vec![("other/src/lib.rs", EXPORTED)],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "mod-emitting macro in the owner's package",
            "src/real.rs",
            "mod real;\nmacro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m;\n    };\n}\n",
            DIRECT,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "include-emitting macro in the owner's package",
            "src/real.rs",
            "mod real;\nmacro_rules! inc {\n    ($f:literal) => {\n        include!($f);\n    };\n}\n",
            DIRECT,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
    ])
}

/// #14952 review R2: a private `#[cfg(unix)] use super::ready;` inside the
/// `#[cfg(test)]` module, where nothing else can bind `ready`, only decides
/// whether `ready` names the parent's item or nothing: main's
/// `weakly_exposed` (the glob-reached re-export) is restored. Negatives: a
/// gated rename in the test module, the same import beside a glob that may
/// bind `ready`, and a private gated rename at file scope (which the test
/// module's `use super::*` reaches, so a `pub`-only rule would be unsound).
#[test]
fn a_redundant_gated_test_import_is_not_a_gate() -> Result<(), String> {
    check_narrow_cases(vec![
        (
            "gated super import in the test module",
            "src/real.rs",
            "mod real;\npub use real::ready;\n",
            "use super::*;\n    #[cfg(unix)]\n    use super::ready;",
            vec![],
            [ExposureClass::WeaklyExposed, ExposureClass::WeaklyExposed],
        ),
        (
            "gated rename in the test module",
            "src/real.rs",
            "mod real;\nmod fake;\npub use real::ready;\n",
            "use super::*;\n    #[cfg(unix)]\n    use super::fake::stub as ready;",
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "gated super import beside another glob",
            "src/real.rs",
            "mod real;\nmod fake;\npub use real::ready;\n",
            "use crate::fake::*;\n    #[cfg(unix)]\n    use super::ready;",
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "private gated rename at file scope",
            "src/real.rs",
            "mod real;\nmod fake;\npub use real::ready;\n#[cfg(test)]\nuse fake::stub as ready;\n",
            GLOB,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
    ])
}

/// #14952 review R3: a gate must name a crate-local path. A gated `use`
/// rooted at a sysroot crate (`std::fmt::Write as ready`, a
/// `std::io::prelude::*` glob) and an `include!` of another file with the
/// owner's stem (`../data/real.rs`) no longer gate: main's class is
/// restored. Negatives: the same names through crate-local roots
/// (`self::fake::stub as ready`, a `self::prelude::*` glob, a bare
/// `fake::stub`) and an `include!` resolving to the owner's own file.
#[test]
fn gates_must_name_crate_local_paths() -> Result<(), String> {
    const PRELUDE: &str = "use crate::prelude::ready;";
    check_narrow_cases(vec![
        (
            "std rename to the owner's name",
            "src/real.rs",
            "mod real;\n#[cfg(unix)]\nuse std::fmt::Write as ready;\n",
            DIRECT,
            vec![],
            [
                ExposureClass::WeaklyExposed,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "std glob over a module named like the owner file",
            "src/prelude.rs",
            "mod prelude;\n#[cfg(unix)]\nuse std::io::prelude::*;\n",
            PRELUDE,
            vec![],
            [ExposureClass::Exposed, ExposureClass::Exposed],
        ),
        (
            "include of another file with the owner's stem",
            "src/real.rs",
            "mod real;\n#[cfg(unix)]\nmod data {\n    include!(\"../data/real.rs\");\n}\n",
            DIRECT,
            vec![("data/real.rs", "pub fn other() {}\n")],
            [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        ),
        (
            "self rename to the owner's name",
            "src/real.rs",
            "mod real;\nmod fake;\n#[cfg(unix)]\nuse self::fake::stub as ready;\n",
            DIRECT,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "bare local rename to the owner's name",
            "src/real.rs",
            "mod real;\nmod fake;\n#[cfg(unix)]\nuse fake::stub as ready;\n",
            DIRECT,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "self glob over the owner's module",
            "src/prelude.rs",
            "mod prelude;\n#[cfg(unix)]\npub use self::prelude::*;\n",
            PRELUDE,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
        (
            "include resolving to the owner's file",
            "src/real.rs",
            "mod real;\n#[cfg(unix)]\nmod shim {\n    include!(\"real.rs\");\n}\n",
            DIRECT,
            vec![],
            [
                ExposureClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ],
        ),
    ])
}

/// One #15035 round-4 gap: `(name, owner file, lib.rs items, test-module
/// import, extra files for a tail path)`.
type GapCase = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    fn(TailPath) -> Vec<(&'static str, String)>,
);

/// Each gap shape must withhold propagation credit and read
/// `propagation_unknown` on both tail paths.
fn check_gap_cases(cases: Vec<GapCase>) -> Result<(), String> {
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        for (name, owner_file, items, import, extra) in &cases {
            let extra = extra(path);
            let extra: Vec<(&str, &str)> = extra
                .iter()
                .map(|(file, text)| (*file, text.as_str()))
                .collect();
            let (class, finding) = tail_class_with(path, owner_file, items, import, &extra)?;
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

/// A module whose `ready` is the test-only stand-in.
fn alias_stand_in(_: TailPath) -> Vec<(&'static str, String)> {
    vec![(
        "src/alt.rs",
        "pub use crate::fake::stub as ready;\n".to_string(),
    )]
}

/// #15035 review F1: a gated `use` of a module (a plain alias, a `pub use
/// crate::..` alias, or two same-named modules in different directories)
/// swaps which file the test reaches, so it gates every owner in the
/// modules its path names.
#[test]
fn a_gated_module_alias_is_a_gate() -> Result<(), String> {
    check_gap_cases(vec![
        (
            "gated module alias",
            "src/real.rs",
            "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse real as imp;\n#[cfg(test)]\nuse alt as imp;\n",
            "use super::imp::ready;",
            alias_stand_in,
        ),
        (
            "gated pub crate module alias",
            "src/real.rs",
            "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\npub use crate::real as imp;\n#[cfg(test)]\npub use crate::alt as imp;\n",
            "use super::imp::ready;",
            alias_stand_in,
        ),
        (
            "gated import of same-named modules in two directories",
            "src/a/imp.rs",
            "mod a;\nmod b;\nmod fake;\n#[cfg(not(test))]\nuse a::imp;\n#[cfg(test)]\nuse b::imp;\n",
            "use super::imp::ready;",
            |_| {
                vec![
                    ("src/a.rs", "pub mod imp;\n".to_string()),
                    ("src/b.rs", "pub mod imp;\n".to_string()),
                    (
                        "src/b/imp.rs",
                        "pub use crate::fake::stub as ready;\n".to_string(),
                    ),
                ]
            },
        ),
    ])
}

/// #15035 review F2: a module-level macro call in the `#[cfg(test)]` module
/// may emit the import that rebinds `ready`, so the gated `use
/// super::ready;` beside it stays a gate.
#[test]
fn a_macro_call_in_the_test_module_keeps_the_gate() -> Result<(), String> {
    check_gap_cases(vec![(
        "gated super import beside a module-level macro call",
        "src/real.rs",
        "mod real;\nmod fake;\npub use real::ready;\nmacro_rules! test_imports {\n    () => {\n        #[cfg(not(unix))]\n        use crate::fake::stub as ready;\n    };\n}\n",
        "use super::*;\n    #[cfg(unix)]\n    use super::ready;\n    test_imports!();",
        |_| Vec::new(),
    )])
}

/// #15035 review F3: an `extern crate .. as std` anywhere in the crate makes
/// `std::` a crate-local root, so a gated `use std::..` is a gate.
#[test]
fn a_renamed_extern_crate_disables_the_sysroot_exemption() -> Result<(), String> {
    check_gap_cases(vec![(
        "extern crate self as std",
        "src/real.rs",
        "extern crate self as std;\nmod real;\nmod fake;\nmod api;\npub use api::*;\n",
        GLOB,
        |_| {
            vec![(
                "src/api.rs",
                "#[cfg(not(test))]\npub use std::real::ready;\n#[cfg(test)]\npub use std::fake::stub as ready;\n"
                    .to_string(),
            )]
        },
    )])
}

/// #15035 review F4: a `;` inside a string literal of the `cfg` predicate
/// does not hide a private gated import from the caller's prefilter.
#[test]
fn a_semicolon_in_a_cfg_string_does_not_hide_the_gate() -> Result<(), String> {
    check_gap_cases(vec![(
        "private gated import under a string predicate holding a semicolon",
        "src/real.rs",
        "mod real;\nmod fake;\nmod api;\npub use api::ready_api as ready;\n",
        GLOB,
        |path| {
            vec![(
                "src/api.rs",
                format!(
                    "#[cfg(not(feature = \"x;y\"))]\nuse crate::real::ready;\n#[cfg(feature = \"x;y\")]\nuse crate::fake::stub as ready;\npub fn ready_api() -> i32 {{ 0 }}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pins() {{\n        {}\n    }}\n}}\n",
                    path.pin()
                ),
            )]
        },
    )])
}

/// #15035 review F5: an `include!` whose path is not a single literal
/// (`concat!(env!("CARGO_MANIFEST_DIR"), "/src/real.rs")`) in a cfg-twin
/// module gates the file its trailing literal names.
#[test]
fn a_non_literal_include_is_a_gate() -> Result<(), String> {
    check_gap_cases(vec![(
        "cfg-twin modules including through concat",
        "src/real.rs",
        "mod real;\nmod fake;\n#[cfg(not(test))]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\"));\n}\n#[cfg(test)]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/alt.rs\"));\n}\n",
        "use super::imp::ready;",
        alias_stand_in,
    )])
}
