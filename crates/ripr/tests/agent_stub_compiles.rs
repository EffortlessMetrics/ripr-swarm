//! #5355 / #5453: `ripr agent stub --write` must leave the user's crate compiling.
//! #6712: scratch crates under `temp_dir()` must create that redirected root first.
//! Each ready inline stub is written into a fresh copy of one fixture crate,
//! the file is compiled with rustc as a test crate, and the stub's test must
//! run and stop at its own `ripr:` `todo!()`. Integration-file stubs take a
//! cargo-built fixture with an established `tests/` layout, then
//! `cargo test --test <stem>` from the stub's own `run_command`. Cases the
//! producer cannot make compile must be refused with a named reason instead.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const FIXTURE: &str = r#"pub const LIMIT: u32 = 10;

pub fn price(amount: u32, threshold: u32) -> u32 {
    if amount >= threshold { amount - 10 } else { amount }
}

pub struct Opaque(u32);

pub fn wrap(n: u32) -> Opaque {
    if n > LIMIT { Opaque(n) } else { Opaque(0) }
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    Empty,
    TooLong(usize),
}

impl ParseError {
    pub fn from_code(code: u8) -> Result<u8, Self> {
        if code == 0 {
            return Err(Self::Empty);
        }
        Ok(code)
    }
}

pub struct Parser {
    pub max: usize,
}

impl Parser {
    pub fn parse(&self, input: &str) -> Result<usize, ParseError> {
        if input.len() > self.max {
            return Err(ParseError::TooLong(input.len()));
        }
        Ok(input.len())
    }

    pub fn clamp(&self, subject: usize) -> usize {
        if subject > self.max { self.max } else { subject }
    }

    pub fn boxed(self: Box<Self>, n: usize) -> usize {
        if n > 7 { n } else { self.max }
    }
}

impl std::fmt::Display for Parser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.max > 3 { write!(f, "big") } else { write!(f, "small") }
    }
}

pub type Res<T> = std::result::Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {
    Bad,
}

pub fn checked(n: u8) -> Res<u8> {
    if n > 5 { Err(AppError::Bad) } else { Ok(n) }
}

pub fn read_len(n: u8) -> std::io::Result<u8> {
    if n > 4 { Ok(n) } else { Err(std::io::Error::other("short")) }
}

pub struct Duration(pub u64);

pub fn wait(n: u64) -> Duration {
    if n > 9 { Duration(n) } else { Duration(0) }
}

use std::sync::Mutex as Vec;

pub fn lock(n: u8) -> Vec<u8> {
    if n > 6 { Vec::new(n) } else { Vec::new(0) }
}

pub mod shapes {
    pub struct S {
        pub v: u8,
    }

    impl self::S {
        pub fn get(&self, n: u8) -> u8 {
            if n > 2 { n } else { self.v }
        }

        pub fn merge(&self, other: Self, n: u8) -> u8 {
            if n > 1 { other.v } else { self.v }
        }
    }

    #[derive(Debug, PartialEq)]
    pub enum E {
        Bad,
    }

    impl self::E {
        pub fn check(n: u8) -> Result<u8, Self> {
            if n == 8 {
                return Err(Self::Bad);
            }
            Ok(n)
        }
    }

    #[cfg(test)]
    mod tests {}
}

pub mod inner {
    pub struct Cfg {
        pub on: bool,
    }

    pub fn flag(cfg: &self::Cfg, n: i32) -> bool {
        if n < 0 { cfg.on } else { !cfg.on }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn smoke() {}
    }
}
"#;

/// #5471: owner shapes from real crates (bytesize, humantime, semver) that
/// were refused with reasons that did not name the blocker.
const FIXTURE_5471: &str = r#"use std::str::FromStr;

pub fn early(x: u32) -> u32 {
    if x > 40 { 1 } else { 0 }
}

