//! `docs/TROUBLESHOOTING.md` quotes only output the built binary prints.
//!
//! The guide tells a reader which line on their screen explains an empty
//! result, a stop or an unexpected finding. A quoted line that the product no
//! longer prints would send that reader looking for text that is not there,
//! so the guide is checked against the binary in both directions:
//!
//! - every line inside a `text` fence in the guide must appear in the output
//!   of at least one reproduced situation;
//! - every reproduced situation must have one of its lines quoted in the
//!   guide, and must end with the exit code the guide states.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"tg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const BASE_SRC: &str = "pub fn price(qty: u32, unit: u32) -> u32 {\n    if qty > 10 {\n        qty * unit - 5\n    } else {\n        qty * unit\n    }\n}\n";
const CHANGED_SRC: &str = "pub fn price(qty: u32, unit: u32) -> u32 {\n    if qty >= 10 {\n        qty * unit - 5\n    } else {\n        qty * unit\n    }\n}\n";
const BASE_TEST: &str =
    "use tg::price;\n#[test]\nfn small() {\n    assert_eq!(price(2, 3), 6);\n}\n";
const BOUNDARY_TEST: &str = "use tg::price;\n#[test]\nfn small() {\n    assert_eq!(price(2, 3), 6);\n}\n#[test]\nfn boundary() {\n    assert_eq!(price(9, 3), 27);\n    assert_eq!(price(10, 3), 25);\n    assert_eq!(price(11, 3), 28);\n}\n";
const LONE_FN: &str = "pub fn lonely(x: u32) -> u32 {\n    if x > 3 { x + 1 } else { x }\n}\n";
/// The production change as a patch whose paths carry a prefix the tree
/// does not have.
const PREFIXED_PATCH: &str = "diff --git a/sub/src/lib.rs b/sub/src/lib.rs\n--- a/sub/src/lib.rs\n+++ b/sub/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn price(qty: u32, unit: u32) -> u32 {\n-    if qty > 10 {\n+    if qty >= 10 {\n         qty * unit - 5\n     } else {\n         qty * unit\n";

