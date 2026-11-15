//! ADR 0028 (#7441): `ripr` emits no terminal styling, and the color
//! environment variables are inert on human routes.
//!
//! `help --json` already has this pin in `cli_smoke.rs`. This file pins the
//! human routes: the same stdout bytes with and without the variables, and no
//! escape byte on either stream.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ESCAPE: u8 = 0x1b;

/// Every variable a styling layer would consult, cleared for the baseline.
const COLOR_VARIABLES: [&str; 6] = [
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "FORCE_COLOR",
    "TERM",
    "COLORTERM",
];

const ENV_CASES: [&[(&str, &str)]; 6] = [
    &[("NO_COLOR", "1")],
    &[("CLICOLOR", "0")],
    &[("CLICOLOR", "1"), ("CLICOLOR_FORCE", "1")],
    &[("FORCE_COLOR", "1")],
    &[
        ("FORCE_COLOR", "3"),
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
    ],
    &[("TERM", "dumb")],
];

fn has_escape(bytes: &[u8]) -> bool {
    bytes.contains(&ESCAPE)
}

fn run(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(root).args(args);
    for name in COLOR_VARIABLES {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command
        .output()
        .map_err(|err| format!("spawn ripr {args:?}: {err}"))
}

/// The bundled sample as a standalone workspace outside the checkout.
struct Sample {
    root: PathBuf,
}

impl Sample {
    fn new(label: &str) -> Result<Self, String> {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
        let root = std::env::temp_dir().join(format!(
            "ripr-terminal-styling-inert-{label}-{}",
            std::process::id()
        ));
        let sample = Self { root };
        let package = sample.root.join("crates/ripr/examples/sample");
        let io = |err: std::io::Error| format!("build sample workspace: {err}");
        std::fs::create_dir_all(package.join("src")).map_err(io)?;
        std::fs::create_dir_all(package.join("tests")).map_err(io)?;
        std::fs::write(
            sample.root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/ripr/examples/sample\"]\nresolver = \"2\"\n",
        )
        .map_err(io)?;
        std::fs::write(
            package.join("Cargo.toml"),
            "[package]\nname = \"ripr-sample\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(io)?;
        for (from, to) in [
            ("src/lib.rs", package.join("src/lib.rs")),
            ("tests/pricing.rs", package.join("tests/pricing.rs")),
            ("example.diff", sample.root.join("sample.diff")),
        ] {
            std::fs::copy(source.join(from), to).map_err(io)?;
        }
        Ok(sample)
    }
}

impl Drop for Sample {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One human route: the subject must be real output (success, expected
/// marker), unstyled, and unmoved by every color variable.
fn assert_route_is_unstyled_and_inert(
    root: &Path,
    args: &[&str],
    marker: &str,
) -> Result<(), String> {
    let baseline = run(root, args, &[])?;
    let stdout = String::from_utf8_lossy(&baseline.stdout).into_owned();
    if !baseline.status.success() {
        return Err(format!(
            "ripr {args:?} failed: status={:?}, stderr={}",
            baseline.status.code(),
            String::from_utf8_lossy(&baseline.stderr)
        ));
    }
    if !stdout.contains(marker) {
        return Err(format!(
            "ripr {args:?} stdout lacks {marker:?}, so the route under test did not render:\n{stdout}"
        ));
    }
    if has_escape(&baseline.stdout) || has_escape(&baseline.stderr) {
        return Err(format!(
            "ripr {args:?} wrote an escape byte with no color variable set"
        ));
    }
    for env in ENV_CASES {
        let output = run(root, args, env)?;
        if output.status.code() != baseline.status.code() {
            return Err(format!("ripr {args:?} exit status moved under env {env:?}"));
        }
        if output.stdout != baseline.stdout {
            return Err(format!(
                "ripr {args:?} stdout bytes moved under env {env:?}:\n{}",
                String::from_utf8_lossy(&output.stdout)
            ));
        }
        if has_escape(&output.stdout) || has_escape(&output.stderr) {
            return Err(format!(
                "ripr {args:?} wrote an escape byte under env {env:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn escape_oracle_rejects_styled_text_and_admits_plain_text() {
    assert!(has_escape(b"\x1b[1mStart here\x1b[0m"));
    assert!(has_escape(b"plain\x1b[31m"));
    assert!(!has_escape("plain text with a ✓ mark\n".as_bytes()));
    assert!(!has_escape(b""));
}

#[test]
fn help_is_unstyled_and_ignores_color_variables() -> Result<(), String> {
    let sample = Sample::new("help")?;
    assert_route_is_unstyled_and_inert(&sample.root, &["--help"], "ripr")?;
    assert_route_is_unstyled_and_inert(&sample.root, &["help", "--all"], "ripr")
}

/// `--legend` keeps the report independent of the first-run marker (#10481),
/// so the only input that varies between runs is the environment.
#[cfg(feature = "lang-rust")]
#[test]
fn check_human_report_is_unstyled_and_ignores_color_variables() -> Result<(), String> {
    let sample = Sample::new("check")?;
    assert_route_is_unstyled_and_inert(
        &sample.root,
        &["check", "--diff", "sample.diff", "--legend"],
        "Start here:",
    )
}