#[cfg(all(test, feature = "slow"))]
mod slow_tests {
    #[test]
    fn smoke() {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

pub struct Parser<'a> {
    pub src: &'a str,
}

impl<'a> Parser<'a> {
    pub fn parse_unit(&mut self, start: usize, end: usize) -> Result<u64, String> {
        if end > start { Ok(1) } else { Err(String::from(self.src)) }
    }
}

#[derive(Debug, PartialEq)]
pub enum Unit {
    Second,
    Minute,
}

#[derive(Debug, PartialEq)]
pub enum UnitError {
    Unknown,
}

impl FromStr for Unit {
    type Err = UnitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "s" => Ok(Unit::Second),
            "m" => Ok(Unit::Minute),
            _ => Err(UnitError::Unknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn next_minor(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor + 1,
            patch: 0,
        }
    }
}

pub struct Wrap<T>(pub T);

impl<T: Copy> Wrap<T> {
    pub fn pick(&self, n: u8) -> u8 {
        if n > 3 { n } else { 0 }
    }
}

pub struct Local;

const _: () = {
    impl Local {
        pub fn hidden(&self, n: u8) -> u8 {
            if n > 11 { n } else { 0 }
        }
    }
};

#[cfg(test)]
mod more_tests {
    #[test]
    fn smoke() {}
}
"#;

enum Expect {
    /// The stub is written, compiles, and its test stops at a `ripr:` todo.
    StopsAtRiprTodo,
    /// No stub; the refusal names this kind.
    Refused(&'static str),
}

#[test]
fn written_stubs_compile_and_stop_at_their_own_todo() -> Result<(), String> {
    let cases = [
        ("amount >= threshold", Expect::StopsAtRiprTodo),
        // Return type with no visible PartialEq: the assertion is the fill-in.
        ("n > LIMIT", Expect::StopsAtRiprTodo),
        // `Self::Empty` inside `impl ParseError` is respelled from the test.
        ("code == 0", Expect::StopsAtRiprTodo),
        ("return Err(ParseError::TooLong", Expect::StopsAtRiprTodo),
        // A parameter named `subject` must not shadow the receiver.
        ("subject > self.max", Expect::StopsAtRiprTodo),
        // `self::Cfg` is respelled for the child test module.
        ("n < 0", Expect::StopsAtRiprTodo),
        // Result aliases, qualified paths and shadowed std names are not
        // assumed comparable: the assertion is the fill-in.
        ("n > 5 { Err", Expect::StopsAtRiprTodo),
        ("n > 4 { Ok", Expect::StopsAtRiprTodo),
        ("n > 9", Expect::StopsAtRiprTodo),
        // `impl self::S` is respelled for the child test module.
        ("n > 2 { n }", Expect::StopsAtRiprTodo),
        // `Self` in a parameter or error pattern is respelled once, not twice.
        ("n > 1 { other.v }", Expect::StopsAtRiprTodo),
        ("n == 8", Expect::StopsAtRiprTodo),
        // A type imported under a std name is not assumed comparable.
        ("n > 6", Expect::StopsAtRiprTodo),
        ("n > 7", Expect::Refused("owner_unsupported")),
        // A trait method with a `&self` receiver is called through
        // `<Parser as std::fmt::Display>::fmt(&subject, ..)`.
        ("self.max > 3", Expect::StopsAtRiprTodo),
    ]
    .map(|(needle, expect)| (FIXTURE, needle, expect));
    let cases_5471 = [
        // Two plain inline test modules: the nearest one after the owner.
        // The nearer `cfg(all(test, feature = "slow"))` module is skipped;
        // a stub there would not run in this plain test build.
        ("x > 40", Expect::StopsAtRiprTodo),
        // `impl<'a> Parser<'a>` with a `&mut self` method.
        ("end > start", Expect::StopsAtRiprTodo),
        // `impl FromStr for Unit` returning `Result<Self, Self::Err>`.
        ("\"m\" =>", Expect::StopsAtRiprTodo),
        // A field of the struct literal the owner returns.
        ("minor: self.minor + 1", Expect::StopsAtRiprTodo),
        ("n > 3 { n }", Expect::Refused("owner_generic_impl")),
        // An impl inside a `const _` block cannot be named from a test module.
        ("n > 11", Expect::Refused("owner_unsupported")),
    ]
    .map(|(needle, expect)| (FIXTURE_5471, needle, expect));
    for (fixture, needle, expect) in cases.into_iter().chain(cases_5471) {
        let line = fixture
            .lines()
            .position(|text| text.contains(needle))
            .map(|index| index + 1)
            .ok_or_else(|| format!("fixture has `{needle}`"))?;
        let scratch = Scratch::new()?;
        let root = scratch.directory.clone();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"stub_oracle\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), fixture).map_err(|error| error.to_string())?;

        let stub = ripr_stub_write(&root, &format!("src/lib.rs:{line}"));
        let output = run_bounded(stub, &root, "stub", Duration::from_mins(2))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        match expect {
            Expect::Refused(kind) => {
                assert_eq!(output.status.code(), Some(3), "{needle}: {stdout}{stderr}");
                assert!(
                    stderr.contains(&format!("\"kind\": \"{kind}\"")),
                    "{needle}: {stderr}"
                );
                assert_eq!(
                    std::fs::read_to_string(root.join("src/lib.rs"))
                        .map_err(|error| error.to_string())?,
                    fixture,
                    "{needle}: a refusal must not touch the file"
                );
            }
            Expect::StopsAtRiprTodo => {
                assert!(output.status.success(), "{needle}: {stdout}{stderr}");
                let document: serde_json::Value = serde_json::from_str(&stdout)
                    .map_err(|error| format!("{needle}: stub JSON: {error}: {stdout}"))?;
                assert_eq!(document["written"], true, "{needle}");
                if fixture == FIXTURE_5471 {
                    // Two inline test modules: one is picked, not refused.
                    assert_eq!(
                        document["placement"]["kind"], "existing_inline_module",
                        "{needle}"
                    );
                }
                let test_name = document["test_name"]
                    .as_str()
                    .ok_or_else(|| format!("{needle}: test_name"))?
                    .to_string();
                if needle == "x > 40" {
                    // The stub lands in `tests`, the nearest plain module
                    // after `early`, not the gated one or `more_tests`.
                    let written = std::fs::read_to_string(root.join("src/lib.rs"))
                        .map_err(|error| error.to_string())?;
                    let module = |name: &str| {
                        written
                            .find(&format!("mod {name} {{"))
                            .ok_or_else(|| format!("{needle}: `mod {name}` is kept"))
                    };
                    let stub = written
                        .find(&format!("fn {test_name}("))
                        .ok_or_else(|| format!("{needle}: the stub is written"))?;
                    // The closing brace of `mod tests`, so a top-level test
                    // written between the two modules does not pass.
                    let open = module("tests")? + "mod tests ".len();
                    let mut depth = 0usize;
                    let close = written[open..]
                        .char_indices()
                        .find_map(|(offset, ch)| {
                            match ch {
                                '{' => depth += 1,
                                '}' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        return Some(open + offset);
                                    }
                                }
                                _ => {}
                            }
                            None
                        })
                        .ok_or_else(|| format!("{needle}: `mod tests` closes"))?;
                    assert!(
                        open < stub && stub < close && close < module("more_tests")?,
                        "{needle}: the stub goes in `tests`:\n{written}"
                    );
                }
                let binary = root.join(format!("stub_oracle{}", std::env::consts::EXE_SUFFIX));
                let mut rustc = Command::new("rustc");
                rustc
                    .args(["--edition=2024", "--test", "--crate-name", "stub_oracle"])
                    .arg(root.join("src/lib.rs"))
                    .arg("-o")
                    .arg(&binary);
                let build = run_bounded(rustc, &root, "rustc", Duration::from_mins(2))?;
                assert!(
                    build.status.success(),
                    "{needle}: the written stub must compile:\n{}\n{}",
                    String::from_utf8_lossy(&build.stderr),
                    std::fs::read_to_string(root.join("src/lib.rs")).unwrap_or_default()
                );
                let mut test = Command::new(&binary);
                test.arg(&test_name);
                let run = run_bounded(test, &root, "test", Duration::from_secs(30))?;
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&run.stdout),
                    String::from_utf8_lossy(&run.stderr)
                );
                assert!(
                    !run.status.success(),
                    "{needle}: the stub must fail: {text}"
                );
                assert!(text.contains("1 failed"), "{needle}: one test ran: {text}");
                assert!(
                    text.contains("not yet implemented: ripr:"),
                    "{needle}: the stub must stop at its own todo: {text}"
                );
            }
        }
        scratch.cleanup()?;
    }
    Ok(())
}

/// The `ripr` binary under test.
fn ripr_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
}

/// #5471: `ripr check` prints `Write a test for it:` only when the command
/// it prints produces a stub. `src/lib.rs` holds seams that sort before the
/// other files; `src/gated.rs` puts its owner behind a feature cfg a plain
/// `cargo test` build may not enable, so its stub is refused; `src/zz.rs` is
/// stubbable; `src/pair.rs` has two predicate seams on one line, which
/// `--at` and `--kind` cannot tell apart; `src/parse.rs` changes a `?` whose
/// function holds a stubbable predicate but no stubbable error variant.
const ROUTE_LIB: &str = "pub mod gated;\npub mod pair;\npub mod parse;\npub mod zz;\n\npub fn price(amount: u32, threshold: u32) -> u32 {\n    if amount >= threshold { amount - 10 } else { amount }\n}\n\npub fn small(n: u32) -> bool {\n    n < 3\n}\n";
const ROUTE_GATED: &str = "#[cfg(feature = \"extra\")] pub fn clamp(n: u32, max: u32) -> u32 {\n    if n > max { max } else { n }\n}\n";
const ROUTE_PAIR: &str = "pub fn both(a: u32, b: u32) -> u32 {\n    if a > 10 && b > 20 { 1 } else { 0 }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn a_boundary() {\n        assert_eq!(both(9, 30), 0);\n        assert_eq!(both(10, 30), 0);\n        assert_eq!(both(11, 30), 1);\n    }\n}\n";
const ROUTE_PARSE: &str = "pub fn read(s: &str) -> Result<u32, std::num::ParseIntError> {\n    let n: u32 = s.parse()?;\n    if n > 5 { Ok(n) } else { Ok(0) }\n}\n";
const ROUTE_ZZ: &str =
    "pub fn fee(n: u32, cap: u32) -> u32 {\n    if n >= cap { cap } else { n }\n}\n";