/// A scratch directory removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        Self::new_under(&std::env::temp_dir())
    }

    fn new_under(parent: &Path) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = parent.join(format!(
            "ripr-troubleshooting-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    /// One crate with one test, committed on a branch named `main`.
    fn repo() -> Result<Self, String> {
        let scratch = Self::new()?;
        scratch.write_crate()?;
        scratch.git(&["-c", "init.templateDir=", "init", "-q"])?;
        scratch.git(&["symbolic-ref", "HEAD", "refs/heads/main"])?;
        scratch.git(&["config", "user.email", "t@example.com"])?;
        scratch.git(&["config", "user.name", "t"])?;
        scratch.git(&["config", "commit.gpgSign", "false"])?;
        scratch.commit("base")?;
        Ok(scratch)
    }

    fn write_crate(&self) -> Result<(), String> {
        std::fs::create_dir_all(self.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(self.0.join("tests")).map_err(|e| e.to_string())?;
        self.write(".gitignore", "/target/\n")?;
        self.write("Cargo.toml", CARGO_TOML)?;
        self.write("src/lib.rs", BASE_SRC)?;
        self.write("tests/t.rs", BASE_TEST)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        git_in(&self.0, args)
    }

    fn commit(&self, message: &str) -> Result<(), String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--no-gpg-sign", "-m", message])
    }

    fn ripr(&self, args: &[&str]) -> Result<Run, String> {
        ripr_in(&self.0, args)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

fn git_in(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Exit code and combined stdout and stderr of one `ripr` run.
struct Run {
    code: Option<i32>,
    text: String,
}

fn ripr_in(dir: &Path, args: &[&str]) -> Result<Run, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(Run {
        code: output.status.code(),
        text: format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    })
}

/// One situation the guide describes: its section heading, the exit code the
/// guide states, and how to reproduce it. `Ok(None)` means this host cannot
/// stage the situation; its section is then left unchecked.
struct Situation {
    section: &'static str,
    exit: i32,
    reproduce: fn() -> Result<Option<Run>, String>,
}

/// A crate directory outside every Git work tree. The workspace's cargo
/// config points the temp directory inside the checkout and ripr resolves a
/// root by walking up, so the directory is staged under `/tmp`; a host with
/// no such directory, or one that keeps it inside a work tree, cannot stage
/// it.
fn outside_any_repository() -> Result<Option<Scratch>, String> {
    let parent = Path::new("/tmp");
    if !cfg!(unix) || !parent.is_dir() {
        return Ok(None);
    }
    let plain = Scratch::new_under(parent)?;
    let enclosing = Command::new("git")
        .current_dir(&plain.0)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| e.to_string())?;
    if enclosing.status.success() {
        return Ok(None);
    }
    plain.write_crate()?;
    Ok(Some(plain))
}

const SITUATIONS: &[Situation] = &[
    Situation {
        section: "The diff is empty",
        exit: 0,
        reproduce: || Scratch::repo()?.ripr(&["check"]).map(Some),
    },
    Situation {
        section: "A new file is not picked up",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/new.rs", LONE_FN)?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "Only comments or tests changed",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", &format!("{BASE_SRC}// a note\n"))?;
            repo.write("tests/t.rs", BOUNDARY_TEST)?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "Only documentation or configuration changed",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("README.md", "notes\n")?;
            repo.git(&["add", "README.md"])?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "The changed language is in preview and not enabled",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("over.py", "def over(qty):\n    return qty > 10\n")?;
            repo.git(&["add", "over.py"])?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "Rust is switched off in `ripr.toml`",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("ripr.toml", "[languages]\nenabled = [\"python\"]\n")?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "The changed language is not supported",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("main.go", "package main\n\nfunc main() {}\n")?;
            repo.git(&["add", "main.go"])?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "Findings exist but are withheld",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", &format!("#[mutants::skip]\n{CHANGED_SRC}"))?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "Every finding reads `exposed`",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.write("tests/t.rs", BOUNDARY_TEST)?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "The base does not resolve",
        exit: 2,
        reproduce: || {
            Scratch::repo()?
                .ripr(&["check", "--base", "no-such-ref"])
                .map(Some)
        },
    },
    Situation {
        section: "The base does not resolve",
        exit: 2,
        reproduce: || {
            // A depth-one clone of the `work` branch has no `origin/main`.
            let origin = Scratch::repo()?;
            origin.git(&["checkout", "-q", "-b", "work"])?;
            origin.write("src/lib.rs", CHANGED_SRC)?;
            origin.commit("change")?;
            let holder = Scratch::new()?;
            let source = origin.0.display().to_string();
            git_in(
                &holder.0,
                &[
                    "clone",
                    "-q",
                    "--no-local",
                    "--depth",
                    "1",
                    "--branch",
                    "work",
                    &source,
                    "clone",
                ],
            )?;
            ripr_in(&holder.0.join("clone"), &["check", "--base", "origin/main"]).map(Some)
        },
    },
    Situation {
        section: "The directory is not a Git repository",
        exit: 2,
        reproduce: || match outside_any_repository()? {
            Some(plain) => plain.ripr(&["check"]).map(Some),
            None => Ok(None),
        },
    },
    Situation {
        section: "The root does not exist",
        exit: 2,
        reproduce: || {
            Scratch::repo()?
                .ripr(&["check", "--root", "no-such-dir"])
                .map(Some)
        },
    },
    Situation {
        section: "The root does not exist",
        exit: 3,
        reproduce: || {
            Scratch::repo()?
                .ripr(&["doctor", "--root", "no-such-dir"])
                .map(Some)
        },
    },
    Situation {
        section: "The root is a subdirectory of the crate",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.ripr(&["check", "--root", "src"]).map(Some)
        },
    },
    Situation {
        section: "`ripr.toml` is rejected",
        exit: 2,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("ripr.toml", "[analysis]\nexclude = [\"src/**\"]\n")?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "A supplied diff names files that are not in the tree",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("prefixed.patch", PREFIXED_PATCH)?;
            repo.ripr(&["check", "--diff", "prefixed.patch"]).map(Some)
        },
    },
    Situation {
        section: "\"no path\" although a test covers the code",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", &format!("{BASE_SRC}{LONE_FN}"))?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "\"weak\" although the test has assertions",
        exit: 0,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.ripr(&["check"]).map(Some)
        },
    },
    Situation {
        section: "`explain` says no finding matched",
        exit: 2,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.ripr(&["explain", "src/lib.rs:3"]).map(Some)
        },
    },
    Situation {
        section: "`explain` says no finding matched",
        exit: 2,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.ripr(&["explain", "src/nope.rs:3"]).map(Some)
        },
    },
    Situation {
        section: "`explain` says no finding matched",
        exit: 2,
        reproduce: || {
            let repo = Scratch::repo()?;
            repo.write("src/lib.rs", CHANGED_SRC)?;
            repo.ripr(&["explain", "tests/t.rs:4"]).map(Some)
        },
    },
];

