//! RIPR-SPEC-0348 Rule 5 (#15931): a line that only writes `Self` for the
//! implementing type's own name (or the reverse) seeds no probe; every
//! spelling the two files do not prove equivalent stays probed. Each case
//! is a real Git commit pair read through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .map_err(|error| format!("git {args:?} did not start: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// The (head-side line, expression) of every finding for the commit that
/// turns `before` into `after`.
fn git_probes(before: &str, after: &str) -> Result<Vec<(usize, String)>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-self-type-respelling-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let root = &repo.root;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    write(
        "Cargo.toml",
        "[package]\nname = \"self-type-respelling\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", before)?;
    git(root, &["init", "-q"])?;
    git(root, &["config", "core.autocrlf", "false"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "-q", "-m", "before"])?;
    write("src/lib.rs", after)?;
    git(root, &["commit", "-q", "-a", "-m", "after"])?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: Some("HEAD~1".to_string()),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let mut lines = output
        .findings
        .iter()
        .map(|finding| {
            (
                finding.probe.location.line,
                finding.probe.expression.clone(),
            )
        })
        .collect::<Vec<_>>();
    lines.sort();
    Ok(lines)
}

fn probed(probes: &[(usize, String)], line: usize) -> bool {
    probes.iter().any(|(probed, _)| *probed == line)
}

/// The clap 710884f shape (`style: Make clippy happy`): match arms, a
/// return type, a call and a constructor respelled inside the type's own
/// inherent and trait impls.
const BEFORE: &str = "\
pub enum Shell {
    Bash,
    Zsh,
}

pub trait Named {
    fn file_name(&self, name: &str) -> String;
}

