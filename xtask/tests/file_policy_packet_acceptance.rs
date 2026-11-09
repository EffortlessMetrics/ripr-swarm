//! Real CLI acceptance over a dependency-free, fixture-sized Cargo inventory.
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

const COMMON_COMMAND: &str = "cargo test -p xtask --bin xtask --locked --offline portable_consumer";
const NATIVE_COMMAND: &str =
    "cargo test -p ripr --locked --offline --test portable_consumer_packet";
const COMMON_SOURCE: &str = "xtask/src/main.rs";
const NATIVE_SOURCE: &str = "ripr/tests/portable_consumer_packet.rs";
const COMMON_FIRST: &str =
    "portable_consumer::tests::consumer_source_does_not_search_path_or_open_a_network_client";
const COMMON_SECOND: &str = "portable_consumer::tests::packet_digest_matches_the_producer_formula";
const NATIVE_FIRST: &str =
    "native_packet_analyzes_a_boundary_gap_without_path_or_compiler_fallback";
const NATIVE_SECOND: &str = "native_packet_pilot_consumes_the_summary_artifact";

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("retain fixture {}: {error}", self.0.display());
        }
    }
}

fn diagnostic(output: &Output) -> String {
    format!(
        "status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

impl Fixture {
    fn new() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!("rpc{}-{nonce:x}", std::process::id()));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let mut fixture = Self(root);
        fixture.0 = fs::canonicalize(&fixture.0).map_err(|error| error.to_string())?;
        fixture.write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"xtask\", \"ripr\"]\nresolver = \"3\"\n",
        )?;
        for package in ["xtask", "ripr"] {
            fixture.write(
                &format!("{package}/Cargo.toml"),
                &format!(
                    "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"
                ),
            )?;
        }
        fixture.write("ripr/src/lib.rs", "// Dependency-free fixture library.\n")?;
        fixture.write_common(false)?;
        fixture.write_native(false)?;
        let mut policy = String::new();
        for (index, path) in [
            "Cargo.toml",
            "xtask/Cargo.toml",
            "ripr/Cargo.toml",
            "policy/non-rust-allowlist.toml",
        ]
        .iter()
        .enumerate()
        {
            policy.push_str(&format!(
                concat!(
                    "[[allow]]\nglob = \"{path}\"\nkind = \"fixture\"\n",
                    "owner = \"packet-cli-test\"\nsurface = \"fixtures\"\n",
                    "classification = \"fixture\"\nreason = \"Tiny Cargo fixture.\"\n",
                ),
                path = path,
            ));
            if index == 0 {
                policy.push_str(&format!(
                    "covered_by = [\"{COMMON_COMMAND}\"]\ncovered_by_unix = [\"{NATIVE_COMMAND}\"]\n"
                ));
            } else {
                policy.push_str("covered_by = []\n");
            }
        }
        fixture.write("policy/non-rust-allowlist.toml", &policy)?;
        let output = fixture.cargo(&["generate-lockfile", "--offline"])?;
        if !output.status.success() || !fixture.0.join("Cargo.lock").is_file() {
            return Err(format!("fixture lockfile setup: {}", diagnostic(&output)));
        }
        Ok(fixture)
    }

    fn write(&self, path: &str, content: &str) -> Result<(), String> {
        let file = self.0.join(path);
        fs::create_dir_all(file.parent().ok_or("fixture file has no parent")?)
            .map_err(|error| error.to_string())?;
        fs::write(file, content).map_err(|error| error.to_string())
    }

    fn write_common(&self, neighbor: bool) -> Result<(), String> {
        let suffix = if neighbor { "_neighbor" } else { "" };
        self.write(
            COMMON_SOURCE,
            &format!(
                concat!(
                    "fn main() {{}}\n#[cfg(test)]\nmod portable_consumer {{\n",
                    "mod tests {{\n#[test]\n",
                    "fn consumer_source_does_not_search_path_or_open_a_network_client() {{}}\n",
                    "#[test]\nfn packet_digest_matches_the_producer_formula{suffix}() {{}}\n",
                    "}}\n}}\n",
                ),
                suffix = suffix,
            ),
        )
    }

    fn write_native(&self, neighbor: bool) -> Result<(), String> {
        let suffix = if neighbor { "_neighbor" } else { "" };
        self.write(
            NATIVE_SOURCE,
            &format!(
                "#[test]\nfn {NATIVE_FIRST}() {{}}\n#[test]\nfn {NATIVE_SECOND}{suffix}() {{}}\n"
            ),
        )
    }

    fn configure(&self, command: &mut Command) {
        command
            .current_dir(&self.0)
            .env("CARGO_TARGET_DIR", self.0.join("target"))
            .env("CARGO_NET_OFFLINE", "true")
            .env_remove("CARGO_BUILD_TARGET")
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER");
    }

    fn cargo(&self, args: &[&str]) -> Result<Output, String> {
        let mut command = Command::new("cargo");
        self.configure(&mut command);
        command
            .args(args)
            .output()
            .map_err(|error| error.to_string())
    }

    fn report_path(&self) -> PathBuf {
        self.0.join("target/ripr/reports/file-policy.md")
    }

    fn check(&self) -> Result<(Output, String), String> {
        match fs::remove_file(self.report_path()) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let output = xtask_bin::output(|command| {
            self.configure(command);
            command.arg("check-file-policy");
        })?;
        let report = fs::read_to_string(self.report_path())
            .map_err(|error| format!("fresh report: {error}; {}", diagnostic(&output)))?;
        Ok((output, report))
    }

    fn require_pass(&self) -> Result<(), String> {
        let (output, report) = self.check()?;
        if !output.status.success() || !report.contains("Status: pass") {
            return Err(format!(
                "valid CLI fixture: {}\n{report}",
                diagnostic(&output)
            ));
        }
        let common = report
            .lines()
            .find(|row| row.ends_with(&format!("`{COMMON_COMMAND}`")))
            .ok_or("common inventory disclosure missing")?;
        let expected = format!(
            "host={}; declared=all; applicable; selected=2; tests=[{COMMON_FIRST}, {COMMON_SECOND}];",
            std::env::consts::FAMILY
        );
        if !common.contains(&expected) {
            return Err(format!("wrong common disclosure: {common}"));
        }
        let native = report
            .lines()
            .find(|row| row.ends_with(&format!("`{NATIVE_COMMAND}`")))
            .ok_or("native inventory disclosure missing")?;
        let expected = if cfg!(windows) {
            String::from("host=windows; declared=unix; not_applicable; selected=not_enumerated;")
        } else {
            format!(
                "host=unix; declared=unix; applicable; selected=2; tests=[{NATIVE_FIRST}, {NATIVE_SECOND}];"
            )
        };
        if !native.contains(&expected) {
            return Err(format!("wrong native disclosure: {native}"));
        }
        Ok(())
    }

    fn require_neighbor_rejected(&self, native: bool) -> Result<(), String> {
        let (command, first, second) = if native {
            (NATIVE_COMMAND, NATIVE_FIRST, NATIVE_SECOND)
        } else {
            (COMMON_COMMAND, COMMON_FIRST, COMMON_SECOND)
        };
        let mut args = command.split_whitespace().skip(1).collect::<Vec<_>>();
        args.extend(["--", "--list", "--format", "terse"]);
        let output = self.cargo(&args)?;
        if !output.status.success() {
            return Err(format!("neighbor inventory setup: {}", diagnostic(&output)));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let selected = stdout
            .lines()
            .filter_map(|line| line.strip_suffix(": test"))
            .collect::<Vec<_>>();
        let neighbor = format!("{second}_neighbor");
        if selected.len() != 2
            || !selected.contains(&first)
            || !selected.contains(&neighbor.as_str())
            || selected.contains(&second)
        {
            return Err(format!(
                "neighbor did not retain two actual tests: {selected:?}"
            ));
        }
        let (output, report) = self.check()?;
        let expected = format!("packet coverage lost exact selected subjects: {command}:");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() != Some(1)
            || !stderr.contains(&expected)
            || !report.contains(&expected)
            || !report.contains(&neighbor)
            || !report.contains("Status: stopped during coverage enumeration")
            || report.contains("Status: pass")
        {
            return Err(format!(
                "CLI missed nonzero neighbor: {}\n{report}",
                diagnostic(&output)
            ));
        }
        Ok(())
    }
}

#[test]
fn file_policy_packet_inventory_reaches_actual_cli_acceptance() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.require_pass()?;
    fixture.write_common(true)?;
    fixture.require_neighbor_rejected(false)?;
    fixture.write_common(false)?;
    fixture.require_pass()?;
    if cfg!(unix) {
        fixture.write_native(true)?;
        fixture.require_neighbor_rejected(true)?;
        fixture.write_native(false)?;
        fixture.require_pass()?;
    }
    Ok(())
}