fn route_crate(scratch: &Scratch) -> Result<(), String> {
    let root = &scratch.directory;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"stub_route\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    for (file, text) in [
        ("src/lib.rs", ROUTE_LIB),
        ("src/gated.rs", ROUTE_GATED),
        ("src/pair.rs", ROUTE_PAIR),
        ("src/parse.rs", ROUTE_PARSE),
        ("src/zz.rs", ROUTE_ZZ),
    ] {
        std::fs::write(root.join(file), text).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// A one-line diff that changes `old` into line 2 of `file`.
fn one_line_diff(scratch: &Scratch, file: &str, old: &str, new: &str) -> Result<PathBuf, String> {
    let path = scratch
        .directory
        .join(format!("{}.diff", file.replace(['/', '.'], "_")));
    std::fs::write(
        &path,
        format!(
            "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -2 +2 @@\n-{old}\n+{new}\n"
        ),
    )
    .map_err(|error| error.to_string())?;
    Ok(path)
}

fn check_human(root: &Path, diff: &Path) -> Result<String, String> {
    let mut check = ripr_command();
    check
        .args(["check", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(diff);
    let output = run_bounded(check, root, "check", Duration::from_mins(2))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.status.success() {
        return Err(format!(
            "check failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(stdout)
}

/// The command `check` printed under `Write a test for it:`, as arguments
/// after `ripr`.
fn printed_stub_args(stdout: &str) -> Option<Vec<String>> {
    let mut lines = stdout.lines();
    lines.find(|line| *line == "Write a test for it:")?;
    let command = lines.next()?.trim();
    let mut words = command.split_whitespace();
    (words.next() == Some("ripr")).then(|| words.map(str::to_string).collect())
}

#[test]
fn check_prints_the_stub_route_only_when_the_printed_command_yields_a_stub() -> Result<(), String> {
    let scratch = Scratch::new()?;
    route_crate(&scratch)?;
    let root = scratch.directory.clone();

    // A refused location prints the refusal, never the route.
    let gated = one_line_diff(
        &scratch,
        "src/gated.rs",
        "    if n >= max { max } else { n }",
        "    if n > max { max } else { n }",
    )?;
    let stdout = check_human(&root, &gated)?;
    assert!(
        stdout.contains("src/gated.rs:2"),
        "the selected finding is the gated.rs change: {stdout}"
    );
    assert!(
        !stdout.contains("Write a test for it:"),
        "a refused stub must not be offered: {stdout}"
    );
    assert!(
        stdout.contains("No test stub here: the owner is a trait default method or nested function, sits in an impl local to a block or whose self type is not a plain path, sits behind a cfg a plain `cargo test` build may not enable, or takes a typed `self` receiver\n"),
        "the refusal reason is printed instead: {stdout}"
    );

    // Two seams of the finding's kind on its line: the route could stub the
    // `a > 10` boundary the tests already pin, so no route is printed.
    let pair = one_line_diff(
        &scratch,
        "src/pair.rs",
        "    if a > 10 && b >= 20 { 1 } else { 0 }",
        "    if a > 10 && b > 20 { 1 } else { 0 }",
    )?;
    let stdout = check_human(&root, &pair)?;
    assert!(
        stdout.contains("src/pair.rs:2"),
        "the selected finding is the pair.rs change: {stdout}"
    );
    assert!(
        !stdout.contains("Write a test for it:"),
        "an ambiguous location must not be offered: {stdout}"
    );

    // An error_path finding never answers with the `n > 5` predicate stub on
    // the next line: a seam of another kind does not speak for it.
    let parse = one_line_diff(
        &scratch,
        "src/parse.rs",
        "    let n: u32 = s.parse().unwrap_or(0);",
        "    let n: u32 = s.parse()?;",
    )?;
    let stdout = check_human(&root, &parse)?;
    assert!(
        stdout.contains("src/parse.rs:2"),
        "the selected finding is the parse.rs change: {stdout}"
    );
    assert!(
        !stdout.contains("Write a test for it:") && !stdout.contains("No test stub here"),
        "another kind's seam must not answer for the finding: {stdout}"
    );

    // A stale patch: its added line is not the disk's line 2, so the
    // resolver (which reads the disk) would answer for other text.
    let stale = one_line_diff(
        &scratch,
        "src/zz.rs",
        "    if n >= cap { cap } else { n }",
        "    if n > cap { cap } else { n }",
    )?;
    let stdout = check_human(&root, &stale)?;
    assert!(stdout.contains("src/zz.rs:2"), "{stdout}");
    assert!(
        !stdout.contains("Write a test for it:") && !stdout.contains("No test stub here"),
        "a patch that disagrees with the disk must not route: {stdout}"
    );

    // A stubbable location prints the route, and the printed command, run
    // as printed, yields a stub through the same resolver.
    let zz = one_line_diff(
        &scratch,
        "src/zz.rs",
        "    if n > cap { cap } else { n }",
        "    if n >= cap { cap } else { n }",
    )?;
    let stdout = check_human(&root, &zz)?;
    assert!(!stdout.contains("No test stub here"), "{stdout}");
    let args = printed_stub_args(&stdout)
        .ok_or_else(|| format!("a stubbable gap prints the route: {stdout}"))?;
    assert!(
        args.iter().any(|arg| arg == "src/zz.rs:2"),
        "the route names the finding location: {args:?}"
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--kind" && pair[1] == "predicate"),
        "the route carries the finding's probe family: {args:?}"
    );
    // The full inventory, capped to one seam, holds only a `src/lib.rs`
    // seam; the location-scoped resolver must not depend on it.
    let mut stub = ripr_command();
    stub.args(&args)
        .arg("--json")
        .env("RIPR_REPO_EXPOSURE_SEAM_LIMIT", "1");
    let output = run_bounded(stub, &root, "stub", Duration::from_mins(2))?;
    let stub_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "the printed route must yield a stub: {stub_stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_str(&stub_stdout)
        .map_err(|error| format!("stub JSON: {error}: {stub_stdout}"))?;
    assert_eq!(document["owner"], "src/zz.rs::fee", "{stub_stdout}");
    assert_eq!(document["written"], false, "{stub_stdout}");
    scratch.cleanup()
}

/// #5471: without `--kind` no `check` finding vouches for the location, so a
/// bare `--at` keeps the gap filter: a seam the tests already pin is not
/// stubbed, while an unpinned one in the same file still is.
#[test]
fn bare_at_stubs_only_a_reported_gap() -> Result<(), String> {
    const PINNED: &str = "pub fn gate(a: u32) -> bool {\n    a > 10\n}\n\npub fn loose(b: u32) -> bool {\n    b > 20\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn gate_boundary() {\n        assert_eq!(gate(10), false);\n        assert_eq!(gate(11), true);\n    }\n}\n";
    let scratch = Scratch::new()?;
    let root = scratch.directory.clone();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"stub_bare\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/lib.rs"), PINNED).map_err(|error| error.to_string())?;
    let stub_at = |at: &str| -> Result<Output, String> {
        let mut stub = ripr_command();
        stub.args(["agent", "stub", "--root"])
            .arg(&root)
            .args(["--at", at, "--json"]);
        run_bounded(stub, &root, "stub", Duration::from_mins(2))
    };

    let pinned = stub_at("src/lib.rs:2")?;
    let stdout = String::from_utf8_lossy(&pinned.stdout).to_string();
    let stderr = String::from_utf8_lossy(&pinned.stderr).to_string();
    assert!(
        !pinned.status.success(),
        "a seam the tests pin is no gap to stub: {stdout}{stderr}"
    );
    assert!(stderr.contains("no reported gap"), "{stderr}");

    let open = stub_at("src/lib.rs:6")?;
    let stdout = String::from_utf8_lossy(&open.stdout).to_string();
    assert!(
        open.status.success(),
        "an unpinned seam is still stubbed: {stdout}{}",
        String::from_utf8_lossy(&open.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|error| format!("stub JSON: {error}: {stdout}"))?;
    assert_eq!(document["owner"], "src/lib.rs::loose", "{stdout}");
    scratch.cleanup()
}

/// #6689 items 1–2: refusal hints keep the requested family and never route
/// the caller to a nearer seam of another family.
const KIND_REFUSAL_SOURCE: &str = "pub fn plain(n: u8) -> u8 {\n    if n > 3 { n } else { 0 }\n}\n\npub fn fallible(input: &str) -> Result<u8, E> {\n    if input.is_empty() {\n        return Err(E::Bad);\n    }\n    Ok(1)\n}\n\n#[derive(Debug, PartialEq)]\npub enum E { Bad }\n";

const KIND_NEAREST_SOURCE: &str = "pub fn plain(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> u8 {\n    if a > 1 && b > 2 && c > 3 && d > 4 && e > 5 && f > 6 { a } else { b }\n}\n\npub fn fallible(input: &str) -> Result<u8, E> {\n    if input.is_empty() {\n        return Err(E::Bad);\n    }\n    Ok(1)\n}\n\n#[derive(Debug, PartialEq)]\npub enum E { Bad }\n";

fn kind_refusal_crate(source: &str) -> Result<Scratch, String> {
    let scratch = Scratch::new()?;
    std::fs::write(
        scratch.directory.join("Cargo.toml"),
        "[package]\nname = \"stub_kind_refusal\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.directory.join("src/lib.rs"), source)
        .map_err(|error| error.to_string())?;
    Ok(scratch)
}

fn kind_refusal_command(root: &Path, at: &str, kind: Option<&str>) -> Command {
    let mut stub = ripr_command();
    stub.args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at, "--json"]);
    if let Some(kind) = kind {
        stub.args(["--kind", kind]);
    }
    stub
}

#[test]
fn kind_not_found_refusal_preserves_the_requested_probe_family() -> Result<(), String> {
    let scratch = kind_refusal_crate(KIND_REFUSAL_SOURCE)?;
    let root = &scratch.directory;
    assert_eq!(
        KIND_REFUSAL_SOURCE.lines().nth(1),
        Some("    if n > 3 { n } else { 0 }")
    );
    let predicate = run_bounded(
        kind_refusal_command(root, "src/lib.rs:2", Some("predicate")),
        root,
        "predicate-control",
        Duration::from_mins(2),
    )?;
    assert!(
        predicate.status.success(),
        "the wrong-family subject must be stubbable: {}{}",
        String::from_utf8_lossy(&predicate.stdout),
        String::from_utf8_lossy(&predicate.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_slice(&predicate.stdout).map_err(|error| error.to_string())?;
    assert_eq!(document["state"], "ready");
    assert_eq!(document["owner"], "src/lib.rs::plain");
    assert_eq!(document["test_name"], "plain_boundary_discriminator");
    assert_eq!(document["written"], false);

    let refused = run_bounded(
        kind_refusal_command(root, "src/lib.rs:2", Some("error_path")),
        root,
        "family-refusal",
        Duration::from_mins(2),
    )?;
    assert_eq!(refused.status.code(), Some(3));
    assert!(
        refused.stdout.is_empty(),
        "a not-found decision has no JSON"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.starts_with(
            "ripr: agent stub: no error_path seam ripr can stub is in the function at src/lib.rs:2; nearest: ",
        ),
        "the refusal must echo the requested probe family: {stderr}"
    );
    assert!(
        !stderr.contains("no error_variant "),
        "the internal seam kind is not the requested family: {stderr}"
    );
    scratch.cleanup()
}

#[test]
fn kind_nearest_hint_filters_before_the_cap_and_recovers_the_error_stub() -> Result<(), String> {
    let scratch = kind_refusal_crate(KIND_NEAREST_SOURCE)?;
    let root = &scratch.directory;
    assert_eq!(
        KIND_NEAREST_SOURCE.lines().nth(1),
        Some("    if a > 1 && b > 2 && c > 3 && d > 4 && e > 5 && f > 6 { a } else { b }")
    );
    assert_eq!(
        KIND_NEAREST_SOURCE.lines().nth(6),
        Some("        return Err(E::Bad);")
    );
    // Six closer predicate seams make filtering after take(5) lose the
    // matching seam. The ready error control also identifies the seam:
    // a return-value seam shares line 7 and is not an error-path suggestion.
    let direct = run_bounded(
        kind_refusal_command(root, "src/lib.rs:7", Some("error_path")),
        root,
        "error-control",
        Duration::from_mins(2),
    )?;
    assert!(
        direct.status.success(),
        "the matching subject must be stubbable: {}{}",
        String::from_utf8_lossy(&direct.stdout),
        String::from_utf8_lossy(&direct.stderr)
    );
    let control: serde_json::Value =
        serde_json::from_slice(&direct.stdout).map_err(|error| error.to_string())?;
    assert_eq!(control["state"], "ready");
    assert_eq!(control["owner"], "src/lib.rs::fallible");
    assert_eq!(control["test_name"], "fallible_exact_error_variant");
    assert_eq!(control["written"], false);
    let expected_id = control["seam_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("the ready error control needs a seam ID: {control}"))?;
    let text = control["text"]
        .as_str()
        .ok_or_else(|| format!("the ready error control needs test text: {control}"))?;
    assert!(text.contains("let actual = fallible(input);"), "{text}");
    assert!(
        text.contains(
            "assert!(matches!(actual, Err(E::Bad { .. })), \"expected Err(E::Bad {{ .. }})\");",
        ),
        "the control observes the error variant, not a nearby predicate: {text}"
    );

    let refused = run_bounded(
        kind_refusal_command(root, "src/lib.rs:2", Some("error_path")),
        root,
        "nearest-refusal",
        Duration::from_mins(2),
    )?;
    assert_eq!(refused.status.code(), Some(3));
    assert!(
        refused.stdout.is_empty(),
        "a not-found decision has no JSON"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    let (_, tail) = stderr
        .split_once("; nearest: ")
        .ok_or_else(|| format!("the refusal needs a nearest section: {stderr}"))?;
    let (nearest, _) = tail
        .split_once(". Run ")
        .ok_or_else(|| format!("the refusal needs its follow-up route: {stderr}"))?;
    assert_eq!(
        nearest,
        format!("src/lib.rs:7 (--seam-id {expected_id})"),
        "only the matching error seam may be suggested: {stderr}"
    );

    // Follow the actual emitted location with the original family.
    let (at, emitted_id) = nearest
        .split_once(" (--seam-id ")
        .ok_or_else(|| format!("the suggestion needs a location and seam ID: {nearest}"))?;
    let emitted_id = emitted_id
        .strip_suffix(')')
        .ok_or_else(|| format!("the suggestion needs a complete seam ID: {nearest}"))?;
    let recovered = run_bounded(
        kind_refusal_command(root, at, Some("error_path")),
        root,
        "nearest-recovery",
        Duration::from_mins(2),
    )?;
    assert!(
        recovered.status.success(),
        "the emitted location must recover with the same family: {}{}",
        String::from_utf8_lossy(&recovered.stdout),
        String::from_utf8_lossy(&recovered.stderr)
    );
    let recovered: serde_json::Value =
        serde_json::from_slice(&recovered.stdout).map_err(|error| error.to_string())?;
    assert_eq!(recovered["seam_id"], emitted_id);
    assert_eq!(
        recovered, control,
        "recovery must select the same error stub"
    );

    let repeated = run_bounded(
        kind_refusal_command(root, "src/lib.rs:2", Some("error_path")),
        root,
        "nearest-repeat",
        Duration::from_mins(2),
    )?;
    assert_eq!(repeated.status.code(), Some(3));
    assert_eq!(repeated.stdout, refused.stdout);
    assert_eq!(repeated.stderr, refused.stderr);
    assert_eq!(
        std::fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?,
        KIND_NEAREST_SOURCE
    );
    scratch.cleanup()
}

#[test]
fn kind_not_found_without_a_matching_family_suggests_no_other_seams() -> Result<(), String> {
    const PLAIN_ONLY: &str = "pub fn plain(n: u8) -> u8 {\n    if n > 3 { n } else { 0 }\n}\n";
    let scratch = kind_refusal_crate(PLAIN_ONLY)?;
    let root = &scratch.directory;
    for (kind, label) in [
        (Some("predicate"), "predicate-control"),
        (None, "bare-control"),
    ] {
        let ready = run_bounded(
            kind_refusal_command(root, "src/lib.rs:2", kind),
            root,
            label,
            Duration::from_mins(2),
        )?;
        assert!(
            ready.status.success(),
            "the plain seam remains available to its family and bare selectors: {}{}",
            String::from_utf8_lossy(&ready.stdout),
            String::from_utf8_lossy(&ready.stderr)
        );
        let document: serde_json::Value =
            serde_json::from_slice(&ready.stdout).map_err(|error| error.to_string())?;
        assert_eq!(document["state"], "ready");
        assert_eq!(document["owner"], "src/lib.rs::plain");
        assert_eq!(document["test_name"], "plain_boundary_discriminator");
        assert_eq!(document["written"], false);
    }

    let refused = run_bounded(
        kind_refusal_command(root, "src/lib.rs:2", Some("error_path")),
        root,
        "absent-family",
        Duration::from_mins(2),
    )?;
    assert_eq!(refused.status.code(), Some(3));
    assert!(
        refused.stdout.is_empty(),
        "a not-found decision has no JSON"
    );
    let stderr = String::from_utf8_lossy(&refused.stderr);
    let (_, tail) = stderr
        .split_once("; nearest: ")
        .ok_or_else(|| format!("the refusal needs a nearest section: {stderr}"))?;
    let (nearest, _) = tail
        .split_once(". Run ")
        .ok_or_else(|| format!("the refusal needs its follow-up route: {stderr}"))?;
    assert_eq!(
        nearest, "",
        "a file with only other families must not offer their seams: {stderr}"
    );
    assert!(
        stderr.starts_with(
            "ripr: agent stub: no error_path seam ripr can stub is in the function at src/lib.rs:2; nearest: ",
        ),
        "{stderr}"
    );
    scratch.cleanup()
}

// #7055 A1: this bridge consumes real review cards through first-action,
// using this file's existing bounded process and exclusive scratch owners.
fn repair_start_fixture(
    fixture: &str,
    test_file: &str,
    old: &str,
    new: &str,
) -> Result<Scratch, String> {
    let scratch = Scratch::new()?;
    let root = &scratch.directory;
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(fixture)
        .join("input");
    std::fs::create_dir(root.join("tests")).map_err(|error| error.to_string())?;
    for relative in ["Cargo.toml", "src/lib.rs", test_file] {
        let mut text = std::fs::read_to_string(source.join(relative))
            .map_err(|error| format!("read {fixture}/{relative}: {error}"))?;
        if relative == "Cargo.toml" {
            text.push_str("\n[workspace]\n");
        }
        std::fs::write(root.join(relative), text).map_err(|error| error.to_string())?;
    }
    let head =
        std::fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
    assert_eq!(
        head.matches(new).count(),
        1,
        "fixture change must be unique"
    );
    std::fs::write(root.join("src/lib.rs"), head.replace(new, old))
        .map_err(|error| error.to_string())?;
    git(root, root, &["init", "-q"])?;
    git(root, root, &["add", "Cargo.toml", "src", "tests"])?;
    git(root, root, &["commit", "-q", "-m", "base"])?;
    std::fs::write(root.join("src/lib.rs"), head).map_err(|error| error.to_string())?;
    git(root, root, &["add", "src/lib.rs"])?;
    git(root, root, &["commit", "-q", "-m", "change"])?;
    Ok(scratch)
}

fn repair_start_review(scratch: &Scratch) -> Result<serde_json::Value, String> {
    let root = &scratch.directory;
    let out = root.join("review.json");
    let mut command = ripr_command();
    isolate_from_outer_repo(&mut command)
        .current_dir(root)
        .env("RIPR_CACHE_DIR", root.join("fixture-cache"))
        .args(["review-comments", "--root"])
        .arg(root)
        .args(["--base", "HEAD~1", "--head", "HEAD", "--out"])
        .arg(&out);
    let output = run_bounded(command, root, "review", Duration::from_mins(2))?;
    assert!(
        output.status.success(),
        "review producer must complete: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&read_bounded_stream(&out)?)
        .map_err(|error| format!("parse real review cards: {error}"))
}

fn repair_start_action(
    scratch: &Scratch,
    guidance: &serde_json::Value,
    label: &str,
) -> Result<(serde_json::Value, String), String> {
    let root = &scratch.directory;
    let input = root.join("guidance.json");
    let out = root.join("action.json");
    let md = root.join("action.md");
    // A successful no-op must not inherit an earlier report.
    for path in [&out, &md] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("remove owned report {}: {error}", path.display())),
        }
    }
    std::fs::write(
        &input,
        serde_json::to_vec(guidance).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut command = ripr_command();
    isolate_from_outer_repo(&mut command)
        .current_dir(root)
        .env("RIPR_CACHE_DIR", root.join("fixture-cache"))
        .args(["first-action", "--root"])
        .arg(root)
        .arg("--pr-guidance")
        .arg(&input)
        .arg("--out")
        .arg(&out)
        .arg("--out-md")
        .arg(&md);
    let output = run_bounded(command, root, label, Duration::from_mins(2))?;
    assert!(
        output.status.success(),
        "first-action must render a decision: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = serde_json::from_slice(&read_bounded_stream(&out)?)
        .map_err(|error| format!("parse first-action decision: {error}"))?;
    let markdown =
        String::from_utf8(read_bounded_stream(&md)?).map_err(|error| error.to_string())?;
    Ok((report, markdown))
}

/// Only the independently validated generation timestamp varies by call.
fn repair_start_semantic_report(report: &serde_json::Value) -> Result<serde_json::Value, String> {
    let generated = report["generated_at"]
        .as_str()
        .and_then(|value| value.strip_prefix("unix_ms:"))
        .ok_or_else(|| format!("report needs its actual generation timestamp: {report}"))?;
    let millis = generated
        .parse::<u128>()
        .map_err(|error| format!("invalid generation timestamp {generated}: {error}"))?;
    assert!(
        millis > 0,
        "the report must carry an actual generation timestamp"
    );
    let mut semantic = report.clone();
    semantic
        .as_object_mut()
        .ok_or("report must be an object")?
        .remove("generated_at")
        .ok_or("report must carry generated_at")?;
    Ok(semantic)
}

#[test]
fn first_action_real_repair_card_uses_neutral_rationale_and_recovers() -> Result<(), String> {
    let scratch = repair_start_fixture(
        "boundary_gap",
        "tests/pricing.rs",
        "amount > discount_threshold",
        "amount >= discount_threshold",
    )?;
    let root = &scratch.directory;
    let original = std::fs::read(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
    let guidance = repair_start_review(&scratch)?;
    let cards = guidance["comments"]
        .as_array()
        .ok_or("producer has no comments array")?;
    let card = cards
        .iter()
        .find(|card| {
            card["owner"] == "src/lib.rs::discounted_total"
                && card["kind"] == "predicate_boundary"
                && card["seam"]["line"] == 2
        })
        .ok_or_else(|| format!("producer must emit the changed predicate card: {guidance}"))?;
    assert_eq!(card["grip_class"], "weakly_gripped");
    assert_eq!(card["gap_state"], "actionable");
    assert_eq!(card["seam"]["expression"], "amount >= discount_threshold");
    assert_eq!(
        card["missing_discriminator"],
        "discount_threshold (equality boundary)"
    );
    let repair = card["llm_guidance"]["repair_command"]
        .as_str()
        .filter(|command| !command.is_empty())
        .ok_or_else(|| format!("real card must carry an admitted repair start: {card}"))?;

    let (action, markdown) = repair_start_action(&scratch, &guidance, "action-first")?;
    assert_eq!(action["status"], "actionable");
    assert_eq!(action["action_kind"], "write_focused_test");
    assert_eq!(action["selected"]["source"], "pr_guidance");
    assert_eq!(action["selected"]["seam_id"], card["seam_id"]);
    assert_eq!(action["selected"]["path"], "src/lib.rs");
    assert_eq!(action["selected"]["line"], 2);
    assert_eq!(action["selected"]["classification"], "weakly_exposed");
    assert_eq!(
        action["selected"]["changed_behavior"],
        "amount >= discount_threshold"
    );
    assert_eq!(
        action["selected"]["missing_discriminator"],
        "discount_threshold (equality boundary)"
    );
    assert_eq!(action["commands"]["repair"], repair);
    for field in ["analysis_outcome", "verify"] {
        assert_eq!(
            action["commands"][field],
            card["llm_guidance"][format!("{field}_command")],
            "carried {field} command"
        );
    }
    assert_eq!(action["commands"]["receipt"], card["receipt_command"]);
    // Independently authored literal; no production vocabulary/helper oracle.
    assert_eq!(
        action["why"],
        "The review card identifies missing discriminator `discount_threshold (equality boundary)` and names its repair start."
    );
    assert_eq!(
        markdown.lines().find(|line| line.starts_with("- Why: ")),
        Some(
            "- Why: The review card identifies missing discriminator `discount_threshold (equality boundary)` and names its repair start.",
        )
    );
    assert!(
        !markdown.contains("a related test reaches this change"),
        "{markdown}"
    );
    let (repeated, repeated_md) = repair_start_action(&scratch, &guidance, "action-repeat")?;
    assert_eq!(
        repair_start_semantic_report(&repeated)?,
        repair_start_semantic_report(&action)?,
        "all fields except the validated timestamp must be deterministic"
    );
    assert_eq!(repeated_md, markdown);

    // Consumer-input recovery controls, preserving the real card's identity.
    for (blank, label) in [(false, "action-missing"), (true, "action-blank")] {
        let mut missing = guidance.clone();
        for bucket in ["comments", "summary_only"] {
            if let Some(cards) = missing
                .get_mut(bucket)
                .and_then(serde_json::Value::as_array_mut)
            {
                for card in cards {
                    if let Some(fields) = card["llm_guidance"].as_object_mut() {
                        fields.remove("repair_command");
                        if blank {
                            fields.insert("repair_command".to_string(), serde_json::json!("  "));
                        }
                    }
                }
            }
        }
        let (refused, refused_md) = repair_start_action(&scratch, &missing, label)?;
        assert_eq!(refused["status"], "missing_required_artifact");
        assert_eq!(refused["action_kind"], "generate_missing_artifact");
        assert!(refused["commands"].get("repair").is_none());
        assert!(!refused_md.contains("## Start Repair"));
    }
    let (recovered, recovered_md) = repair_start_action(&scratch, &guidance, "action-recovery")?;
    assert_eq!(
        repair_start_semantic_report(&recovered)?,
        repair_start_semantic_report(&action)?,
        "restored evidence recovers every field except the validated timestamp"
    );
    assert_eq!(recovered_md, markdown);
    assert_eq!(
        std::fs::read(root.join("src/lib.rs")).map_err(|error| error.to_string())?,
        original
    );
    scratch.cleanup()
}

#[test]
fn first_action_real_callee_only_wrapper_keeps_its_refusal() -> Result<(), String> {
    let scratch = repair_start_fixture(
        "wrapper_seam_callee_call_attribution",
        "tests/attribution.rs",
        "try_parse_summary(raw).map_err(Into::into)",
        "try_parse_summary(raw).map_err(|error| error.to_string().into())",
    )?;
    let root = &scratch.directory;
    let mut check = ripr_command();
    isolate_from_outer_repo(&mut check)
        .current_dir(root)
        .env("RIPR_CACHE_DIR", root.join("fixture-cache"))
        .args(["check", "--root"])
        .arg(root)
        .args(["--base", "HEAD~1", "--mode", "draft", "--json"]);
    let output = run_bounded(check, root, "callee-check", Duration::from_mins(2))?;
    assert!(
        output.status.success(),
        "actual callee-only analysis must complete: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse callee-only analysis: {error}"))?;
    let findings = document["findings"]
        .as_array()
        .ok_or("check has no findings array")?;
    let subjects: Vec<_> = findings
        .iter()
        .filter(|finding| {
            finding["probe"]["line"] == 14
                && finding["probe"]["expression"]
                    == "try_parse_summary(raw).map_err(|error| error.to_string().into())"
        })
        .collect();
    assert_eq!(subjects.len(), 2, "genuine wrapper subjects: {document}");
    assert!(
        subjects
            .iter()
            .any(|f| f["probe"]["family"] == "error_path")
    );
    assert!(
        subjects
            .iter()
            .any(|f| f["probe"]["family"] == "return_value")
    );
    for finding in subjects {
        assert_eq!(finding["classification"], "weakly_exposed");
        assert_eq!(finding["ripr"]["reach"]["state"], "weak");
        assert_eq!(
            finding["static_limit_kind"],
            "wrapper_error_binding_unresolved"
        );
        assert_eq!(finding["missing_discriminators"], serde_json::json!([]));
        let related = finding["related_tests"]
            .as_array()
            .ok_or("subject has no related tests")?;
        // The return-value probe emits one record per assertion. The
        // before-shadow test has two; that test-level association does
        // not prove its later shadowed assertion invokes the owner.
        let mut expected = vec![
            (
                "calls_callee_before_shadow_binding",
                "tests/attribution.rs",
                13,
                "assert_eq!(parsed.map(|n| n), Ok(3));",
            ),
            (
                "observes_callee_outcome",
                "tests/attribution.rs",
                4,
                "assert_eq!(parsed.map(|n| n), Ok(3));",
            ),
        ];
        match finding["probe"]["family"].as_str() {
            Some("error_path") => {}
            Some("return_value") => expected.push((
                "calls_callee_before_shadow_binding",
                "tests/attribution.rs",
                13,
                "assert_eq!(ignored, Ok(5));",
            )),
            other => return Err(format!("unexpected wrapper family: {other:?}")),
        }
        let mut actual = related
            .iter()
            .map(|test| {
                Ok::<_, String>((
                    test["name"].as_str().ok_or("related test has no name")?,
                    test["file"].as_str().ok_or("related test has no file")?,
                    test["line"].as_u64().ok_or("related test has no line")?,
                    test["oracle"]
                        .as_str()
                        .ok_or("related test has no oracle")?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(
            actual, expected,
            "every genuine callee assertion record must match: {finding}"
        );
        for test in related {
            assert_eq!(test["relation_reason"], "seam_callee_call");
            assert_eq!(test["relation_confidence"], "medium");
        }
    }

    let guidance = repair_start_review(&scratch)?;
    // A genuine check Finding is not a RepoSeam card. Keep only actual
    // wrapper cards in this consumer control; no card is invented if the
    // producer refuses to render one.
    let mut wrapper_guidance = guidance.clone();
    for bucket in ["comments", "summary_only"] {
        let cards = wrapper_guidance[bucket]
            .as_array_mut()
            .ok_or_else(|| format!("producer has no {bucket} array"))?;
        cards.retain(|card| card["owner"] == "src/lib.rs::parse_summary");
        for card in cards {
            assert!(
                card["llm_guidance"]["repair_command"]
                    .as_str()
                    .is_none_or(|command| command.trim().is_empty()),
                "the currently limited wrapper must not carry a repair start: {card}"
            );
        }
    }
    let wrapper_card = wrapper_guidance["comments"]
        .as_array()
        .and_then(|cards| cards.first())
        .or_else(|| {
            wrapper_guidance["summary_only"]
                .as_array()
                .and_then(|cards| cards.first())
        })
        .ok_or("producer must retain its genuine non-actionable wrapper card")?;
    assert_eq!(wrapper_card["owner"], "src/lib.rs::parse_summary");
    assert_eq!(wrapper_card["kind"], "return_value");
    assert_eq!(wrapper_card["grip_class"], "ungripped");
    assert_eq!(wrapper_card["seam"]["line"], 14);
    assert_eq!(
        wrapper_card["seam"]["expression"],
        "try_parse_summary(raw).map_err(|error| error.to_string().into())"
    );
    let seam_id = wrapper_card["seam_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("the genuine wrapper card must carry its identity")?;
    // Preserve the producer's global suppression evidence; this does
    // not attribute a per-card suppression cause.
    let (action, markdown) = repair_start_action(&scratch, &wrapper_guidance, "callee-action")?;
    assert_eq!(action["schema_version"], "0.1");
    assert_eq!(action["kind"], "first_useful_action");
    assert!(action["commands"].get("repair").is_none(), "{action}");
    assert_eq!(action["status"], "suppressed", "{action}");
    assert_eq!(action["action_kind"], "no_action");
    assert_eq!(action["fallback"]["kind"], "suppressed");
    assert_eq!(action["why"], "The seam is suppressed or configured off.");
    assert_eq!(action["selected"]["source"], "pr_guidance");
    assert_eq!(action["selected"]["seam_id"], seam_id);
    assert_eq!(action["selected"]["path"], "src/lib.rs");
    assert_eq!(action["selected"]["line"], 14);
    assert_eq!(action["selected"]["seam_kind"], "return_value");
    assert_eq!(action["selected"]["classification"], "no_static_path");
    assert_eq!(
        action["selected"]["changed_behavior"],
        "try_parse_summary(raw).map_err(|error| error.to_string().into())"
    );
    assert_eq!(
        markdown.lines().find(|line| line.starts_with("- Why: ")),
        Some("- Why: The seam is suppressed or configured off.")
    );
    assert!(!markdown.contains("## Start Repair"));
    scratch.cleanup()
}

/// Clears inherited repository selectors so a hook or wrapper that exports
/// them cannot redirect the fixture into the outer repository.
fn isolate_from_outer_repo(command: &mut Command) -> &mut Command {
    command
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
}

/// Runs git in `repo`; its captured streams land in `scratch`, outside it.
fn git(scratch: &Path, repo: &Path, args: &[&str]) -> Result<(), String> {
    let mut git = Command::new("git");
    isolate_from_outer_repo(&mut git)
        .current_dir(repo)
        .args([
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args);
    let output = run_bounded(git, scratch, "git", Duration::from_secs(30))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// #5471: the stub resolver reads the file on disk. A committed-history
/// check reads HEAD content for a file with uncommitted edits, so the route
/// could stub an expression the finding never saw; it is not printed then.
#[test]
fn check_prints_no_stub_route_when_it_analyzed_other_bytes_than_the_disk() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let streams = scratch.directory.clone();
    let root = streams.join("repo");
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"stub_snapshot\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    let lib = root.join("src/lib.rs");
    let source = |op: &str| {
        format!(
            "pub fn fee(n: u32, cap: u32) -> u32 {{\n    if n {op} cap {{ cap }} else {{ n }}\n}}\n"
        )
    };
    std::fs::write(&lib, source(">")).map_err(|error| error.to_string())?;
    git(&streams, &root, &["init", "-q"])?;
    git(&streams, &root, &["add", "."])?;
    git(&streams, &root, &["commit", "-qm", "base"])?;
    std::fs::write(&lib, source(">=")).map_err(|error| error.to_string())?;
    git(&streams, &root, &["commit", "-qam", "change boundary"])?;
    let check = |root: &Path, extra: &[&str]| -> Result<String, String> {
        let mut check = ripr_command();
        isolate_from_outer_repo(&mut check)
            .args(["check", "--root"])
            .arg(root)
            .args(["--base", "HEAD~1"])
            .args(extra);
        let output = run_bounded(check, &streams, "check", Duration::from_mins(2))?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    };

    // Clean tree: HEAD is the disk, so the route is offered.
    let clean = check(&root, &[])?;
    assert!(clean.contains("src/lib.rs:2"), "{clean}");
    assert!(clean.contains("Write a test for it:"), "{clean}");

    // An uncommitted edit with a forced committed-history read: HEAD was
    // analyzed, the disk differs. `--committed` is load-bearing: the
    // dirty-tree default would read the working tree (the disk) instead.
    std::fs::write(&lib, source("<")).map_err(|error| error.to_string())?;
    let dirty = check(&root, &["--committed"])?;
    assert!(dirty.contains("src/lib.rs:2"), "{dirty}");
    assert!(
        !dirty.contains("Write a test for it:") && !dirty.contains("No test stub here"),
        "a route over other bytes must not be offered: {dirty}"
    );
    scratch.cleanup()
}
/// #5453: a producer-admitted `new_integration_file` stub must compile under
/// the cargo command the CLI prints, including a `crate::` parameter type and
/// a crate-root `pub const` boundary. rustc-on-lib.rs cannot see this path.
#[test]
fn written_integration_stub_compiles_under_cargo_test_and_stops_at_its_own_todo()
-> Result<(), String> {
    const SOURCE: &str = r#"pub const LIMIT: u32 = 10;

pub struct Tag;

pub fn price(amount: u32, tag: crate::Tag) -> u32 {
    let _ = tag;
    if amount >= LIMIT { amount - 10 } else { amount }
}

pub mod extra {
    pub struct Unused;
}
"#;
    let scratch = Scratch::new()?;
    let root = scratch.directory.clone();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"stub_integration\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n# Keep this scratch crate out of a parent Cargo workspace.\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/lib.rs"), SOURCE).map_err(|error| error.to_string())?;
    std::fs::create_dir(root.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("tests/smoke.rs"),
        "#[test]\nfn crate_compiles() {}\n",
    )
    .map_err(|error| error.to_string())?;

    let line = SOURCE
        .lines()
        .position(|text| text.contains("amount >= LIMIT"))
        .map(|index| index + 1)
        .ok_or_else(|| "fixture has `amount >= LIMIT`".to_string())?;
    let stub = ripr_stub_write(&root, &format!("src/lib.rs:{line}"));
    let output = run_bounded(stub, &root, "stub", Duration::from_mins(2))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "stub: {stdout}{stderr}");
    let document: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|error| format!("stub JSON: {error}: {stdout}"))?;
    assert_eq!(
        document["placement"]["kind"], "new_integration_file",
        "precondition: established tests/ layout must route to an integration file: {stdout}"
    );
    assert_eq!(document["written"], true, "{stdout}");
    let derived = document["derived_inputs"]
        .as_array()
        .ok_or_else(|| format!("derived_inputs: {stdout}"))?;
    assert!(
        derived
            .iter()
            .any(|value| value.as_str() == Some("amount = LIMIT")),
        "public const boundary must be a derived input: {stdout}"
    );
    let test_name = document["test_name"]
        .as_str()
        .ok_or_else(|| "test_name".to_string())?;
    let file = document["placement"]["file"]
        .as_str()
        .ok_or_else(|| "placement.file".to_string())?;
    assert_eq!(file, "tests/price.rs", "{stdout}");
    let written = std::fs::read_to_string(root.join(file)).map_err(|error| error.to_string())?;
    assert!(written.contains("use stub_integration::*;"), "{written}");
    assert!(
        written.contains("let amount: u32 = LIMIT;"),
        "public const boundary must remain a derived input: {written}"
    );
    assert!(
        written.contains("let tag: stub_integration::Tag = todo!("),
        "crate:: parameter types must rebase to the crate name: {written}"
    );
    assert!(
        !written.contains("crate::"),
        "leftover crate:: would name the test crate, not the library: {written}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?,
        SOURCE,
        "an integration write must not touch the owner file"
    );

    let run = document["run_command"]
        .as_str()
        .ok_or_else(|| format!("run_command: {stdout}"))?;
    assert!(
        run.contains("--test price") && run.contains(test_name),
        "{run}"
    );
    let cargo = cargo_from_run_command(run, &root)?;
    let cargo_run = run_bounded(cargo, &root, "cargo-test", Duration::from_mins(2))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&cargo_run.stdout),
        String::from_utf8_lossy(&cargo_run.stderr)
    );
    if cargo_run.status.success() {
        return Err(format!(
            "the stub must fail at its labelled todo, but cargo test passed:\n{text}\n{written}"
        ));
    }
    if (text.contains("could not compile") || text.contains("error: "))
        && !text.contains("not yet implemented: ripr:")
    {
        return Err(format!(
            "the written stub must compile under cargo test:\n{text}\n{written}"
        ));
    }
    if !text.contains("not yet implemented: ripr:") {
        return Err(format!(
            "the stub must stop at its own todo: {text}\n{written}"
        ));
    }
    scratch.cleanup()?;
    Ok(())
}

fn cargo_from_run_command(run: &str, root: &Path) -> Result<Command, String> {
    let mut parts = run.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| format!("empty run_command: {run}"))?;
    if program != "cargo" {
        return Err(format!("run_command must start with cargo, got {run}"));
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .args(parts)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_TERM_COLOR", "never");
    Ok(command)
}

fn ripr_stub_write(root: &Path, at: &str) -> Command {
    let mut stub = Command::new(env!("CARGO_BIN_EXE_ripr"));
    stub.args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at, "--write", "--json"]);
    stub
}

struct Scratch {
    directory: PathBuf,
    cleanup_attempted: bool,
}

fn io_error_at(action: &str, path: &Path, error: std::io::Error) -> String {
    format!("{action} {}: {error}", path.display())
}

/// `.cargo/config.toml` force-redirects `TMPDIR` to `<workspace>/target`.
/// That directory is gitignored and absent in a fresh worktree whose build
/// output lives in another `CARGO_TARGET_DIR`.
fn ensure_scratch_parent(parent: &Path) -> Result<(), String> {
    std::fs::create_dir_all(parent)
        .map_err(|error| io_error_at("create scratch parent", parent, error))
}

fn create_exclusive_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir(path).map_err(|error| io_error_at("create exclusive scratch", path, error))
}

impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self::create(
            std::env::temp_dir(),
            &format!(
                "ripr-agent-stub-compiles-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
        )
    }

    fn create(parent: impl AsRef<Path>, name: &str) -> Result<Self, String> {
        let parent = parent.as_ref();
        ensure_scratch_parent(parent)?;
        let directory = parent.join(name);
        // Own the directory exclusively; never adopt a preexisting one.
        create_exclusive_dir(&directory)?;
        let scratch = Self {
            directory,
            cleanup_attempted: false,
        };
        create_exclusive_dir(&scratch.directory.join("src"))?;
        Ok(scratch)
    }

    fn cleanup(mut self) -> Result<(), String> {
        self.cleanup_attempted = true;
        std::fs::remove_dir_all(&self.directory)
            .map_err(|error| format!("cleanup failed at {}: {error}", self.directory.display()))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.cleanup_attempted {
            return;
        }
        if std::thread::panicking() {
            eprintln!(
                "retained failed stub fixture at {}",
                self.directory.display()
            );
            return;
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn unused_scratch_probe(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "ripr-agent-stub-temp-root-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

struct RemovePath(PathBuf);

impl Drop for RemovePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn scratch_creates_a_missing_temp_parent() -> Result<(), String> {
    let root = unused_scratch_probe("missing");
    let _cleanup = RemovePath(root.clone());
    let parent = root.join("nested");
    assert!(
        !parent.exists(),
        "discriminator requires an absent parent: {}",
        parent.display()
    );
    let scratch = Scratch::create(&parent, "scratch")?;
    assert!(
        parent.is_dir(),
        "create_dir_all must materialize {}: missing parent is the #6712 repro",
        parent.display()
    );
    assert_eq!(scratch.directory, parent.join("scratch"));
    assert!(scratch.directory.join("src").is_dir());
    scratch.cleanup()
}

#[test]
fn scratch_reuses_an_existing_temp_parent() -> Result<(), String> {
    let parent = unused_scratch_probe("existing");
    let _cleanup = RemovePath(parent.clone());
    ensure_scratch_parent(&parent)?;
    let scratch = Scratch::create(&parent, "scratch")?;
    assert_eq!(scratch.directory, parent.join("scratch"));
    assert!(scratch.directory.join("src").is_dir());
    scratch.cleanup()
}

#[test]
fn scratch_refuses_to_adopt_a_preexisting_directory() -> Result<(), String> {
    let parent = unused_scratch_probe("exclusive");
    let _cleanup = RemovePath(parent.clone());
    let first = Scratch::create(&parent, "scratch")?;
    let error = match Scratch::create(&parent, "scratch") {
        Ok(unexpected) => {
            let path = unexpected.directory.display().to_string();
            unexpected.cleanup()?;
            return Err(format!("must not adopt a preexisting scratch at {path}"));
        }
        Err(error) => error,
    };
    assert!(
        error.contains(&first.directory.display().to_string()),
        "exclusive-create error must name the path, got {error}"
    );
    first.cleanup()
}

#[test]
fn scratch_names_the_path_when_the_temp_parent_is_a_file() -> Result<(), String> {
    let parent = unused_scratch_probe("file");
    let _cleanup = RemovePath(parent.clone());
    if let Some(temp_root) = parent.parent() {
        ensure_scratch_parent(temp_root)?;
    }
    std::fs::write(&parent, b"not a directory").map_err(|error| {
        io_error_at("write file standing in for scratch parent", &parent, error)
    })?;
    let error = match Scratch::create(&parent, "scratch") {
        Ok(unexpected) => {
            unexpected.cleanup()?;
            return Err(format!(
                "a file cannot be a scratch parent: {}",
                parent.display()
            ));
        }
        Err(error) => error,
    };
    assert!(
        error.contains(&parent.display().to_string()),
        "parent-is-file error must name the path, got {error}"
    );
    Ok(())
}

#[test]
fn exclusive_scratch_dir_error_names_the_path_when_parent_is_missing() -> Result<(), String> {
    let parent = unused_scratch_probe("enoent");
    let child = parent.join("child");
    assert!(
        !parent.exists(),
        "discriminator requires an absent parent: {}",
        parent.display()
    );
    let error = match create_exclusive_dir(&child) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&parent);
            return Err(format!(
                "create_dir cannot invent a missing parent: {}",
                child.display()
            ));
        }
        Err(error) => error,
    };
    assert!(
        error.contains(&child.display().to_string()),
        "bare ENOENT hid the missing path; got {error}"
    );
    Ok(())
}

const STREAM_LIMIT: u64 = 256 * 1024;

fn read_bounded_stream(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(STREAM_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > STREAM_LIMIT {
        return Err(format!(
            "stream {} exceeds {STREAM_LIMIT} bytes",
            path.display()
        ));
    }
    Ok(bytes)
}

/// File-backed streams avoid pipe backpressure; the shared process owner
/// terminates and reaps on timeout before the scratch directory is removed.
fn run_bounded(
    mut command: Command,
    root: &Path,
    label: &str,
    budget: Duration,
) -> Result<Output, String> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|error| error.to_string())?);
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= budget {
            child.terminate_tree()?;
            return Err(format!(
                "{label} exceeded its {budget:?} budget; owned process terminated and reaped"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: read_bounded_stream(&stdout)?,
        stderr: read_bounded_stream(&stderr)?,
    })
}