impl Named for Shell {
    fn file_name(&self, name: &str) -> String {
        match self {
            Shell::Bash => format!(\"{name}.bash\"),
            Shell::Zsh => format!(\"_{name}\"),
        }
    }
}

impl Shell {
    pub fn from_name(name: &str) -> Option<Shell> {
        if name == \"bash\" {
            Some(Shell::Bash)
        } else {
            Shell::fallback(name)
        }
    }

    fn fallback(name: &str) -> Option<Shell> {
        if name.len() > 3 { Some(Shell::Zsh) } else { None }
    }
}
";

fn respelled(source: &str) -> String {
    source
        .replace("Shell::", "Self::")
        .replace("Option<Shell>", "Option<Self>")
}

fn line_of(source: &str, needle: &str) -> Result<usize, String> {
    source
        .lines()
        .position(|line| line.contains(needle))
        .map(|index| index + 1)
        .ok_or_else(|| format!("fixture lost `{needle}`"))
}

#[test]
fn self_for_the_implementing_type_seeds_no_probe_in_either_direction() -> Result<(), String> {
    let after = respelled(BEFORE);
    assert_ne!(after, BEFORE, "the fixture must respell something");
    assert_eq!(git_probes(BEFORE, &after)?, Vec::new());
    assert_eq!(git_probes(&after, BEFORE)?, Vec::new());
    Ok(())
}

/// The respelling beside a real edit: only the real edit keeps its probe.
#[test]
fn a_real_change_beside_the_respelling_keeps_its_probe() -> Result<(), String> {
    let after = respelled(BEFORE).replace("name.len() > 3", "name.len() >= 3");
    let probes = git_probes(BEFORE, &after)?;
    let lines = probes.clone();
    let real = line_of(&after, "name.len() >= 3")?;
    assert!(probed(&probes, real), "{lines:?}");
    assert_eq!(
        lines
            .iter()
            .filter(|(line, _)| *line != real)
            .collect::<Vec<_>>(),
        Vec::<&(usize, String)>::new()
    );
    Ok(())
}

/// Controls: each case is the same one-line edit of a call, in a shape
/// where `Self` and the spelled name are not established as the same item.
/// Every one must keep the probe the established shape drops.
#[test]
fn a_spelling_the_files_do_not_prove_equivalent_stays_probed() -> Result<(), String> {
    let wrap = |header: &str, body: &str, tail: &str| {
        format!(
            "pub struct Shell(pub u8);\npub struct Other(pub u8);\n{header}\n{body}\n{tail}\n\
             impl Other {{\n    pub fn make(n: u8) -> Other {{\n        Other(n)\n    }}\n}}\n"
        )
    };
    let inherent = |before_line: &str, after_line: &str| {
        let file = |line: &str| {
            wrap(
                "impl Shell {\n    pub fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n\
                 \n    pub fn twice(n: u8) -> Shell {",
                line,
                "    }\n}",
            )
        };
        (file(before_line), file(after_line))
    };
    let shaped = |header: &str, tail: &str| {
        let file = |line: &str| wrap(header, line, tail);
        (
            file("        Shell::make(n + n)"),
            file("        Self::make(n + n)"),
        )
    };
    let neutral = inherent("        Shell::make(n + n)", "        Self::make(n + n)");
    let cases = [
        (
            "another type's name became Self",
            inherent(
                "        Other::make(n + n).into()",
                "        Self::make(n + n).into()",
            ),
        ),
        (
            "the line also changes another token",
            inherent("        Shell::make(n + n)", "        Self::make(n * n)"),
        ),
        (
            "the path is qualified",
            inherent(
                "        crate::Shell::make(n + n)",
                "        crate::Self::make(n + n)",
            ),
        ),
        (
            "the token is macro input",
            inherent(
                "        dbg!(Shell::make(n + n))",
                "        dbg!(Self::make(n + n))",
            ),
        ),
        (
            "the impl is for a reference to the type",
            shaped(
                "pub trait Twice {\n    fn twice(n: u8) -> Shell;\n}\n\
                 impl Twice for &Shell {\n    fn twice(n: u8) -> Shell {",
                "    }\n}\nimpl Shell {\n    pub fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n}",
            ),
        ),
        (
            "a nested impl of another type holds the line",
            shaped(
                "impl Shell {\n    pub fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n\
                 \n    pub fn outer() {\n        struct Inner;\n        impl Inner {\n\
                 \x20           fn make(n: u8) -> Shell {\n                Shell(n)\n            }\n\
                 \x20           fn twice(n: u8) -> Shell {",
                "            }\n        }\n        let _ = Inner::twice(1);\n    }\n}",
            ),
        ),
        (
            "a generic parameter shadows the type name",
            shaped(
                "pub trait Make {\n    fn make(n: u8) -> Self;\n}\n\
                 impl Make for Shell {\n    fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n}\n\
                 impl Other {\n    pub fn twice<Shell: Make>(n: u8) -> Shell {",
                "    }\n}",
            ),
        ),
        (
            "the method body imports a name",
            shaped(
                "impl Shell {\n    pub fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n\
                 \n    pub fn twice(n: u8) -> Shell {\n        use self::Other as Renamed;\n\
                 \x20       let _ = Renamed(n);",
                "    }\n}",
            ),
        ),
        (
            "an attribute macro receives the method",
            shaped(
                "impl Shell {\n    pub fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n\
                 \n    #[tracing::instrument]\n    pub fn twice(n: u8) -> Shell {",
                "    }\n}",
            ),
        ),
        (
            "an attribute macro receives the enclosing module",
            shaped(
                "#[bridge::module]\nmod inner {\n    pub struct Shell(pub u8);\n    impl Shell {\n\
                 \x20       pub fn make(n: u8) -> Shell {\n            Shell(n)\n        }\n\
                 \x20       pub fn twice(n: u8) -> Shell {",
                "        }\n    }\n}",
            ),
        ),
        (
            "the line is outside every impl",
            shaped(
                "impl Shell {\n    pub fn make(n: u8) -> Shell {\n        Shell(n)\n    }\n}\n\
                 pub fn twice(n: u8) -> Shell {",
                "}",
            ),
        ),
    ];
    let twice =
        |source: &str| line_of(source, "make(n + n)").or_else(|_| line_of(source, "make(n * n)"));
    let neutral_probes = git_probes(&neutral.0, &neutral.1)?;
    assert!(
        !probed(&neutral_probes, twice(&neutral.1)?),
        "the established shape must drop its probe: {:?}",
        neutral_probes
    );
    let mut unprobed = Vec::new();
    for (case, (before, after)) in cases {
        assert_ne!(before, after, "{case}: the fixture must edit a line");
        let probes = git_probes(&before, &after)?;
        if !probed(&probes, twice(&after)?) {
            unprobed.push((case, probes));
        }
    }
    assert_eq!(unprobed, Vec::new());
    Ok(())
}

/// A type declared with generic parameters: `Wrap::new()` infers what
/// `Self::new()` fixes, so the pair is not established.
#[test]
fn a_generic_type_keeps_its_respelling_probed() -> Result<(), String> {
    let file = |line: &str| {
        format!(
            "pub struct Wrap<T = u8>(pub T);\n\nimpl Wrap {{\n    pub fn make(n: u8) -> Wrap {{\n\
             \x20       Wrap(n)\n    }}\n\n    pub fn twice(n: u8) -> Wrap {{\n{line}\n    }}\n}}\n"
        )
    };
    let before = file("        Wrap::make(n + n)");
    let after = file("        Self::make(n + n)");
    let probes = git_probes(&before, &after)?;
    assert!(
        probed(&probes, line_of(&after, "Self::make(n + n)")?),
        "{:?}",
        probes
    );
    Ok(())
}