fn guide() -> Result<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/TROUBLESHOOTING.md");
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Each `text` fence in the guide as (section heading, quoted lines).
fn quoted_blocks(guide: &str) -> Vec<(String, Vec<String>)> {
    let mut blocks = Vec::new();
    let mut section = String::new();
    let mut open: Option<Vec<String>> = None;
    for line in guide.lines() {
        match &mut open {
            Some(lines) if line.trim() == "```" => {
                blocks.push((section.clone(), std::mem::take(lines)));
                open = None;
            }
            Some(lines) => {
                if !line.trim().is_empty() {
                    lines.push(line.trim().to_string());
                }
            }
            None if line.trim() == "```text" => open = Some(Vec::new()),
            None => {
                if let Some(heading) = line.strip_prefix("### ") {
                    section = heading.trim().to_string();
                }
            }
        }
    }
    blocks
}

#[test]
fn every_quoted_line_is_printed_and_every_situation_is_quoted() -> Result<(), String> {
    let guide = guide()?;
    let blocks = quoted_blocks(&guide);
    if blocks.len() < 15 {
        return Err(format!(
            "the guide's quoted blocks were not read: found {}",
            blocks.len()
        ));
    }

    // Reproduce every situation once, checking the exit code the guide gives.
    let mut runs: Vec<(&'static str, String)> = Vec::new();
    let mut unstaged: Vec<&'static str> = Vec::new();
    for situation in SITUATIONS {
        let Some(run) = (situation.reproduce)()? else {
            unstaged.push(situation.section);
            continue;
        };
        if run.code != Some(situation.exit) {
            return Err(format!(
                "{}: expected exit {}, got {:?}:\n{}",
                situation.section, situation.exit, run.code, run.text
            ));
        }
        runs.push((situation.section, run.text));
    }

    let mut problems = Vec::new();
    // Guide to binary: a quoted line must be printed in its own section's
    // reproduced situation.
    for (section, lines) in &blocks {
        if unstaged.contains(&section.as_str()) {
            continue;
        }
        let outputs: Vec<&String> = runs
            .iter()
            .filter(|(name, _)| name == section)
            .map(|(_, text)| text)
            .collect();
        if outputs.is_empty() {
            problems.push(format!(
                "section {section:?} quotes output but is not reproduced"
            ));
            continue;
        }
        for line in lines {
            if !outputs.iter().any(|text| text.contains(line.as_str())) {
                problems.push(format!(
                    "section {section:?} quotes a line ripr did not print: {line:?}"
                ));
            }
        }
    }
    // Binary to guide: each reproduced run must be the source of a quote.
    for (section, text) in &runs {
        let quoted = blocks
            .iter()
            .filter(|(name, _)| name == section)
            .flat_map(|(_, lines)| lines)
            .any(|line| text.contains(line.as_str()));
        if !quoted {
            problems.push(format!(
                "a reproduced situation in {section:?} has no line quoted in the guide:\n{text}"
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The exit codes the guide states in prose match the situations above, so
/// the table here cannot pass while the page says something else.
#[test]
fn stated_exit_codes_match_the_reproduced_situations() -> Result<(), String> {
    let guide = guide()?;
    for situation in SITUATIONS {
        let Some(start) = guide.find(&format!("### {}\n", situation.section)) else {
            return Err(format!("the guide has no section {:?}", situation.section));
        };
        let body = &guide[start..];
        let end = body[4..].find("\n## ").map_or(body.len(), |at| at + 4);
        let end = body[4..].find("\n### ").map_or(end, |at| end.min(at + 4));
        let stated = body[..end].contains(&format!("xit code `{}`", situation.exit));
        if situation.exit != 0 && !stated {
            return Err(format!(
                "section {:?} must state exit code {}",
                situation.section, situation.exit
            ));
        }
    }
    Ok(())
}
