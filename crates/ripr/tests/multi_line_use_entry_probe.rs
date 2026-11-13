//! #8389: an entry added to or removed from a wrapped `use a::{ .. };` is
//! import text, like the single-line `use a::B;` that already seeds no probe.
//! Dogfood (ripgrep `68b3c9e`) read each such line as its own
//! `static_unknown` finding. The same text as a call argument, and the same
//! entry in a `pub(crate) use` re-export, stay probed.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-multi-line-use-entry-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"multi-line-use-entry\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
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

/// The changed text. It is a valid `use` tree entry and a valid call argument.
const ENTRY: &str = "    limits::CEILING,";

/// A library whose lines 1-4 are `opener`, the entry (only when `with_entry`),
/// a second line and `closer`, followed by an unchanged tested function.
fn lib(opener: &str, second: &str, closer: &str, with_entry: bool) -> String {
    let entry = if with_entry {
        format!("{ENTRY}\n")
    } else {
        String::new()
    };
    format!(
        "{opener}
{entry}{second}
{closer}

pub mod limits {{
    pub const CEILING: u64 = 90;
    pub const FLOOR: u64 = 10;
}}

pub fn pick(a: u64, b: u64) -> u64 {{
    a.max(b)
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn picks_the_larger() {{
        assert_eq!(pick(limits::FLOOR, 33), 33);
    }}
}}
"
    )
}

/// A diff that adds (`+`) or removes (`-`) the entry at line 2.
fn diff(opener: &str, second: &str, closer: &str, sign: char) -> String {
    let (old, new) = if sign == '+' { (3, 4) } else { (4, 3) };
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,{old} +1,{new} @@
 {opener}
{sign}{ENTRY}
 {second}
 {closer}
"
    )
}

/// The findings anchored at the changed entry (line 2) for one shape.
fn entry_findings(
    opener: &str,
    second: &str,
    closer: &str,
    sign: char,
) -> Result<Vec<Finding>, String> {
    let source = lib(opener, second, closer, sign == '+');
    if sign == '+' {
        let line = source.lines().nth(1).ok_or("fixture lost line 2")?;
        if line != ENTRY {
            return Err(format!("premise: line 2 is the entry, got {line:?}"));
        }
    }
    let findings = TempRepo::create(&source, &diff(opener, second, closer, sign))?.findings()?;
    Ok(findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect())
}

const CALL: (&str, &str, &str) = (
    "pub fn picked() -> u64 { pick(",
    "    limits::FLOOR,",
    ") }",
);

#[test]
fn an_entry_of_a_wrapped_use_declaration_seeds_no_finding() -> Result<(), String> {
    for (label, opener) in [("use", "use crate::{"), ("pub use", "pub use crate::{")] {
        for sign in ['+', '-'] {
            let found = entry_findings(opener, "    limits::FLOOR,", "};", sign)?;
            if !found.is_empty() {
                return Err(format!(
                    "{label} {sign}: a wrapped import entry is not a changed behavior: {found:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn the_same_text_outside_a_plain_use_declaration_stays_probed() -> Result<(), String> {
    // Control: without it, an empty result above could mean the fixture never
    // seeds a probe at line 2 at all.
    let (opener, second, closer) = CALL;
    for sign in ['+', '-'] {
        if entry_findings(opener, second, closer, sign)?.is_empty() {
            return Err(format!(
                "control {sign}: the entry as a call argument must still be a finding"
            ));
        }
    }
    // A visibility-qualified re-export can retarget existing call sites; its
    // single-line form is probed, so its wrapped entries stay probed too.
    if entry_findings("pub(crate) use crate::{", "    limits::FLOOR,", "};", '+')?.is_empty() {
        return Err("a `pub(crate) use` entry must keep its finding".to_string());
    }
    Ok(())
}
