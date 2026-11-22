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
    tail_class_patched(path, owner_file, lib_with(items, import, path), extra, "")
}

/// [`tail_class_with`] with the whole `lib` text given, and `lib_patch` (a
/// diff of other files, possibly empty) after the owner's diff.
fn tail_class_patched(
    path: TailPath,
    owner_file: &str,
    lib: String,
    extra: &[(&str, &str)],
    lib_patch: &str,
) -> Result<(ExposureClass, Finding), String> {
    let mut files = vec![
        ("src/lib.rs", lib),
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
    let patch = format!("{}{lib_patch}", path.patch(owner_file));
    let findings = TempRepo::create(&files, &patch)?.findings()?;
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
            "include of another file named across split literals",
            "src/real.rs",
            "mod real;\nmod generated {\n    include!(concat!(env!(\"OUT_DIR\"), \"/\", \"gen\", \".rs\"));\n}\n",
            DIRECT,
            vec![],
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

/// Files beside the four [`tail_class_with`] writes, for a tail path.
type Extra = fn(TailPath) -> Vec<(&'static str, String)>;

fn no_extra(_: TailPath) -> Vec<(&'static str, String)> {
    Vec::new()
}

/// One #15035 gap shape and its ungated control: the same fixture with the
/// gate (and the test-only twin it selects) removed, so a refusal caused by
/// a fixture the reading cannot parse would fail the control.
struct GapCase {
    name: &'static str,
    owner: &'static str,
    items: &'static str,
    import: &'static str,
    extra: Extra,
    control_items: &'static str,
    control_import: &'static str,
    control_extra: Extra,
    /// The control's class on the constant and the unary tail path (main's
    /// class; the unary tail keeps its own rename refusals).
    control: [ExposureClass; 2],
}

fn tail_class_of(
    path: TailPath,
    owner: &str,
    items: &str,
    import: &str,
    extra: Extra,
) -> Result<(ExposureClass, Finding), String> {
    let extra = extra(path);
    let extra: Vec<(&str, &str)> = extra
        .iter()
        .map(|(file, text)| (*file, text.as_str()))
        .collect();
    tail_class_with(path, owner, items, import, &extra)
}

/// Each gap shape must withhold propagation credit and read
/// `propagation_unknown` on both tail paths, and its control must read its
/// expected class, with the propagation witness unless that class is
/// `propagation_unknown`.
fn check_gap_cases(cases: Vec<GapCase>) -> Result<(), String> {
    let mut wrong = Vec::new();
    for (index, path) in [TailPath::Constant, TailPath::Unary]
        .into_iter()
        .enumerate()
    {
        for case in &cases {
            let (class, finding) =
                tail_class_of(path, case.owner, case.items, case.import, case.extra)?;
            if class != ExposureClass::PropagationUnknown
                || finding.ripr.propagate.state == StageState::Yes
            {
                wrong.push(format!(
                    "{path:?} {}: {class:?} {:?}",
                    case.name, finding.ripr.propagate
                ));
            }
            let expected = &case.control[index];
            let (class, finding) = tail_class_of(
                path,
                case.owner,
                case.control_items,
                case.control_import,
                case.control_extra,
            )?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            if class != *expected || credited == (*expected == ExposureClass::PropagationUnknown) {
                wrong.push(format!(
                    "{path:?} {} control: {class:?} {:?}",
                    case.name, finding.ripr.propagate
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

const ALIAS_IMPORT: &str = "use super::imp::ready;";

/// #15035 review F1: a gated `use` of a module (a plain alias, a `pub use
/// crate::..` alias, an alias inside a `{..}` list or of `self`, an alias
/// chain, or two same-named modules in different directories) swaps which
/// file the test reaches, so it gates every owner in the modules it may
/// alias.
#[test]
fn a_gated_module_alias_is_a_gate() -> Result<(), String> {
    const CREDITED: [ExposureClass; 2] = [
        ExposureClass::WeaklyExposed,
        ExposureClass::PropagationUnknown,
    ];
    check_gap_cases(vec![
        GapCase {
            name: "gated module alias",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse real as imp;\n#[cfg(test)]\nuse alt as imp;\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: "mod real;\nmod fake;\nmod alt;\nuse real as imp;\n",
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: CREDITED,
        },
        GapCase {
            name: "gated pub crate module alias",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\npub use crate::real as imp;\n#[cfg(test)]\npub use crate::alt as imp;\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: "mod real;\nmod fake;\nmod alt;\npub use crate::real as imp;\n",
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: CREDITED,
        },
        GapCase {
            name: "gated module alias in a use list",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse crate::{real as imp};\n#[cfg(test)]\nuse crate::{alt as imp};\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: "mod real;\nmod fake;\nmod alt;\nuse crate::{real as imp};\n",
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: CREDITED,
        },
        GapCase {
            name: "gated self alias in a use list",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse self::real::{self as imp};\n#[cfg(test)]\nuse self::alt::{self as imp};\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: "mod real;\nmod fake;\nmod alt;\nuse self::real::{self as imp};\n",
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: CREDITED,
        },
        GapCase {
            name: "gated alias behind an ungated alias chain",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\nmod sel {\n    #[cfg(not(test))]\n    pub use crate::real as inner;\n    #[cfg(test)]\n    pub use crate::alt as inner;\n}\nuse sel::inner as imp;\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: "mod real;\nmod fake;\nmod alt;\nmod sel {\n    pub use crate::real as inner;\n}\nuse sel::inner as imp;\n",
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: CREDITED,
        },
        GapCase {
            name: "gated import of same-named modules in two directories",
            owner: "src/a/imp.rs",
            items: "mod a;\nmod b;\nmod fake;\n#[cfg(not(test))]\nuse a::imp;\n#[cfg(test)]\nuse b::imp;\n",
            import: ALIAS_IMPORT,
            extra: two_directories,
            control_items: "mod a;\nmod b;\nmod fake;\nuse a::imp;\n",
            control_import: ALIAS_IMPORT,
            control_extra: two_directories,
            control: [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        },
    ])
}

/// `src/a/imp.rs` and `src/b/imp.rs`, the second the test-only stand-in.
fn two_directories(_: TailPath) -> Vec<(&'static str, String)> {
    vec![
        ("src/a.rs", "pub mod imp;\n".to_string()),
        ("src/b.rs", "pub mod imp;\n".to_string()),
        (
            "src/b/imp.rs",
            "pub use crate::fake::stub as ready;\n".to_string(),
        ),
    ]
}

/// The owner file with a second item, `Helper`, beside the changed tail.
fn owner_with_helper(path: TailPath) -> Vec<(&'static str, String)> {
    vec![(
        "src/real.rs",
        format!("{}pub struct Helper;\n", path.owner()),
    )]
}

/// [`owner_with_helper`] plus `src/other.rs` holding `other`.
fn other_file(path: TailPath, other: &str) -> Vec<(&'static str, String)> {
    let mut files = owner_with_helper(path);
    files.push(("src/other.rs", other.to_string()));
    files
}

/// #15035 round-5 review: a gated `use` gates a module only through the
/// segments by which it may alias one (its last segment, a rename and the
/// renamed segment, every segment of a glob or `self` import). An
/// intermediate segment only names the module its leaf lives in, so main's
/// class is restored for a gated import of another item of the owner's
/// module, from another file or as a `pub` re-export, and for a third-party
/// path through a segment named like the owner's module. Each sits beside
/// its gated counterpart naming the owner, which still gates.
#[test]
fn only_aliasing_segments_of_a_gated_use_gate() -> Result<(), String> {
    const OWNER_REEXPORT: &str = "mod real;\nmod other;\npub use real::ready;\n";
    let shapes: Vec<(&str, &str, Extra, ExposureClass)> = vec![
        (
            "gated import of another item of the owner's module",
            OWNER_REEXPORT,
            |path| {
                other_file(
                    path,
                    "#[cfg(test)]\nuse crate::real::Helper;\npub fn o() {}\n",
                )
            },
            ExposureClass::WeaklyExposed,
        ),
        (
            "same import naming the owner",
            OWNER_REEXPORT,
            |path| {
                other_file(
                    path,
                    "#[cfg(test)]\npub use crate::real::ready;\npub fn o() {}\n",
                )
            },
            ExposureClass::PropagationUnknown,
        ),
        (
            "third-party path through a segment named like the owner's module",
            OWNER_REEXPORT,
            |path| {
                other_file(
                    path,
                    "#[cfg(feature = \"serde\")]\nuse serde_like::real::Thing;\npub fn o() {}\n",
                )
            },
            ExposureClass::WeaklyExposed,
        ),
        (
            "same third-party path ending in the owner's module",
            OWNER_REEXPORT,
            |path| {
                other_file(
                    path,
                    "#[cfg(feature = \"serde\")]\nuse serde_like::real;\npub fn o() {}\n",
                )
            },
            ExposureClass::PropagationUnknown,
        ),
        (
            "gated re-export of another item of the owner's module",
            "mod real;\npub use real::ready;\n#[cfg(feature = \"x\")]\npub use real::Helper;\n",
            owner_with_helper,
            ExposureClass::WeaklyExposed,
        ),
        (
            "same re-export naming the owner",
            "mod real;\n#[cfg(feature = \"x\")]\npub use real::ready;\npub use real::Helper;\n",
            owner_with_helper,
            ExposureClass::PropagationUnknown,
        ),
    ];
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        for (name, items, extra, expected) in &shapes {
            let (class, finding) = tail_class_of(path, "src/real.rs", items, GLOB, *extra)?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            if class != *expected || credited == (*expected == ExposureClass::PropagationUnknown) {
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

/// The class of the changed tail of `Thing::ready` in `src/real.rs` (an
/// inherent method, its tail on line 4), with `items` in `src/lib.rs` and a
/// test calling `Thing::ready` through `use super::*`, plus the test-only
/// `src/fake.rs` twin of `Thing` returning the new value.
fn impl_tail_class(path: TailPath, items: &str) -> Result<(ExposureClass, Finding), String> {
    let method = |tail: &str| {
        format!(
            "pub struct Thing;\nimpl Thing {{\n    {}\n        {tail}\n    }}\n}}\n",
            path.signature()
        )
    };
    let call = match path {
        TailPath::Constant => "assert_eq!(Thing::ready(), false);",
        TailPath::Unary => "assert_eq!(Thing::ready(5), 5);",
    };
    let lib = format!(
        "{items}#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pins() {{\n        {call}\n    }}\n}}\n"
    );
    let patch = format!(
        "diff --git a/src/real.rs b/src/real.rs
index 1111111..2222222 100644
--- a/src/real.rs
+++ b/src/real.rs
@@ -2,5 +2,5 @@
 impl Thing {{
     {signature}
-        {old}
+        {new}
     }}
 }}
",
        signature = path.signature(),
        old = path.old_tail(),
        new = path.new_tail(),
    );
    let files = vec![
        ("src/lib.rs", lib),
        ("src/real.rs", method(path.new_tail())),
        ("src/fake.rs", method(path.new_tail())),
    ];
    let findings = TempRepo::create(&files, &patch)?.findings()?;
    let tail = path.new_tail();
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.file.ends_with("src/real.rs")
            && finding.probe.location.line == 4
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok((finding.class.clone(), finding.clone())),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` in src/real.rs, got {other:?} in {findings:?}"
        )),
    }
}

/// #15035 round-5 review: a gated swap of the owner's impl self type
/// (`pub use real::Thing;` beside a test-only `pub use fake::Thing;`) swaps
/// which `Thing::ready` the test runs, so it gates the owner even though no
/// gated path names `ready` or a module the owner may be reached through by
/// alias. Control: the ungated re-export keeps main's credit.
#[test]
fn a_gated_swap_of_the_owners_self_type_is_a_gate() -> Result<(), String> {
    let mut wrong = Vec::new();
    for (index, path) in [TailPath::Constant, TailPath::Unary]
        .into_iter()
        .enumerate()
    {
        for (name, items, expected) in [
            (
                "gated self-type twin",
                "mod real;\nmod fake;\n#[cfg(not(test))]\npub use real::Thing;\n#[cfg(test)]\npub use fake::Thing;\n",
                ExposureClass::PropagationUnknown,
            ),
            (
                "ungated self-type re-export",
                "mod real;\nmod fake;\npub use real::Thing;\n",
                [
                    ExposureClass::WeaklyExposed,
                    ExposureClass::PropagationUnknown,
                ][index]
                    .clone(),
            ),
        ] {
            let (class, finding) = impl_tail_class(path, items)?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            if class != expected || credited == (expected == ExposureClass::PropagationUnknown) {
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

/// The test-module import of F2: a gated redundant `super::ready` import
/// beside a module-level macro call.
const MACRO_BESIDE_IMPORT: &str =
    "use super::*;\n    #[cfg(unix)]\n    use super::ready;\n    test_imports!();";

/// #15035 review F2: a module-level macro call in the `#[cfg(test)]` module
/// may emit the import that rebinds `ready`, so the gated `use
/// super::ready;` beside it stays a gate.
#[test]
fn a_macro_call_in_the_test_module_keeps_the_gate() -> Result<(), String> {
    const ITEMS: &str = "mod real;\nmod fake;\npub use real::ready;\nmacro_rules! test_imports {\n    () => {\n        #[cfg(not(unix))]\n        use crate::fake::stub as ready;\n    };\n}\n";
    check_gap_cases(vec![GapCase {
        name: "gated super import beside a module-level macro call",
        owner: "src/real.rs",
        items: ITEMS,
        import: MACRO_BESIDE_IMPORT,
        extra: no_extra,
        control_items: ITEMS,
        control_import: "use super::*;\n    use super::ready;\n    test_imports!();",
        control_extra: no_extra,
        control: [ExposureClass::WeaklyExposed, ExposureClass::WeaklyExposed],
    }])
}

/// `src/api.rs` re-exporting `ready` through `std::`.
fn std_api(gated: bool) -> String {
    if gated {
        "#[cfg(not(test))]\npub use std::real::ready;\n#[cfg(test)]\npub use std::fake::stub as ready;\n".to_string()
    } else {
        "pub use std::real::ready;\n".to_string()
    }
}

/// #15035 review F3: an `extern crate .. as std` anywhere in the crate makes
/// `std::` a crate-local root, so a gated `use std::..` is a gate. The crate
/// is `#![no_std]`, so the rename is valid Rust.
#[test]
fn a_renamed_extern_crate_disables_the_sysroot_exemption() -> Result<(), String> {
    const ITEMS: &str =
        "#![no_std]\nextern crate self as std;\nmod real;\nmod fake;\nmod api;\npub use api::*;\n";
    check_gap_cases(vec![GapCase {
        name: "extern crate self as std",
        owner: "src/real.rs",
        items: ITEMS,
        import: GLOB,
        extra: |_| vec![("src/api.rs", std_api(true))],
        control_items: ITEMS,
        control_import: GLOB,
        control_extra: |_| vec![("src/api.rs", std_api(false))],
        control: [
            ExposureClass::WeaklyExposed,
            ExposureClass::PropagationUnknown,
        ],
    }])
}

/// `src/api.rs` importing `ready` privately after `prefix`, gated by
/// `predicate` (`None`: the ungated import alone), with a test module
/// pinning it.
fn private_api(path: TailPath, prefix: &str, predicate: Option<&str>) -> String {
    let imports = match predicate {
        Some(predicate) => format!(
            "#[cfg(not({predicate}))]\nuse crate::real::ready;\n#[cfg({predicate})]\nuse crate::fake::stub as ready;\n"
        ),
        None => "use crate::real::ready;\n".to_string(),
    };
    format!(
        "{prefix}{imports}const D: &str = \"\";\npub fn ready_api() -> i32 {{ 0 }}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pins() {{\n        {}\n    }}\n}}\n",
        path.pin()
    )
}

/// `lib.rs` for [`private_api`]: the test reaches the owner only through
/// `src/api.rs`.
const PRIVATE_API_ITEMS: &str =
    "mod real;\nmod fake;\nmod api;\npub use api::ready_api as ready;\n";

/// #15035 review F4: a `;` inside a string literal of the `cfg` predicate
/// does not hide a private gated import from the caller's prefilter.
#[test]
fn a_semicolon_in_a_cfg_string_does_not_hide_the_gate() -> Result<(), String> {
    check_gap_cases(vec![GapCase {
        name: "private gated import under a string predicate holding a semicolon",
        owner: "src/real.rs",
        items: PRIVATE_API_ITEMS,
        import: GLOB,
        extra: |path| {
            vec![(
                "src/api.rs",
                private_api(path, "", Some("feature = \"x;y\"")),
            )]
        },
        control_items: PRIVATE_API_ITEMS,
        control_import: GLOB,
        control_extra: |path| vec![("src/api.rs", private_api(path, "", None))],
        control: [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
    }])
}

/// #15035 round-5 review: a raw C string (`cr"\"`, `cr#".."#`) before a
/// private gated import is lexed as raw, so its backslash does not swallow
/// the code after it and hide the gate from the caller's prefilter.
#[test]
fn a_raw_c_string_does_not_hide_the_gate() -> Result<(), String> {
    const CR: &str = "const C: &core::ffi::CStr = cr\"\\\";\n";
    const CR_HASH: &str = "const C: &core::ffi::CStr = cr#\"say \"hi\"\"#;\n";
    check_gap_cases(vec![
        GapCase {
            name: "raw C string ending in a backslash",
            owner: "src/real.rs",
            items: PRIVATE_API_ITEMS,
            import: GLOB,
            extra: |path| vec![("src/api.rs", private_api(path, CR, Some("feature = \"x\"")))],
            control_items: PRIVATE_API_ITEMS,
            control_import: GLOB,
            control_extra: |path| vec![("src/api.rs", private_api(path, CR, None))],
            control: [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        },
        GapCase {
            name: "raw C string with hashes",
            owner: "src/real.rs",
            items: PRIVATE_API_ITEMS,
            import: GLOB,
            extra: |path| {
                vec![(
                    "src/api.rs",
                    private_api(path, CR_HASH, Some("feature = \"x\"")),
                )]
            },
            control_items: PRIVATE_API_ITEMS,
            control_import: GLOB,
            control_extra: |path| vec![("src/api.rs", private_api(path, CR_HASH, None))],
            control: [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        },
    ])
}

/// The control of the `include!` gaps: an ungated `imp` module re-exporting
/// the owner without an `include!`. An `include!` naming the owner file
/// gates whether or not it is itself gated (it compiles the file a second
/// time), so the control drops it with the gate.
const INCLUDE_CONTROL: &str =
    "mod real;\nmod fake;\nmod alt;\nmod imp {\n    pub use crate::real::ready;\n}\n";

/// #15035 review F5: an `include!` whose path is not a single literal
/// (`concat!(env!("CARGO_MANIFEST_DIR"), "/src/real.rs")`) in a cfg-twin
/// module gates the file its trailing literals name, joined when they split
/// the file name (`"/src/re", "al.rs"`, round 5).
#[test]
fn a_non_literal_include_is_a_gate() -> Result<(), String> {
    check_gap_cases(vec![
        GapCase {
            name: "cfg-twin modules including through concat",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\"));\n}\n#[cfg(test)]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/alt.rs\"));\n}\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: INCLUDE_CONTROL,
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        },
        GapCase {
            name: "cfg-twin modules including a file name split across literals",
            owner: "src/real.rs",
            items: "mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/re\", \"al.rs\"));\n}\n#[cfg(test)]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/alt.rs\"));\n}\n",
            import: ALIAS_IMPORT,
            extra: alias_stand_in,
            control_items: INCLUDE_CONTROL,
            control_import: ALIAS_IMPORT,
            control_extra: alias_stand_in,
            control: [ExposureClass::Exposed, ExposureClass::PropagationUnknown],
        },
    ])
}

/// The class of the changed tail of `src/bin/helper/mod.rs` (module
/// `helper` of the bin `src/bin/app.rs`, whose text is `app`), pinned by a
/// test in `app` through `super::helper::ready`.
fn bin_module_tail_class(path: TailPath, app: &str) -> Result<(ExposureClass, Finding), String> {
    const OWNER: &str = "src/bin/helper/mod.rs";
    let call = match path {
        TailPath::Constant => "assert_eq!(super::helper::ready(), false);",
        TailPath::Unary => "assert_eq!(super::helper::ready(5), 5);",
    };
    let twin = format!(
        "#[cfg(test)]\nmod helper {{\n    {}\n        {}\n    }}\n}}\n",
        path.signature(),
        path.new_tail()
    );
    let app = format!(
        "{}fn main() {{}}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn pins() {{\n        {call}\n    }}\n}}\n",
        app.replace("@TWIN@", &twin)
    );
    let files = vec![
        ("src/lib.rs", "pub fn other() {}\n".to_string()),
        ("src/bin/app.rs", app),
        (OWNER, path.owner()),
    ];
    let findings = TempRepo::create(&files, &path.patch(OWNER))?.findings()?;
    let tail = path.new_tail();
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.file.ends_with(OWNER)
            && finding.probe.location.line == 2
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok((finding.class.clone(), finding.clone())),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` in {OWNER}, got {other:?} in {findings:?}"
        )),
    }
}

/// #15035 review: a target directory's `x/mod.rs` (`src/bin/helper/mod.rs`,
/// `tests/common/mod.rs`) is module `x`, not a crate root, so the
/// declaration and macro-scope checks apply to it. Gap: the bin declares
/// `helper` only through a macro emitting a gated `mod $m;`, beside a
/// test-only inline twin. Control: a plain `mod helper;` keeps main's
/// credit.
#[test]
fn a_target_directory_mod_file_is_not_a_crate_root() -> Result<(), String> {
    const MACRO: &str = "macro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m;\n    };\n}\ndeclare!(helper);\n@TWIN@";
    let mut wrong = Vec::new();
    for (index, path) in [TailPath::Constant, TailPath::Unary]
        .into_iter()
        .enumerate()
    {
        for (name, app, expected) in [
            (
                "macro-declared module with a test-only twin",
                MACRO,
                ExposureClass::PropagationUnknown,
            ),
            (
                "plain module declaration",
                "mod helper;\n",
                [
                    ExposureClass::WeaklyExposed,
                    ExposureClass::InfectionUnknown,
                ][index]
                    .clone(),
            ),
        ] {
            let (class, finding) = bin_module_tail_class(path, app)?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            if class != expected || credited == (expected == ExposureClass::PropagationUnknown) {
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

/// `lib.rs` items re-exporting the owner, with `src/imp.rs` (no `mod`
/// keyword, so the caller keeps it only through the outside-gate prefilter)
/// as a module of its own.
const OWNER_AND_IMP: &str = "mod real;\npub use real::ready;\npub mod imp;\n";

/// #15035 round-6 review F1: a comment between `include` and its `!`
/// (`include /* c */ !("real.rs")`) in a file with no `mod` keyword does
/// not hide the gated `include!` of the owner file from the caller's
/// prefilter. Control: the same spelling including another file keeps
/// main's credit.
#[test]
fn a_comment_inside_an_include_call_does_not_hide_it() -> Result<(), String> {
    check_gap_cases(vec![GapCase {
        name: "gated include of the owner file spelled with a comment",
        owner: "src/real.rs",
        items: OWNER_AND_IMP,
        import: GLOB,
        extra: |_| {
            vec![(
                "src/imp.rs",
                "#[cfg(not(test))]\ninclude /* real */ !(\"real.rs\");\n".to_string(),
            )]
        },
        control_items: OWNER_AND_IMP,
        control_import: GLOB,
        control_extra: |_| {
            vec![(
                "src/imp.rs",
                "#[cfg(not(test))]\ninclude /* other */ !(\"fake.rs\");\n".to_string(),
            )]
        },
        control: [
            ExposureClass::WeaklyExposed,
            ExposureClass::PropagationUnknown,
        ],
    }])
}

/// `lib.rs` items for [`a_user_macro_include_path_is_opaque`]: a
/// `src_file!` macro that expands to the owner file whatever its argument.
const SRC_FILE_MACRO: &str = "#[macro_use]\nmod paths {\n    macro_rules! src_file {\n        ($name:literal) => {\n            concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\")\n        };\n    }\n}\nmod real;\npub use real::ready;\npub mod imp;\n";

/// #15035 round-6 review F2: only `concat!`'s own trailing literals are
/// trusted as an `include!` path. A user macro's argument
/// (`include!(src_file!("unused.rs"))`) may expand to any path, here the
/// owner file, so it is an opaque include that gates every owner. Control:
/// the same file named through `concat!` keeps main's credit.
#[test]
fn a_user_macro_include_path_is_opaque() -> Result<(), String> {
    check_gap_cases(vec![GapCase {
        name: "gated include through a user macro",
        owner: "src/real.rs",
        items: SRC_FILE_MACRO,
        import: GLOB,
        extra: |_| {
            vec![
                (
                    "src/imp.rs",
                    "#[cfg(not(test))]\ninclude!(src_file!(\"unused.rs\"));\n".to_string(),
                ),
                ("src/unused.rs", "pub fn unused() {}\n".to_string()),
            ]
        },
        control_items: SRC_FILE_MACRO,
        control_import: GLOB,
        control_extra: |_| {
            vec![
                (
                    "src/imp.rs",
                    "#[cfg(not(test))]\ninclude!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/unused.rs\"));\n"
                        .to_string(),
                ),
                ("src/unused.rs", "pub fn unused() {}\n".to_string()),
            ]
        },
        control: [
            ExposureClass::WeaklyExposed,
            ExposureClass::PropagationUnknown,
        ],
    }])
}

/// The class of the changed tail of `ready` written in the inline module
/// `real_impl` on lines 1-5 of `src/lib.rs` (its tail on line 3), with
/// `items` after it and a test pinning `ready` through `super::imp::ready`.
fn inline_module_tail_class(
    path: TailPath,
    items: &str,
) -> Result<(ExposureClass, Finding), String> {
    let module = |tail: &str| {
        format!(
            "pub mod real_impl {{\n    {}\n        {tail}\n    }}\n}}\n",
            path.signature()
        )
    };
    let call = match path {
        TailPath::Constant => "assert_eq!(ready(), false);",
        TailPath::Unary => "assert_eq!(ready(5), 5);",
    };
    let lib = format!(
        "{}{items}#[cfg(test)]\nmod tests {{\n    use super::imp::ready;\n    #[test]\n    fn pins() {{\n        {call}\n    }}\n}}\n",
        module(path.new_tail())
    );
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,5 +1,5 @@
 pub mod real_impl {{
     {signature}
-        {old}
+        {new}
     }}
 }}
",
        signature = path.signature(),
        old = path.old_tail(),
        new = path.new_tail(),
    );
    let findings = TempRepo::create(&[("src/lib.rs", lib)], &patch)?.findings()?;
    let tail = path.new_tail();
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.file.ends_with("src/lib.rs")
            && finding.probe.location.line == 3
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok((finding.class.clone(), finding.clone())),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` in src/lib.rs, got {other:?} in {findings:?}"
        )),
    }
}

/// #15035 round-6 review F3: a gated alias of the owner's inline module
/// (`#[cfg(not(test))] pub use real_impl as imp;` beside a test-only `pub
/// use fake_impl as imp;`) swaps which `ready` the test runs, so the
/// enclosing inline module's name gates like a file module's, as does a
/// gated glob of it (`#[cfg(not(test))] pub use crate::real_impl::*;`,
/// round-7 review). Controls: the ungated alias and glob keep main's credit.
#[test]
fn a_gated_alias_of_the_owners_inline_module_is_a_gate() -> Result<(), String> {
    let twin = |path: TailPath| {
        format!(
            "pub mod fake_impl {{\n    {}\n        {}\n    }}\n}}\n#[cfg(not(test))]\npub use real_impl as imp;\n#[cfg(test)]\npub use fake_impl as imp;\n",
            path.signature(),
            path.new_tail()
        )
    };
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        for (name, items, expected) in [
            (
                "gated alias of the inline module",
                twin(path),
                ExposureClass::PropagationUnknown,
            ),
            (
                "ungated alias of the inline module",
                "pub use real_impl as imp;\n".to_string(),
                ExposureClass::WeaklyExposed,
            ),
            (
                "gated glob of the inline module",
                format!(
                    "pub mod fake_impl {{\n    {}\n        {}\n    }}\n}}\npub mod imp {{\n    #[cfg(not(test))]\n    pub use crate::real_impl::*;\n    #[cfg(test)]\n    pub use crate::fake_impl::*;\n}}\n",
                    path.signature(),
                    path.new_tail()
                ),
                ExposureClass::PropagationUnknown,
            ),
            (
                "ungated glob of the inline module",
                "pub mod imp {\n    pub use crate::real_impl::*;\n}\n".to_string(),
                ExposureClass::WeaklyExposed,
            ),
        ] {
            let (class, finding) = inline_module_tail_class(path, &items)?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            if class != expected || credited == (expected == ExposureClass::PropagationUnknown) {
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

/// One round-7 case: the gap (`gap`) must read `propagation_unknown`
/// without the propagation witness on both tail paths; the control must
/// read what `baseline` (the same fixture without the part the fix reads)
/// reads, with the constant tail's propagation credited.
fn check_against_baseline(
    name: &str,
    gap: impl Fn(TailPath) -> Result<(ExposureClass, Finding), String>,
    control: impl Fn(TailPath) -> Result<(ExposureClass, Finding), String>,
    baseline: impl Fn(TailPath) -> Result<(ExposureClass, Finding), String>,
) -> Result<(), String> {
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        let (class, finding) = gap(path)?;
        if class != ExposureClass::PropagationUnknown
            || finding.ripr.propagate.state == StageState::Yes
        {
            wrong.push(format!(
                "{path:?} {name}: {class:?} {:?}",
                finding.ripr.propagate
            ));
        }
        let (class, finding) = control(path)?;
        let (expected, expected_finding) = baseline(path)?;
        let credited = finding.ripr.propagate.state == StageState::Yes;
        if class != expected
            || credited != (expected_finding.ripr.propagate.state == StageState::Yes)
            || matches!(path, TailPath::Constant)
                && (class == ExposureClass::PropagationUnknown || !credited)
        {
            wrong.push(format!(
                "{path:?} {name} control: {class:?} {:?}, baseline {expected:?}",
                finding.ripr.propagate
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// The diff of `src/lib.rs` for one hunk header and body.
fn lib_diff(hunk: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 3333333..4444444 100644
--- a/src/lib.rs
+++ b/src/lib.rs
{hunk}"
    )
}

/// `lib.rs` items for [`a_removed_enclosing_gate_of_an_unchanged_reexport_refuses`].
const API_ITEMS: &str = "mod real;\npub mod api {\n    pub use crate::real::ready;\n}\n";

/// #15035 round-7 review (Devin 4658588280): removing `#[cfg(not(test))]`
/// from `mod api` (and its test-only twin defining `ready` itself) leaves
/// the re-export line unchanged, but its old-side gate let the tests run
/// the twin, so the diff changed which `ready` they reach. Control: the diff
/// removes a non-`cfg` attribute (`#[doc(hidden)]`) from the same module,
/// which keeps the credit an unchanged `lib.rs` has.
#[test]
fn a_removed_enclosing_gate_of_an_unchanged_reexport_refuses() -> Result<(), String> {
    const IMPORT: &str = "use super::api::ready;";
    let lib = |path: TailPath| lib_with(API_ITEMS, IMPORT, path);
    check_against_baseline(
        "removed enclosing cfg of an unchanged re-export",
        |path| {
            let hunk = format!(
                "@@ -1,13 +1,6 @@\n mod real;\n-#[cfg(not(test))]\n pub mod api {{\n     pub use crate::real::ready;\n }}\n-#[cfg(test)]\n-pub mod api {{\n-    {}\n-        {}\n-    }}\n-}}\n #[cfg(test)]\n mod tests {{\n",
                path.signature(),
                path.new_tail()
            );
            tail_class_patched(path, "src/real.rs", lib(path), &[], &lib_diff(&hunk))
        },
        |path| {
            let hunk = "@@ -1,7 +1,6 @@\n mod real;\n-#[doc(hidden)]\n pub mod api {\n     pub use crate::real::ready;\n }\n #[cfg(test)]\n mod tests {\n";
            tail_class_patched(path, "src/real.rs", lib(path), &[], &lib_diff(hunk))
        },
        |path| tail_class_patched(path, "src/real.rs", lib(path), &[], ""),
    )
}

/// #15035 round-7 review (Devin 4658588495): a library's `mod bin;` file
/// `src/bin.rs` is module `bin`, not the `src/bin` target directory, so its
/// owner reads as the same owner in `src/real.rs` does.
#[test]
fn a_library_bin_module_is_a_module() -> Result<(), String> {
    let mut wrong = Vec::new();
    for path in [TailPath::Constant, TailPath::Unary] {
        let (class, finding) =
            tail_class(path, "src/bin.rs", "mod bin;\npub use bin::ready;\n", GLOB)?;
        let (expected, expected_finding) = tail_class(
            path,
            "src/real.rs",
            "mod real;\npub use real::ready;\n",
            GLOB,
        )?;
        let credited = finding.ripr.propagate.state == StageState::Yes;
        if class != expected
            || credited != (expected_finding.ripr.propagate.state == StageState::Yes)
            || matches!(path, TailPath::Constant)
                && (class == ExposureClass::PropagationUnknown || !credited)
        {
            wrong.push(format!(
                "{path:?}: {class:?} {:?}, src/real.rs {expected:?}",
                finding.ripr.propagate
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// `lib.rs` items for [`an_added_sysroot_rename_refuses_only_matching_gates`]
/// (the alias line is added by the diff).
const RENAMED_STD_ITEMS: &str =
    "#![no_std]\nextern crate self as std;\nmod real;\nmod fake;\nmod api;\npub use api::*;\n";

/// #15035 round-7 review (Devin 4658588707): a diff adding `extern crate
/// self as std;` refuses an owner only when a gated `std::` import names
/// its chain or names; without one, the owner keeps the credit an
/// unchanged `lib.rs` has.
#[test]
fn an_added_sysroot_rename_refuses_only_matching_gates() -> Result<(), String> {
    let hunk = "@@ -1,2 +1,3 @@\n #![no_std]\n+extern crate self as std;\n mod real;\n";
    let run = |path: TailPath, gated: bool, patch: &str| {
        let api = std_api(gated);
        tail_class_patched(
            path,
            "src/real.rs",
            lib_with(RENAMED_STD_ITEMS, GLOB, path),
            &[("src/api.rs", api.as_str())],
            patch,
        )
    };
    check_against_baseline(
        "added sysroot rename with a gated std import",
        |path| run(path, true, &lib_diff(hunk)),
        |path| run(path, false, &lib_diff(hunk)),
        |path| run(path, false, ""),
    )
}

/// The class of the changed tail of `ready` on lines 1-3 of the crate root
/// `src/lib.rs`, after `items` and a test module pinning it, with
/// `lib_hunk` (a hunk of the same file after the owner's) in the diff.
fn root_tail_class(
    path: TailPath,
    items: &str,
    lib_hunk: &str,
) -> Result<(ExposureClass, Finding), String> {
    let lib = format!(
        "{}{items}#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pins() {{\n        {}\n    }}\n}}\n",
        path.owner(),
        path.pin()
    );
    let patch = format!("{}{lib_hunk}", path.patch("src/lib.rs"));
    let findings = TempRepo::create(&[("src/lib.rs", lib)], &patch)?.findings()?;
    let tail = path.new_tail();
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.file.ends_with("src/lib.rs")
            && finding.probe.location.line == 2
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok((finding.class.clone(), finding.clone())),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` in src/lib.rs, got {other:?} in {findings:?}"
        )),
    }
}

/// A module-emitting macro after a three-line owner, naming `module`.
fn declaring_macro(module: &str) -> String {
    format!("macro_rules! declare {{\n    ($m:ident) => {{\n        mod {module};\n    }};\n}}\n")
}

/// #15035 round-7 review (Devin 4658588614): editing an unused
/// module-emitting macro does not refuse a crate-root owner, which no
/// macro-emitted declaration can reach (as for an unchanged macro). The
/// same edit still refuses an owner in a module file.
#[test]
fn a_changed_macro_leaves_crate_root_owners_credited() -> Result<(), String> {
    let mut wrong = Vec::new();
    // Lines 4-8 of `lib.rs` hold the macro; line 6 its `mod`.
    let hunk = "@@ -6,1 +6,1 @@\n-        mod extra;\n+        mod new_extra;\n";
    for path in [TailPath::Constant, TailPath::Unary] {
        let (class, finding) = root_tail_class(path, &declaring_macro("new_extra"), hunk)?;
        let (expected, expected_finding) =
            root_tail_class(path, &declaring_macro("new_extra"), "")?;
        let credited = finding.ripr.propagate.state == StageState::Yes;
        if class != expected
            || credited != (expected_finding.ripr.propagate.state == StageState::Yes)
            || matches!(path, TailPath::Constant)
                && (class == ExposureClass::PropagationUnknown || !credited)
        {
            wrong.push(format!(
                "{path:?} crate root: {class:?} {:?}, unchanged macro {expected:?}",
                finding.ripr.propagate
            ));
        }
        // `mod real;` on line 1, the macro's `mod` on line 5.
        let items = format!(
            "mod real;\npub use real::ready;\n{}",
            declaring_macro("new_extra")
        );
        let module_hunk = "@@ -5,1 +5,1 @@\n-        mod extra;\n+        mod new_extra;\n";
        let (class, finding) = tail_class_patched(
            path,
            "src/real.rs",
            lib_with(&items, GLOB, path),
            &[],
            &lib_diff(module_hunk),
        )?;
        if class != ExposureClass::PropagationUnknown
            || finding.ripr.propagate.state == StageState::Yes
        {
            wrong.push(format!(
                "{path:?} module file: {class:?} {:?}",
                finding.ripr.propagate
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// `src/imp.rs` including `src/unused.rs` through `concat` (spelled
/// `concat`), with that file.
fn concat_include(concat: &'static str) -> Vec<(&'static str, String)> {
    vec![
        (
            "src/imp.rs",
            format!("#[cfg(not(test))]\ninclude!({concat}!(\"unused.rs\"));\n"),
        ),
        ("src/unused.rs", "pub fn unused() {}\n".to_string()),
    ]
}

/// A user `concat` macro expanding to the owner file whatever its
/// argument.
const SHADOWING_CONCAT: &str = "macro_rules! concat {\n    ($n:literal) => {\n        ::core::concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\")\n    };\n}\nmod real;\npub use real::ready;\npub mod imp;\n";

/// #15035 round-7 review: a bare `concat!` in an `include!` is the built-in
/// only when no workspace macro named `concat` shadows it, and a
/// `core::concat!` only when no crate is renamed to `core`. Each gap's
/// `include!` may expand to the owner file, so it gates every owner.
/// Controls: the same `include!` without the shadowing macro or rename
/// keeps main's credit.
#[test]
fn a_shadowed_concat_include_path_is_opaque() -> Result<(), String> {
    const CREDITED: [ExposureClass; 2] = [
        ExposureClass::WeaklyExposed,
        ExposureClass::PropagationUnknown,
    ];
    check_gap_cases(vec![
        GapCase {
            name: "bare concat shadowed by a user macro",
            owner: "src/real.rs",
            items: SHADOWING_CONCAT,
            import: GLOB,
            extra: |_| concat_include("concat"),
            control_items: OWNER_AND_IMP,
            control_import: GLOB,
            control_extra: |_| concat_include("concat"),
            control: CREDITED,
        },
        GapCase {
            name: "core concat under a crate renamed to core",
            owner: "src/real.rs",
            items: "extern crate self as core;\nmod real;\npub use real::ready;\npub mod imp;\n",
            import: GLOB,
            extra: |_| concat_include("core::concat"),
            control_items: OWNER_AND_IMP,
            control_import: GLOB,
            control_extra: |_| concat_include("core::concat"),
            control: CREDITED,
        },
    ])
}
