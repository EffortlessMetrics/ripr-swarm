//! Public-API controls for formatting dispatch reach and code a test cannot run
//! (#8999, RIPR-SPEC-0297).
//!
//! A `to_string()` or `format!` that runs reaches a changed `Display::fmt`
//! even when its result is dropped. One that sits in a literal `if false`
//! block, behind `#[cfg(any())]`, in a closure or nested fn nothing calls, or
//! after a `return` does not run, so it is not reach. An unknown condition may
//! run and stays reached. Reach never raises a class.

use ripr::domain::RelationReason;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const OLD_LINE: &str = "        write!(f, \"merge-{}\", self.n + 1)";
const NEW_LINE: &str = "        write!(f, \"merge-{}\", self.n + 2)";

fn source(tests: &str) -> String {
    format!(
        "use std::fmt;

pub struct Merge {{
    n: u32,
}}

impl fmt::Display for Merge {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
{NEW_LINE}
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

{tests}}}
"
    )
}

const HELPER_OLD: &str = "    n + 1";
const HELPER_NEW: &str = "    n + 2";

/// The changed line of the helper-entry fixture: `fmt` only calls `bump`.
fn helper_source(tests: &str) -> String {
    format!(
        "use std::fmt;

pub struct Merge {{
    n: u32,
}}

fn bump(n: u32) -> u32 {{
{HELPER_NEW}
}}

impl fmt::Display for Merge {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
        write!(f, \"merge-{{}}\", bump(self.n))
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

{tests}}}
"
    )
}

#[derive(Clone, Copy)]
enum Route {
    /// The changed line is in `fmt`; a test relates as `direct_owner_call`.
    Direct,
    /// The changed line is in `bump`, called only by `fmt`; a test relates as
    /// `helper_owner_call` through the helper-entry route.
    Helper,
}

/// The changed line's finding, with the names of the tests it relates to by
/// the route's relation reason.
fn related(tests: &str) -> Result<Vec<String>, String> {
    related_via(Route::Direct, tests)
}

fn helper_related(tests: &str) -> Result<Vec<String>, String> {
    related_via(Route::Helper, tests)
}

fn related_via(route: Route, tests: &str) -> Result<Vec<String>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-fmt-live-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = related_in(&root, route, tests);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn related_in(root: &Path, route: Route, tests: &str) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("mkdir failed: {error}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"merged\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
    let (lib, old_line, new_line, reason, owner) = match route {
        Route::Direct => (
            source(tests),
            OLD_LINE,
            NEW_LINE,
            RelationReason::DirectOwnerCall,
            "::fmt",
        ),
        Route::Helper => (
            helper_source(tests),
            HELPER_OLD,
            HELPER_NEW,
            RelationReason::HelperOwnerCall,
            "::bump",
        ),
    };
    let line = lib
        .lines()
        .position(|line| line == new_line)
        .map(|index| index + 1)
        .ok_or("premise: the changed fmt line is in the source")?;
    std::fs::write(root.join("src/lib.rs"), &lib)
        .map_err(|error| format!("write source failed: {error}"))?;
    std::fs::write(
        root.join("diff.patch"),
        format!(
            "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{old_line}
+{new_line}
"
        ),
    )
    .map_err(|error| format!("write diff failed: {error}"))?;
    let output = check_workspace(CheckInput {
        root: root.to_path_buf(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == line
                && finding
                    .probe
                    .owner
                    .as_ref()
                    .is_some_and(|found| found.0.ends_with(owner))
        })
        .collect();
    if findings.is_empty() {
        return Err("premise: the changed fmt line yields a finding".to_string());
    }
    let mut names: Vec<String> = Vec::new();
    for finding in &findings {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "reach never raises a class: {finding:?}"
        );
        names.extend(
            finding
                .related_tests
                .iter()
                .filter(|test| test.relation_reason == Some(reason))
                .map(|test| test.name.clone()),
        );
    }
    names.sort();
    names.dedup();
    Ok(names)
}

