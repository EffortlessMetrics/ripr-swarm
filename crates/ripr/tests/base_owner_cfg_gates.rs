//! Public-API acceptance for #14952 and #15035: a `cfg` gate that lives
//! outside a `mod` declaration (`include!`, a `pub use` re-export, a
//! macro-emitted `mod`) must keep the shared base-owner identity from
//! crediting the changed function on both whole-body tail paths that rely on
//! it: the constant tail (#14852, `true` to `false`) and the unary tail
//! (#14815, `-x` to `x`).
//!
//! Each gated shape lets the tests run a stand-in (`fake::stub`) returning
//! the new value on both sides, so it must read `propagation_unknown`
//! without the propagation witness; each control keeps main's class.

use ripr::ExposureClass::{
    self, Exposed as E, InfectionUnknown as IU, PropagationUnknown as PU, WeaklyExposed as W,
};
use ripr::domain::StageState;
use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
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

    /// An exact pin of the new value of `callee`, where the versions differ.
    fn pin_of(self, callee: &str) -> String {
        match self {
            Self::Constant => format!("assert_eq!({callee}(), false);"),
            Self::Unary => format!("assert_eq!({callee}(5), 5);"),
        }
    }

    fn pin(self) -> String {
        self.pin_of("ready")
    }

    /// The changed owner's file: the new tail on line 2.
    fn owner(self) -> String {
        format!("{}\n    {}\n}}\n", self.signature(), self.new_tail())
    }

    /// The test-only stand-in, returning the new value on both sides.
    fn stub(self) -> String {
        self.owner().replace("fn ready(", "fn stub(")
    }

    /// The diff of `file` changing the tail after the `before` lines
    /// (starting at line `first`, the tail indented four spaces per line
    /// of `before`) from the old value to the new.
    fn diff(self, file: &str, first: usize, before: &[&str], after: &[&str]) -> String {
        let count = before.len() + after.len() + 1;
        let indent = " ".repeat(4 * before.len());
        let mut text = format!(
            "diff --git a/{file} b/{file}\nindex 1111111..2222222 100644\n--- a/{file}\n+++ b/{file}\n@@ -{first},{count} +{first},{count} @@\n"
        );
        for line in before {
            text.push_str(&format!(" {line}\n"));
        }
        text.push_str(&format!(
            "-{indent}{}\n+{indent}{}\n",
            self.old_tail(),
            self.new_tail()
        ));
        for line in after {
            text.push_str(&format!(" {line}\n"));
        }
        text
    }

    /// The diff that changes line 2 of `file` from the old tail to the new.
    fn patch(self, file: &str) -> String {
        self.diff(file, 1, &[self.signature()], &["}"])
    }
}

/// The class and finding of the one changed `return_value` tail at
/// `file:line`, after writing `files` and `patch`.
fn class_at(
    path: TailPath,
    files: &[(&str, String)],
    patch: &str,
    file: &str,
    line: usize,
) -> Result<(ExposureClass, Finding), String> {
    let findings = TempRepo::create(files, patch)?.findings()?;
    let tail = path.new_tail();
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.file.ends_with(file)
            && finding.probe.location.line == line
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok((finding.class.clone(), finding.clone())),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` in {file}, got {other:?} in {findings:?}"
        )),
    }
}

/// A `#[cfg(test)]` module with `import` and one test running `pin`.
fn tests_module(import: &str, pin: &str) -> String {
    format!(
        "#[cfg(test)]\nmod tests {{\n    {import}\n    #[test]\n    fn pins() {{\n        {pin}\n    }}\n}}\n"
    )
}

/// `src/lib.rs`: `items`, then a test module pinning `ready` through
/// `import`.
fn lib_with(items: &str, import: &str, path: TailPath) -> String {
    format!("{items}\n{}", tests_module(import, &path.pin()))
}

/// Files beside the four [`tail_class_patched`] writes, for a tail path.
type Extra = fn(TailPath) -> Vec<(&'static str, String)>;

fn none(_: TailPath) -> Vec<(&'static str, String)> {
    Vec::new()
}

/// The class of the changed tail in `owner_file` with `lib` as
/// `src/lib.rs`, plus `src/fake.rs` (the stand-in), `src/other.rs` and
/// `extra` (written last), and `lib_patch` (a diff of other files, possibly
/// empty) after the owner's diff.
fn tail_class_patched(
    path: TailPath,
    owner_file: &str,
    lib: String,
    extra: Vec<(&str, String)>,
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
    files.extend(extra);
    let patch = format!("{}{lib_patch}", path.patch(owner_file));
    class_at(path, &files, &patch, owner_file, 2)
}

/// One fixture read on a tail path.
type Run = Box<dyn Fn(TailPath) -> Result<(ExposureClass, Finding), String>>;

/// The [`tail_class_patched`] fixture with `items` and the test module's
/// `import` in `src/lib.rs`.
fn lib(owner: &'static str, items: impl Into<String>, import: &'static str, extra: Extra) -> Run {
    let items = items.into();
    Box::new(move |path| {
        tail_class_patched(path, owner, lib_with(&items, import, path), extra(path), "")
    })
}

/// [`lib`] with the owner `src/real.rs`, no extra files and the glob import.
fn real(items: impl Into<String>) -> Run {
    lib("src/real.rs", items, GLOB, none)
}

/// What a row must read on each tail path.
enum Want {
    /// This class on the constant and the unary path, with the propagation
    /// witness exactly when the class is not `propagation_unknown`.
    Is([ExposureClass; 2]),
    /// What the baseline fixture reads (class and witness), with the
    /// constant tail's witness credited.
    Like(Run),
}

struct Row {
    name: String,
    run: Run,
    want: Want,
}

fn row(name: &str, run: Run, want: [ExposureClass; 2]) -> Vec<Row> {
    vec![Row {
        name: name.to_string(),
        run,
        want: Want::Is(want),
    }]
}

/// A gap: `propagation_unknown` without the witness on both paths.
fn gap(name: &str, run: Run) -> Vec<Row> {
    row(name, run, [PU, PU])
}

/// A gap and its control, which reads `control`.
fn pair(name: &str, gap_run: Run, control_run: Run, control: [ExposureClass; 2]) -> Vec<Row> {
    let mut rows = gap(name, gap_run);
    rows.extend(row(&format!("{name} control"), control_run, control));
    rows
}

fn like(name: &str, run: Run, baseline: Run) -> Vec<Row> {
    vec![Row {
        name: name.to_string(),
        run,
        want: Want::Like(baseline),
    }]
}

/// The rows of one test, as data: each entry is a [`row`], [`gap`],
/// [`pair`] or [`like`].
macro_rules! rows {
    ($($rows:expr),* $(,)?) => {{
        let mut all: Vec<Row> = Vec::new();
        $(all.extend($rows);)*
        all
    }};
}

fn check(rows: Vec<Row>) -> Result<(), String> {
    let mut wrong = Vec::new();
    for (index, path) in [TailPath::Constant, TailPath::Unary]
        .into_iter()
        .enumerate()
    {
        for row in &rows {
            let (class, finding) = (row.run)(path)?;
            let credited = finding.ripr.propagate.state == StageState::Yes;
            let right = match &row.want {
                Want::Is(classes) => class == classes[index] && credited != (class == PU),
                Want::Like(baseline) => {
                    let (expected, base) = baseline(path)?;
                    class == expected
                        && credited == (base.ripr.propagate.state == StageState::Yes)
                        && (index == 1 || (class != PU && credited))
                }
            };
            if !right {
                wrong.push(format!(
                    "{path:?} {}: {class:?} {:?}",
                    row.name, finding.ripr.propagate
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// The test module's import of everything its parent names.
const GLOB: &str = "use super::*;";

/// The test module's direct import of the owner.
const DIRECT: &str = "use crate::real::ready;";

/// The test-only stand-in exported under the owner's name.
const STAND_IN: &str = "#[cfg(test)]\nmod fake;\n#[cfg(test)]\npub use fake::stub as ready;\n";

/// A macro emitting a gated out-of-line `mod $m;`.
const DECLARE: &str = "macro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m;\n    };\n}\n";

/// #14952 acceptance: the issue's four gated shapes read
/// `propagation_unknown` on both tail paths (main credited all four on the
/// constant tail).
#[test]
fn gated_shapes_outside_mod_declarations_read_propagation_unknown() -> Result<(), String> {
    check(rows! {
        gap("inc_cfg", real(format!("#[cfg(not(test))]\ninclude!(\"real.rs\");\n{STAND_IN}"))),
        gap("inc_mod", lib("src/real_impl.rs", format!("#[cfg(not(test))]\nmod real {{\n    include!(\"real_impl.rs\");\n}}\n#[cfg(not(test))]\npub use real::ready;\n{STAND_IN}"), GLOB, none)),
        gap("cfg_use", real(format!("mod real;\n#[cfg(not(test))]\npub use real::ready;\n{STAND_IN}"))),
        gap("macro_mod_decl", real(format!("{DECLARE}declare!(real);\n#[cfg(not(test))]\npub use real::ready;\n{STAND_IN}"))),
    })
}

/// #14952 controls (`pos_plain`, `pos_unrelated_mod`,
/// `pos_unrelated_gated`) keep their credit on both paths: `exposed`
/// through a direct import, `weakly_exposed` through an ungated re-export.
#[test]
fn ungated_controls_keep_their_credit_on_both_tail_paths() -> Result<(), String> {
    let direct = |items: &'static str| lib("src/real.rs", items, DIRECT, none);
    check(rows! {
        row("pos_plain", direct("mod real;\n"), [E, E]),
        row("pos_unrelated_mod", direct("mod real;\n#[cfg(test)]\nmod other;\n"), [E, E]),
        row("pos_unrelated_gated", direct("mod real;\nmod other;\n#[cfg(not(test))]\npub use other::g;\n"), [E, E]),
        row("pos_plain re-export", real("mod real;\npub use real::ready;\n"), [W, W]),
        row("pos_unrelated_mod re-export", real("mod real;\npub use real::ready;\n#[cfg(test)]\nmod other;\n"), [W, W]),
        row("pos_unrelated_gated re-export", real("mod real;\npub use real::ready;\nmod other;\n#[cfg(not(test))]\npub use other::g;\n"), [W, W]),
    })
}

/// #14952 review R1: only a macro that may emit an out-of-line module (or an
/// `include!` or `#[path]`) in a transcriber gates, and only owners in the
/// package defining it unless it is exported.
#[test]
fn macro_gates_need_an_out_of_line_module_in_the_owners_package() -> Result<(), String> {
    let direct = |items: &'static str, extra: Extra| lib("src/real.rs", items, DIRECT, extra);
    check(rows! {
        row("inline-only macro", direct("mod real;\nmacro_rules! wrap {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m {\n            pub fn helper() {}\n        }\n    };\n}\nwrap!(inner);\n", none), [E, PU]),
        row("mod-emitting macro in another package", direct("mod real;\n", |_| vec![("other/src/lib.rs", DECLARE.to_string())]), [E, PU]),
        gap("exported mod-emitting macro in another package", direct("mod real;\n", |_| vec![("other/src/lib.rs", format!("#[macro_export]\n{DECLARE}"))])),
        gap("mod-emitting macro in the owner's package", lib("src/real.rs", format!("mod real;\n{DECLARE}"), DIRECT, none)),
        gap("include-emitting macro in the owner's package", direct("mod real;\nmacro_rules! inc {\n    ($f:literal) => {\n        include!($f);\n    };\n}\n", none)),
        row("mod only in a macro matcher", direct("mod real;\nmacro_rules! m {\n    (mod real;) => {\n        fn helper() {}\n    };\n}\n", none), [E, PU]),
    })
}

/// #14952 review R2: a private `#[cfg(unix)] use super::ready;` inside the
/// `#[cfg(test)]` module, where nothing else can bind `ready`, is not a
/// gate; a gated rename there, the same import beside another glob, and a
/// private gated rename at file scope are.
#[test]
fn a_redundant_gated_test_import_is_not_a_gate() -> Result<(), String> {
    const ITEMS: &str = "mod real;\nmod fake;\npub use real::ready;\n";
    let test_import =
        |items: &'static str, import: &'static str| lib("src/real.rs", items, import, none);
    check(rows! {
        row("gated super import in the test module", test_import("mod real;\npub use real::ready;\n", "use super::*;\n    #[cfg(unix)]\n    use super::ready;"), [W, W]),
        gap("gated rename in the test module", test_import(ITEMS, "use super::*;\n    #[cfg(unix)]\n    use super::fake::stub as ready;")),
        gap("gated super import beside another glob", lib("src/real.rs", ITEMS, "use crate::fake::*;\n    #[cfg(unix)]\n    use super::ready;", |path| vec![("src/fake.rs", path.stub() + "pub use self::stub as ready;\n")])),
        gap("private gated rename at file scope", real(format!("{ITEMS}#[cfg(test)]\nuse fake::stub as ready;\n"))),
    })
}

/// #14952 review R3: a gate must name a crate-local path. Sysroot-rooted
/// uses and an `include!` of another file with the owner's stem keep main's
/// class; the same names through crate-local roots, and an `include!` of the
/// owner's own file, gate.
#[test]
fn gates_must_name_crate_local_paths() -> Result<(), String> {
    const PRELUDE: &str = "use crate::prelude::ready;";
    let direct = |items: &'static str| lib("src/real.rs", items, DIRECT, none);
    let prelude = |items: &'static str| lib("src/prelude.rs", items, PRELUDE, none);
    check(rows! {
        row("std rename to the owner's name", direct("mod real;\n#[cfg(unix)]\nuse std::fmt::Write as ready;\n"), [W, PU]),
        row("std glob over a module named like the owner file", prelude("mod prelude;\n#[cfg(unix)]\nuse std::io::prelude::*;\n"), [E, E]),
        row("include of another file with the owner's stem", lib("src/real.rs", "mod real;\n#[cfg(unix)]\nmod data {\n    include!(\"../data/real.rs\");\n}\n", DIRECT, |_| vec![("data/real.rs", "pub fn other() {}\n".to_string())]), [E, PU]),
        row("include of another file named across split literals", direct("mod real;\nmod generated {\n    include!(concat!(env!(\"OUT_DIR\"), \"/\", \"gen\", \".rs\"));\n}\n"), [E, PU]),
        gap("self rename to the owner's name", direct("mod real;\nmod fake;\n#[cfg(unix)]\nuse self::fake::stub as ready;\n")),
        gap("bare local rename to the owner's name", direct("mod real;\nmod fake;\n#[cfg(unix)]\nuse fake::stub as ready;\n")),
        gap("self glob over the owner's module", prelude("mod prelude;\n#[cfg(unix)]\npub use self::prelude::*;\n")),
        gap("include resolving to the owner's file", direct("mod real;\n#[cfg(unix)]\nmod shim {\n    include!(\"real.rs\");\n}\n")),
        gap("include of the owner file in a Unicode case variant", lib("src/kit.rs", "mod kit;\n#[cfg(unix)]\nmod shim {\n    include!(\"\u{212A}it.rs\");\n}\n", "use crate::kit::ready;", none)),
        gap("include of the owner file in a long-s case variant", lib("src/ser.rs", "mod ser;\n#[cfg(unix)]\nmod shim {\n    include!(\"\u{17F}er.rs\");\n}\n", "use crate::ser::ready;", none)),
    })
}

/// A module whose `ready` is the test-only stand-in.
fn alias_stand_in(_: TailPath) -> Vec<(&'static str, String)> {
    vec![(
        "src/alt.rs",
        "pub use crate::fake::stub as ready;\n".to_string(),
    )]
}

const ALIAS_IMPORT: &str = "use super::imp::ready;";

/// [`lib`] for `src/real.rs` reached through `super::imp::ready`, with the
/// stand-in module `src/alt.rs`.
fn aliased(items: &'static str) -> Run {
    lib("src/real.rs", items, ALIAS_IMPORT, alias_stand_in)
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

/// #15035 review F1: a gated `use` of a module (a plain alias, a `pub use
/// crate::..` alias, an alias in a `{..}` list or of `self`, an alias chain,
/// or same-named modules in two directories) gates every owner in the
/// modules it may alias.
#[test]
fn a_gated_module_alias_is_a_gate() -> Result<(), String> {
    const CREDITED: [ExposureClass; 2] = [W, PU];
    let two = |items: &'static str| lib("src/a/imp.rs", items, ALIAS_IMPORT, two_directories);
    check(rows! {
        pair("gated module alias",
            aliased("mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse real as imp;\n#[cfg(test)]\nuse alt as imp;\n"),
            aliased("mod real;\nmod fake;\nmod alt;\nuse real as imp;\n"), CREDITED),
        pair("gated pub crate module alias",
            aliased("pub mod real;\nmod fake;\npub mod alt;\n#[cfg(not(test))]\npub use crate::real as imp;\n#[cfg(test)]\npub use crate::alt as imp;\n"),
            aliased("pub mod real;\nmod fake;\npub mod alt;\npub use crate::real as imp;\n"), CREDITED),
        pair("gated module alias in a use list",
            aliased("mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse crate::{real as imp};\n#[cfg(test)]\nuse crate::{alt as imp};\n"),
            aliased("mod real;\nmod fake;\nmod alt;\nuse crate::{real as imp};\n"), CREDITED),
        pair("gated self alias in a use list",
            aliased("mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nuse self::real::{self as imp};\n#[cfg(test)]\nuse self::alt::{self as imp};\n"),
            aliased("mod real;\nmod fake;\nmod alt;\nuse self::real::{self as imp};\n"), CREDITED),
        pair("gated alias behind an ungated alias chain",
            aliased("pub mod real;\nmod fake;\npub mod alt;\nmod sel {\n    #[cfg(not(test))]\n    pub use crate::real as inner;\n    #[cfg(test)]\n    pub use crate::alt as inner;\n}\nuse sel::inner as imp;\n"),
            aliased("pub mod real;\nmod fake;\npub mod alt;\nmod sel {\n    pub use crate::real as inner;\n}\nuse sel::inner as imp;\n"), CREDITED),
        pair("gated import of same-named modules in two directories",
            two("mod a;\nmod b;\nmod fake;\n#[cfg(not(test))]\nuse a::imp;\n#[cfg(test)]\nuse b::imp;\n"),
            two("mod a;\nmod b;\nmod fake;\nuse a::imp;\n"), [E, PU]),
    })
}

/// `src/real.rs` with a second item, `Helper`, beside the changed tail,
/// plus `src/other.rs` holding `other` when given.
fn with_helper(path: TailPath, other: Option<&str>) -> Vec<(&'static str, String)> {
    let mut files = vec![(
        "src/real.rs",
        format!("{}pub struct Helper;\n", path.owner()),
    )];
    files.extend(other.map(|other| ("src/other.rs", other.to_string())));
    files
}

/// #15035 round-5 review: a gated `use` gates a module only through the
/// segments by which it may alias one; an intermediate segment only names
/// the module its leaf lives in. Each shape sits beside its gated
/// counterpart naming the owner, which still gates.
#[test]
fn only_aliasing_segments_of_a_gated_use_gate() -> Result<(), String> {
    const OWNER_REEXPORT: &str = "mod real;\nmod other;\npub use real::ready;\n";
    let other = |extra: Extra| lib("src/real.rs", OWNER_REEXPORT, GLOB, extra);
    let helper =
        |items: &'static str| lib("src/real.rs", items, GLOB, |path| with_helper(path, None));
    check(rows! {
        row("gated import of another item of the owner's module",
            other(|path| with_helper(path, Some("#[cfg(test)]\nuse crate::real::Helper;\npub fn o() {}\n"))), [W, W]),
        gap("same import naming the owner",
            other(|path| with_helper(path, Some("#[cfg(test)]\npub use crate::real::ready;\npub fn o() {}\n")))),
        row("third-party path through a segment named like the owner's module",
            other(|path| with_helper(path, Some("#[cfg(feature = \"serde\")]\nuse serde_like::real::Thing;\npub fn o() {}\n"))), [W, W]),
        gap("same third-party path ending in the owner's module",
            other(|path| with_helper(path, Some("#[cfg(feature = \"serde\")]\nuse serde_like::real;\npub fn o() {}\n")))),
        row("gated re-export of another item of the owner's module",
            helper("mod real;\npub use real::ready;\n#[cfg(feature = \"x\")]\npub use real::Helper;\n"), [W, W]),
        gap("same re-export naming the owner",
            helper("mod real;\n#[cfg(feature = \"x\")]\npub use real::ready;\npub use real::Helper;\n")),
    })
}

/// The changed tail of `Thing::ready` in `src/real.rs` (an inherent method,
/// its tail on line 4), with `items` in `src/lib.rs`, a test calling
/// `Thing::ready` through `use super::*`, and the test-only `src/fake.rs`
/// twin of `Thing`.
fn impl_tail(items: &'static str) -> Run {
    Box::new(move |path| {
        let method = format!(
            "pub struct Thing;\nimpl Thing {{\n    {}\n        {}\n    }}\n}}\n",
            path.signature(),
            path.new_tail()
        );
        let lib = format!(
            "{items}{}",
            tests_module(GLOB, &path.pin_of("Thing::ready"))
        );
        let signature = format!("    {}", path.signature());
        let patch = path.diff(
            "src/real.rs",
            2,
            &["impl Thing {", signature.as_str()],
            &["    }", "}"],
        );
        let files = [
            ("src/lib.rs", lib),
            ("src/real.rs", method.clone()),
            ("src/fake.rs", method),
        ];
        class_at(path, &files, &patch, "src/real.rs", 4)
    })
}

/// #15035 round-5 review: a gated swap of the owner's impl self type
/// (`pub use real::Thing;` beside a test-only `pub use fake::Thing;`) gates
/// the owner. Control: the ungated re-export keeps main's credit.
#[test]
fn a_gated_swap_of_the_owners_self_type_is_a_gate() -> Result<(), String> {
    check(rows! {
        pair("gated self-type twin",
            impl_tail("mod real;\nmod fake;\n#[cfg(not(test))]\npub use real::Thing;\n#[cfg(test)]\npub use fake::Thing;\n"),
            impl_tail("mod real;\nmod fake;\npub use real::Thing;\n"), [W, PU]),
    })
}

/// #15035 review F2: a module-level macro call in the `#[cfg(test)]` module
/// may emit the import that rebinds `ready`, so the gated `use
/// super::ready;` beside it stays a gate.
#[test]
fn a_macro_call_in_the_test_module_keeps_the_gate() -> Result<(), String> {
    const ITEMS: &str = "mod real;\nmod fake;\npub use real::ready;\nmacro_rules! test_imports {\n    () => {\n        #[cfg(not(unix))]\n        use crate::fake::stub as ready;\n    };\n}\n";
    check(rows! {
        pair("gated super import beside a module-level macro call",
            lib("src/real.rs", ITEMS, "use super::*;\n    #[cfg(unix)]\n    use super::ready;\n    test_imports!();", none),
            lib("src/real.rs", ITEMS.replace("as ready", "as other_ready"), "use super::*;\n    use super::ready;\n    test_imports!();", none), [W, W]),
    })
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
    check(rows! {
        pair("extern crate self as std",
            lib("src/real.rs", ITEMS, GLOB, |_| vec![("src/api.rs", std_api(true))]),
            lib("src/real.rs", ITEMS, GLOB, |_| vec![("src/api.rs", std_api(false))]), [W, PU]),
    })
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
        "{prefix}{imports}const D: &str = \"\";\npub fn ready_api() -> i32 {{ 0 }}\n{}",
        tests_module(GLOB, &path.pin())
    )
}

/// The [`private_api`] fixture: only `src/api.rs`'s test reaches the owner
/// (no root re-export of another name, which the unary reading refuses).
fn private(extra: Extra) -> Run {
    let lib = "mod real;\nmod fake;\nmod api;\n".to_string();
    Box::new(move |path| tail_class_patched(path, "src/real.rs", lib.clone(), extra(path), ""))
}

/// #15035 review F4 and round-8 review: a `;` inside a string literal of the
/// `cfg` predicate, or a `{` inside a later attribute, does not hide a
/// private gated import from the caller's prefilter.
#[test]
fn a_semicolon_in_a_cfg_string_does_not_hide_the_gate() -> Result<(), String> {
    check(rows! {
        pair("private gated import under a string predicate holding a semicolon",
            private(|path| vec![("src/api.rs", private_api(path, "", Some("feature = \"x;y\"")))]),
            private(|path| vec![("src/api.rs", private_api(path, "", None))]), [E, E]),
        pair("gated import after an attribute holding a brace, in a file with no `mod`",
            lib("src/real.rs", OWNER_AND_IMP, GLOB, |_| vec![("src/imp.rs", "#[cfg(not(test))]\n#[doc = stringify!({})]\nuse crate::real::ready;\n".to_string())]),
            lib("src/real.rs", OWNER_AND_IMP, GLOB, |_| vec![("src/imp.rs", "#[doc = stringify!({})]\nuse crate::real::ready;\n".to_string())]), [W, W]),
    })
}

const CR: &str = "const C: &core::ffi::CStr = cr\"\\\";\n";
const CR_HASH: &str = "const C: &core::ffi::CStr = cr#\"say \"hi\"\"#;\n";

/// #15035 round-5 review: a raw C string (`cr"\"`, `cr#".."#`) before a
/// private gated import is lexed as raw, so its backslash does not hide the
/// gate from the caller's prefilter.
#[test]
fn a_raw_c_string_does_not_hide_the_gate() -> Result<(), String> {
    check(rows! {
        pair("raw C string ending in a backslash",
            private(|path| vec![("src/api.rs", private_api(path, CR, Some("feature = \"x\"")))]),
            private(|path| vec![("src/api.rs", private_api(path, CR, None))]), [E, E]),
        pair("raw C string with hashes",
            private(|path| vec![("src/api.rs", private_api(path, CR_HASH, Some("feature = \"x\"")))]),
            private(|path| vec![("src/api.rs", private_api(path, CR_HASH, None))]), [E, E]),
    })
}

/// The control of the `include!` gaps: an ungated `imp` module re-exporting
/// the owner without an `include!` (one naming the owner file gates whether
/// or not it is gated, since it compiles the file a second time).
const INCLUDE_CONTROL: &str =
    "mod real;\nmod fake;\nmod alt;\nmod imp {\n    pub use crate::real::ready;\n}\n";

/// #15035 review F5: an `include!` whose path is not a single literal
/// (`concat!(env!("CARGO_MANIFEST_DIR"), "/src/real.rs")`) in a cfg-twin
/// module gates the file its trailing literals name, joined when they split
/// the file name (`"/src/re", "al.rs"`, round 5).
#[test]
fn a_non_literal_include_is_a_gate() -> Result<(), String> {
    check(rows! {
        pair("cfg-twin modules including through concat",
            aliased("mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\"));\n}\n#[cfg(test)]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/alt.rs\"));\n}\n"),
            aliased(INCLUDE_CONTROL), [E, PU]),
        pair("cfg-twin modules including a file name split across literals",
            aliased("mod real;\nmod fake;\nmod alt;\n#[cfg(not(test))]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/re\", \"al.rs\"));\n}\n#[cfg(test)]\nmod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/alt.rs\"));\n}\n"),
            aliased(INCLUDE_CONTROL), [E, PU]),
    })
}

/// The changed tail of `src/bin/helper/mod.rs` (module `helper` of the bin
/// `src/bin/app.rs`, whose text is `app`), pinned by a test in `app`
/// through `super::helper::ready`. `@TWIN@` in `app` becomes a test-only
/// inline `helper` twin.
fn bin_module(app: &'static str) -> Run {
    const OWNER: &str = "src/bin/helper/mod.rs";
    Box::new(move |path| {
        let twin = format!(
            "#[cfg(test)]\nmod helper {{\n    {}\n        {}\n    }}\n}}\n",
            path.signature(),
            path.new_tail()
        );
        let app = format!(
            "{}fn main() {{}}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn pins() {{\n        {}\n    }}\n}}\n",
            app.replace("@TWIN@", &twin),
            path.pin_of("super::helper::ready")
        );
        let files = [
            ("src/lib.rs", "pub fn other() {}\n".to_string()),
            ("src/bin/app.rs", app),
            (OWNER, path.owner()),
        ];
        class_at(path, &files, &path.patch(OWNER), OWNER, 2)
    })
}

/// #15035 review: a target directory's `x/mod.rs` (`src/bin/helper/mod.rs`,
/// `tests/common/mod.rs`) is module `x`, not a crate root. Gap: the bin
/// declares `helper` only through a macro emitting a gated `mod $m;`,
/// beside a test-only inline twin. Control: a plain `mod helper;`.
#[test]
fn a_target_directory_mod_file_is_not_a_crate_root() -> Result<(), String> {
    check(rows! {
        pair("macro-declared module with a test-only twin",
            bin_module("macro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        mod $m;\n    };\n}\ndeclare!(helper);\n@TWIN@"),
            bin_module("mod helper;\n"), [W, IU]),
    })
}

/// `lib.rs` items re-exporting the owner, with `src/imp.rs` (no `mod`
/// keyword, so the caller keeps it only through the outside-gate prefilter)
/// as a module of its own.
const OWNER_AND_IMP: &str = "mod real;\npub use real::ready;\npub mod imp;\n";

/// #15035 round-6 review F1: a comment between `include` and its `!` in a
/// file with no `mod` keyword does not hide the gated `include!` of the
/// owner file. Control: the same spelling including another file.
#[test]
fn a_comment_inside_an_include_call_does_not_hide_it() -> Result<(), String> {
    check(rows! {
        pair("gated include of the owner file spelled with a comment",
            lib("src/real.rs", OWNER_AND_IMP, GLOB, |_| vec![("src/imp.rs", "#[cfg(not(test))]\ninclude /* real */ !(\"real.rs\");\n".to_string())]),
            lib("src/real.rs", OWNER_AND_IMP, GLOB, |_| vec![("src/imp.rs", "#[cfg(not(test))]\ninclude /* other */ !(\"fake.rs\");\n".to_string())]), [W, PU]),
    })
}

/// `lib.rs` items with a `src_file!` macro that expands to the owner file
/// whatever its argument.
const SRC_FILE_MACRO: &str = "#[macro_use]\nmod paths {\n    macro_rules! src_file {\n        ($name:literal) => {\n            concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\")\n        };\n    }\n}\nmod real;\npub use real::ready;\npub mod imp;\n";

/// `src/imp.rs` holding `imp` and the file `src/unused.rs`.
fn imp_and_unused(imp: String) -> Vec<(&'static str, String)> {
    vec![
        ("src/imp.rs", imp),
        ("src/unused.rs", "pub fn unused() {}\n".to_string()),
    ]
}

/// #15035 round-6 review F2: only `concat!`'s own trailing literals are
/// trusted as an `include!` path; a user macro's argument may expand to any
/// path, so it is an opaque include that gates every owner. Control: the
/// same file named through `concat!`.
#[test]
fn a_user_macro_include_path_is_opaque() -> Result<(), String> {
    check(rows! {
        pair("gated include through a user macro",
            lib("src/real.rs", SRC_FILE_MACRO, GLOB, |_| imp_and_unused("#[cfg(not(test))]\ninclude!(src_file!(\"unused.rs\"));\n".to_string())),
            lib("src/real.rs", SRC_FILE_MACRO, GLOB, |_| imp_and_unused("#[cfg(not(test))]\ninclude!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/unused.rs\"));\n".to_string())), [W, PU]),
    })
}

/// The changed tail of `ready` in the inline module `real_impl` on lines
/// 1-5 of `src/lib.rs` (its tail on line 3), with `items` after it and a
/// test pinning `ready` through `super::imp::ready`.
fn inline_module(items: fn(TailPath) -> String) -> Run {
    Box::new(move |path| {
        let lib = format!(
            "pub mod real_impl {{\n    {}\n        {}\n    }}\n}}\n{}{}",
            path.signature(),
            path.new_tail(),
            items(path),
            tests_module("use super::imp::ready;", &path.pin())
        );
        let signature = format!("    {}", path.signature());
        let patch = path.diff(
            "src/lib.rs",
            1,
            &["pub mod real_impl {", signature.as_str()],
            &["    }", "}"],
        );
        class_at(path, &[("src/lib.rs", lib)], &patch, "src/lib.rs", 3)
    })
}

/// The test-only inline twin `fake_impl` of `real_impl`.
fn fake_impl(path: TailPath) -> String {
    format!(
        "pub mod fake_impl {{\n    {}\n        {}\n    }}\n}}\n",
        path.signature(),
        path.new_tail()
    )
}

/// #15035 round-6 review F3 and round-7 review: a gated alias or glob of
/// the owner's inline module (`#[cfg(not(test))] pub use real_impl as imp;`,
/// `pub use crate::real_impl::*;`) beside a test-only twin gates like a
/// file module's. Controls: the ungated alias and glob keep main's credit.
#[test]
fn a_gated_alias_of_the_owners_inline_module_is_a_gate() -> Result<(), String> {
    check(rows! {
        gap("gated alias of the inline module", inline_module(|path| format!("{}#[cfg(not(test))]\npub use real_impl as imp;\n#[cfg(test)]\npub use fake_impl as imp;\n", fake_impl(path)))),
        row("ungated alias of the inline module", inline_module(|_| "pub use real_impl as imp;\n".to_string()), [W, W]),
        gap("gated glob of the inline module", inline_module(|path| format!("{}pub mod imp {{\n    #[cfg(not(test))]\n    pub use crate::real_impl::*;\n    #[cfg(test)]\n    pub use crate::fake_impl::*;\n}}\n", fake_impl(path)))),
        row("ungated glob of the inline module", inline_module(|_| "pub mod imp {\n    pub use crate::real_impl::*;\n}\n".to_string()), [W, W]),
    })
}

/// The diff of `src/lib.rs` for one hunk header and body.
fn lib_diff(hunk: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 3333333..4444444 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n{hunk}"
    )
}

/// The [`tail_class_patched`] fixture for `src/real.rs` with `lib.rs` built
/// from `items` and `import`, `extra`, and `lib_hunk` (a hunk of `lib.rs`,
/// or none) in the diff.
fn patched(
    items: &'static str,
    import: &'static str,
    extra: Extra,
    lib_hunk: fn(TailPath) -> Option<String>,
) -> Run {
    Box::new(move |path| {
        let patch = lib_hunk(path).map_or_else(String::new, |hunk| lib_diff(&hunk));
        tail_class_patched(
            path,
            "src/real.rs",
            lib_with(items, import, path),
            extra(path),
            &patch,
        )
    })
}

/// #15035 round-7 review (Devin 4658588280): removing `#[cfg(not(test))]`
/// from `mod api` (and its test-only twin defining `ready`) leaves the
/// re-export line unchanged, but its old-side gate let the tests run the
/// twin. Control: removing `#[doc(hidden)]` from the same module keeps the
/// credit an unchanged `lib.rs` has.
#[test]
fn a_removed_enclosing_gate_of_an_unchanged_reexport_refuses() -> Result<(), String> {
    const ITEMS: &str = "mod real;\npub mod api {\n    pub use crate::real::ready;\n}\n";
    const IMPORT: &str = "use super::api::ready;";
    check(rows! {
        gap("removed enclosing cfg of an unchanged re-export", patched(ITEMS, IMPORT, none, |path| Some(format!(
            "@@ -1,13 +1,6 @@\n mod real;\n-#[cfg(not(test))]\n pub mod api {{\n     pub use crate::real::ready;\n }}\n-#[cfg(test)]\n-pub mod api {{\n-    {}\n-        {}\n-    }}\n-}}\n #[cfg(test)]\n mod tests {{\n",
            path.signature(), path.new_tail())))),
        like("removed enclosing cfg of an unchanged re-export control",
            patched(ITEMS, IMPORT, none, |_| Some("@@ -1,7 +1,6 @@\n mod real;\n-#[doc(hidden)]\n pub mod api {\n     pub use crate::real::ready;\n }\n #[cfg(test)]\n mod tests {\n".to_string())),
            patched(ITEMS, IMPORT, none, |_| None)),
    })
}

/// The changed tail of a crate-root `file` holding the owner and its test.
fn package_root(file: &'static str) -> Run {
    Box::new(move |path| {
        let lib = format!("{}{}", path.owner(), tests_module(GLOB, &path.pin()));
        class_at(path, &[(file, lib)], &path.patch(file), file, 2)
    })
}

/// #15035 round-7 review (Devin 4658588495): a library's `mod bin;` file
/// `src/bin.rs` is module `bin`, not the `src/bin` target directory, so its
/// owner reads as the same owner in `src/real.rs` does. A package root
/// below a top-level `src/` (`src/tools/foo/src/lib.rs`) with no `src`
/// module declared stays a crate root (round-8 review).
#[test]
fn a_library_bin_module_is_a_module() -> Result<(), String> {
    check(rows! {
        like("src/bin.rs owner", lib("src/bin.rs", "mod bin;\npub use bin::ready;\n", GLOB, none), real("mod real;\npub use real::ready;\n")),
        like("nested src package root", package_root("src/tools/foo/src/lib.rs"), package_root("crates/foo/src/lib.rs")),
    })
}

/// #15035 round-7 review (Devin 4658588707): a diff adding `extern crate
/// self as std;` refuses an owner only when a gated `std::` import names
/// its chain or names; without one, the owner keeps the credit an unchanged
/// `lib.rs` has.
#[test]
fn an_added_sysroot_rename_refuses_only_matching_gates() -> Result<(), String> {
    const ITEMS: &str =
        "#![no_std]\nextern crate self as std;\nmod real;\nmod fake;\nmod api;\npub use api::*;\n";
    fn added(_: TailPath) -> Option<String> {
        Some("@@ -1,2 +1,3 @@\n #![no_std]\n+extern crate self as std;\n mod real;\n".to_string())
    }
    check(rows! {
        gap("added sysroot rename with a gated std import", patched(ITEMS, GLOB, |_| vec![("src/api.rs", std_api(true))], added)),
        like("added sysroot rename with a gated std import control",
            patched(ITEMS, GLOB, |_| vec![("src/api.rs", std_api(false))], added),
            patched(ITEMS, GLOB, |_| vec![("src/api.rs", std_api(false))], |_| None)),
    })
}

/// The changed tail of `ready` on lines 1-3 of the crate root
/// `src/lib.rs`, after `items` (a macro naming `module`) and a test module
/// pinning it, with `lib_hunk` (a hunk of the same file) after the owner's.
fn root_tail(lib_hunk: &'static str) -> Run {
    Box::new(move |path| {
        let lib = format!(
            "{}{}{}",
            path.owner(),
            declaring_macro("new_extra"),
            tests_module(GLOB, &path.pin())
        );
        let patch = format!("{}{lib_hunk}", path.patch("src/lib.rs"));
        class_at(path, &[("src/lib.rs", lib)], &patch, "src/lib.rs", 2)
    })
}

/// A module-emitting macro naming `module`.
fn declaring_macro(module: &str) -> String {
    format!("macro_rules! declare {{\n    ($m:ident) => {{\n        mod {module};\n    }};\n}}\n")
}

/// #15035 round-7 review (Devin 4658588614): editing an unused
/// module-emitting macro does not refuse a crate-root owner, which no
/// macro-emitted declaration can reach. The same edit still refuses an
/// owner in a module file.
#[test]
fn a_changed_macro_leaves_crate_root_owners_credited() -> Result<(), String> {
    // Lines 4-8 of the crate-root `lib.rs` hold the macro, line 6 its `mod`;
    // with the owner in `src/real.rs`, the macro's `mod` is on line 5.
    let items = format!(
        "mod real;\npub use real::ready;\n{}",
        declaring_macro("new_extra")
    );
    let module_file: Run = Box::new(move |path| {
        let hunk = "@@ -5,1 +5,1 @@\n-        mod extra;\n+        mod new_extra;\n";
        tail_class_patched(
            path,
            "src/real.rs",
            lib_with(&items, GLOB, path),
            Vec::new(),
            &lib_diff(hunk),
        )
    });
    check(rows! {
        like("crate root", root_tail("@@ -6,1 +6,1 @@\n-        mod extra;\n+        mod new_extra;\n"), root_tail("")),
        gap("module file", module_file),
    })
}

/// `src/imp.rs` including `src/unused.rs` through `concat` (spelled
/// `concat`, after `prefix`), with that file.
fn concat_include(prefix: &str, concat: &str) -> Vec<(&'static str, String)> {
    imp_and_unused(format!(
        "{prefix}#[cfg(not(test))]\ninclude!({concat}!(\"unused.rs\"));\n"
    ))
}

/// #15035 round-7 review: a bare `concat!` in an `include!` is the built-in
/// only when no workspace macro or import named `concat` shadows it (a
/// `macro_rules! concat`, `use crate::src_path as concat;`), and a
/// `core::concat!` only when no crate or import is renamed to `core`. Each gap's
/// `include!` may expand to the owner file, so it gates every owner.
/// Controls: the same `include!` without the shadowing macro, import or
/// rename keeps main's credit.
#[test]
fn a_shadowed_concat_include_path_is_opaque() -> Result<(), String> {
    const SHADOWING_CONCAT: &str = "macro_rules! concat {\n    ($n:literal) => {\n        ::core::concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\")\n    };\n}\nmod real;\npub use real::ready;\npub mod imp;\n";
    const SRC_PATH_MACRO: &str = "mod real;\n#[macro_export]\nmacro_rules! src_path {\n    ($name:literal) => {\n        ::core::concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\")\n    };\n}\npub use real::ready;\npub mod imp;\n";
    const RENAMED_CORE: &str = "extern crate self as core;\n#[macro_export]\nmacro_rules! concat {\n    ($($t:tt)*) => {\n        \"real.rs\"\n    };\n}\nmod real;\npub use real::ready;\npub mod imp;\n";
    const PATHS: &str = "#[macro_export]\nmacro_rules! real_path {\n    ($($t:tt)*) => {\n        \"real.rs\"\n    };\n}\npub mod paths {\n    pub use crate::real_path as concat;\n}\nmod real;\npub use real::ready;\npub mod imp;\n";
    let bare: Extra = |_| concat_include("", "concat");
    let core: Extra = |_| concat_include("", "core::concat");
    check(rows! {
        pair("bare concat shadowed by a user macro",
            lib("src/real.rs", SHADOWING_CONCAT, GLOB, bare), lib("src/real.rs", OWNER_AND_IMP, GLOB, bare), [W, PU]),
        pair("bare concat shadowed by an import renamed to concat",
            lib("src/real.rs", SRC_PATH_MACRO, GLOB, |_| concat_include("use crate::src_path as concat;\n", "concat")),
            lib("src/real.rs", SRC_PATH_MACRO, GLOB, bare), [W, PU]),
        pair("core concat under a crate renamed to core",
            lib("src/real.rs", RENAMED_CORE, GLOB, core), lib("src/real.rs", OWNER_AND_IMP, GLOB, core), [W, PU]),
        pair("core concat under an import renamed to core",
            lib("src/real.rs", PATHS, GLOB, |_| concat_include("use crate::paths as core;\n", "core::concat")),
            lib("src/real.rs", OWNER_AND_IMP, GLOB, core), [W, PU]),
        gap("root concat shadow over an include under a nested src module", lib("src/real.rs", "macro_rules! concat {\n    ($($t:tt)*) => {\n        \"../../real.rs\"\n    };\n}\nmod real;\npub use real::ready;\npub mod a;\n", GLOB, |_| vec![
            ("src/a.rs", "pub mod src;\n".into()),
            ("src/a/src.rs", "pub mod imp;\n".into()),
            ("src/a/src/imp.rs", "#[cfg(not(test))]\ninclude!(concat!(\"unused.rs\"));\n".into()),
            ("src/a/src/unused.rs", "pub fn unused() {}\n".into()),
        ])),
        row("cast to a type named core elsewhere", lib("src/real.rs", format!("{OWNER_AND_IMP}mod other;\n"), GLOB, |_| {
            let mut files = concat_include("", "core::concat");
            files.push(("src/other.rs", "type core = u8;\npub fn cast(x: u16) -> u8 {\n    x as core\n}\n".into()));
            files
        }), [W, PU]),
    })
}