const STRAIGHT: &str = "    #[test]
    fn straight() {
        let m = Merge { n: 1 };
        assert_eq!(m.to_string(), \"merge-2\");
    }
";

#[test]
fn clearly_run_formatting_shapes_stay_reached() -> Result<(), String> {
    let tests = format!(
        "{STRAIGHT}
    #[test]
    fn dropped_let() {{
        let m = Merge {{ n: 1 }};
        let _ = format!(\"{{m}}\");
    }}

    #[test]
    fn dropped_statement() {{
        let m = Merge {{ n: 1 }};
        m.to_string();
    }}

    #[test]
    fn unknown_condition() {{
        let m = Merge {{ n: 1 }};
        let cond = std::env::var(\"X\").is_ok();
        if cond {{
            m.to_string();
        }}
    }}

    #[test]
    fn else_of_a_dead_if() {{
        let m = Merge {{ n: 1 }};
        if false {{
            let _ = 1;
        }} else {{
            m.to_string();
        }}
    }}

    #[test]
    fn called_closure() {{
        let m = Merge {{ n: 1 }};
        let f = || m.to_string();
        let _ = f();
    }}

    #[test]
    fn local_drop_impl() {{
        struct Guard;
        impl Drop for Guard {{
            fn drop(&mut self) {{
                let _ = Merge {{ n: 1 }}.to_string();
            }}
        }}
        let _guard = Guard;
    }}

    #[test]
    fn called_nested_fn() {{
        fn render() -> String {{
            Merge {{ n: 1 }}.to_string()
        }}
        let _ = render();
    }}
"
    );
    assert_eq!(
        related(&tests)?,
        [
            "called_closure",
            "called_nested_fn",
            "dropped_let",
            "dropped_statement",
            "else_of_a_dead_if",
            "local_drop_impl",
            "straight",
            "unknown_condition"
        ]
    );
    Ok(())
}

#[test]
fn formatting_that_cannot_run_is_not_reach() -> Result<(), String> {
    let tests = format!(
        "{STRAIGHT}
    #[test]
    fn if_false() {{
        let m = Merge {{ n: 1 }};
        if false {{
            assert_eq!(m.to_string(), \"merge-2\");
        }}
    }}

    #[test]
    fn while_false() {{
        let m = Merge {{ n: 1 }};
        while false {{
            let _ = format!(\"{{m}}\");
        }}
    }}

    #[test]
    fn after_return() {{
        let m = Merge {{ n: 1 }};
        return;
        #[allow(unreachable_code)]
        {{
            m.to_string();
        }}
    }}

    #[test]
    fn cfg_disabled() {{
        let m = Merge {{ n: 1 }};
        #[cfg(any())]
        {{
            m.to_string();
        }}
    }}

    #[test]
    fn never_called_closure() {{
        let m = Merge {{ n: 1 }};
        let _f = || m.to_string();
    }}

    #[test]
    fn never_called_nested_fn() {{
        fn unused() -> String {{
            Merge {{ n: 1 }}.to_string()
        }}
    }}
"
    );
    // The straight-line control proves the fixture reaches fmt at all.
    assert_eq!(related(&tests)?, ["straight"]);
    Ok(())
}

#[test]
fn the_helper_entry_route_reads_only_live_code_too() -> Result<(), String> {
    // `bump` is called only by `Display::fmt`. A test reaches it by formatting
    // a `Merge` it can run; the same formatting in a dead region does not.
    let tests = format!(
        "{STRAIGHT}
    #[test]
    fn dropped_statement() {{
        let m = Merge {{ n: 1 }};
        m.to_string();
    }}

    #[test]
    fn if_false() {{
        let m = Merge {{ n: 1 }};
        if false {{
            assert_eq!(m.to_string(), \"merge-3\");
        }}
    }}

    #[test]
    fn never_called_closure() {{
        let m = Merge {{ n: 1 }};
        let _f = || m.to_string();
    }}

    #[test]
    fn after_return() {{
        let m = Merge {{ n: 1 }};
        return;
        #[allow(unreachable_code)]
        {{
            m.to_string();
        }}
    }}
"
    );
    assert_eq!(
        helper_related(&tests)?,
        ["dropped_statement", "straight"],
        "live formatting reaches the helper; dead formatting does not"
    );
    Ok(())
}
