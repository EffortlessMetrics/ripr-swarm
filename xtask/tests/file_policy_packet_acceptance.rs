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

#[test]
fn file_policy_static_rejection_precedes_cargo_enumeration() -> Result<(), String> {
    let fixture = Fixture::new()?;
    // A cold fixture with broken Rust cannot successfully enumerate. The
    // unrelated unapproved file must still produce the static policy rejection.
    fixture.write(COMMON_SOURCE, "this is not valid Rust\n")?;
    fixture.write("unapproved.py", "raise RuntimeError('not executed')\n")?;
    let (output, report) = fixture.check()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(1)
        || !report.contains("unapproved non-Rust programming/declarative file: unapproved.py")
        || !report.contains("not_evaluated: static file-policy violations; no coverage established")
        || report.contains("Status: pass")
        || report.contains("applicable; selected=")
        || stderr.contains("policy cargo child start:")
        || report.contains("Status: stopped during coverage enumeration")
    {
        return Err(format!(
            "static rejection did not precede Cargo: {}\n{report}",
            diagnostic(&output)
        ));
    }
    // Remove only the static blocker. The same broken source must now reach
    // Cargo and fail; skipping coverage on the clean path is a false green.
    fs::remove_file(fixture.0.join("unapproved.py")).map_err(|error| error.to_string())?;
    let (output, report) = fixture.check()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(1)
        || !stderr.contains("policy cargo child start:")
        || !report.contains("Status: stopped during coverage enumeration")
        || report.contains("Status: pass")
        || report.contains("not_evaluated: static file-policy violations")
    {
        return Err(format!(
            "clean static scan bypassed coverage: {}\n{report}",
            diagnostic(&output)
        ));
    }
    Ok(())
}

#[test]
fn file_policy_rechecks_files_created_during_enumeration() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.write(
        "xtask/build.rs",
        "fn main() { std::fs::write(\"../generated.py\", \"# generated by Cargo\\n\").expect(\"fixture output\"); }\n",
    )?;
    if fixture.0.join("generated.py").exists() {
        return Err("fixture output existed before enumeration".to_string());
    }
    let (output, report) = fixture.check()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !fixture.0.join("generated.py").is_file()
        || output.status.code() != Some(1)
        || !stderr.contains("policy cargo child start:")
        || !report.contains("unapproved non-Rust programming/declarative file: generated.py")
        || !report.contains("applicable; selected=2;")
        || report.contains("Status: pass")
        || report.contains("not_evaluated: static file-policy violations")
    {
        return Err(format!(
            "post-enumeration static violation was lost: {}\n{report}",
            diagnostic(&output)
        ));
    }
    Ok(())
}

fn libtest_artifact(output: &Output) -> Result<(PathBuf, bool), String> {
    if !output.status.success() {
        return Err(diagnostic(output));
    }
    let rows = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    if rows
        .iter()
        .filter(|row| row["reason"] == "build-finished" && row["success"] == true)
        .count()
        != 1
    {
        return Err("missing unique completed Cargo producer".to_string());
    }
    let artifacts = rows
        .iter()
        .filter(|row| {
            row["reason"] == "compiler-artifact"
                && row["target"]["name"] == "ripr"
                && row["profile"]["test"] == true
        })
        .collect::<Vec<_>>();
    if artifacts.len() != 1 {
        return Err("missing unique ripr libtest artifact".to_string());
    }
    let path = artifacts[0]["executable"]
        .as_str()
        .ok_or("missing native executable")?;
    let all_fresh = rows
        .iter()
        .filter(|row| row["reason"] == "compiler-artifact")
        .all(|row| row["fresh"] == true);
    Ok((PathBuf::from(path), all_fresh))
}

fn ordinary_defaults_cover_contract(ripr: &str, contract: &str) -> Result<bool, String> {
    let ripr: toml::Value = toml::from_str(ripr).map_err(|error| error.to_string())?;
    let contract: toml::Value = toml::from_str(contract).map_err(|error| error.to_string())?;
    let feature_table = ripr
        .get("features")
        .and_then(toml::Value::as_table)
        .ok_or("missing facade features")?;
    let mut pending = vec!["default".to_string()];
    let mut visited = std::collections::BTreeSet::new();
    let mut forwarded = std::collections::BTreeSet::new();
    while let Some(feature) = pending.pop() {
        if !visited.insert(feature.clone()) {
            continue;
        }
        let values = feature_table
            .get(&feature)
            .and_then(toml::Value::as_array)
            .ok_or("unknown local feature")?;
        for value in values {
            let value = value.as_str().ok_or("non-string feature")?;
            if let Some(name) = value.strip_prefix("ripr-contract/") {
                forwarded.insert(name.to_string());
            } else if feature_table.contains_key(value) {
                pending.push(value.to_string());
            }
        }
    }
    let defaults = contract
        .get("features")
        .and_then(|v| v.get("default"))
        .and_then(toml::Value::as_array)
        .ok_or("missing contract defaults")?;
    Ok(!defaults.is_empty()
        && defaults
            .iter()
            .all(|value| value.as_str().is_some_and(|name| forwarded.contains(name))))
}

#[test]
fn file_policy_default_lib_selector_reuses_workspace_preparation() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("missing repository")?
        .to_path_buf();
    let ripr_manifest =
        fs::read_to_string(root.join("crates/ripr/Cargo.toml")).map_err(|e| e.to_string())?;
    let contract_manifest = fs::read_to_string(root.join("crates/ripr-contract/Cargo.toml"))
        .map_err(|e| e.to_string())?;
    if !ordinary_defaults_cover_contract(&ripr_manifest, &contract_manifest)? {
        return Err(
            "contract defaults would broaden ordinary facade language selection".to_string(),
        );
    }
    let changed_defaults = contract_manifest.replace(
        "default = [",
        "unexpected-language = []\ndefault = [\"unexpected-language\", ",
    );
    if ordinary_defaults_cover_contract(&ripr_manifest, &changed_defaults)? {
        return Err("new contract default escaped conservation control".to_string());
    }
    let policy: toml::Value = toml::from_str(
        &fs::read_to_string(root.join("policy/non-rust-allowlist.toml"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let declarations = policy
        .get("allow")
        .and_then(toml::Value::as_array)
        .ok_or("missing allowlist")?;
    let vendor = declarations
        .iter()
        .find(|row| row.get("glob").and_then(toml::Value::as_str) == Some("crates/ripr/vendor/**"))
        .ok_or("missing SCIP declaration")?;
    let command = vendor
        .get("covered_by")
        .and_then(toml::Value::as_array)
        .ok_or("missing coverage")?
        .iter()
        .filter_map(toml::Value::as_str)
        .find(|value| value.starts_with("cargo test "))
        .ok_or("missing native selector")?;
    if command.split_whitespace().any(|arg| {
        matches!(
            arg,
            "--workspace" | "--all" | "--no-default-features" | "--all-features"
        )
    }) {
        return Err("alignment must remain the ordinary per-package default selector".to_string());
    }
    let fixture = Fixture::new()?;
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"xtask\",\"ripr\",\"ripr-contract\"]\nresolver=\"3\"\n",
    )?;
    fixture.write("ripr-contract/Cargo.toml", "[package]\nname=\"ripr-contract\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[features]\ndefault=[\"lang-rust\",\"lang-typescript\",\"lang-python\"]\nlang-rust=[]\nlang-typescript=[]\nlang-python=[]\n")?;
    fixture.write(
        "ripr-contract/src/lib.rs",
        "pub fn language_enabled() -> bool { cfg!(feature=\"lang-rust\") }\n",
    )?;
    fixture.write("ripr/Cargo.toml", "[package]\nname=\"ripr\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nripr-contract={path=\"../ripr-contract\",default-features=false,features=[\"lang-rust\",\"lang-typescript\",\"lang-python\"]}\n")?;
    fixture.write("ripr/src/lib.rs", "#[cfg(test)] mod analysis { mod scip { mod admission { mod tests { #[test] fn native_subject() { assert!(ripr_contract::language_enabled()); } #[test] #[ignore] fn ignored_subject() {} } } } }\n")?;
    let producer = fixture.cargo(&[
        "test",
        "--workspace",
        "--lib",
        "--message-format=json",
        "--",
        "--list",
        "--format",
        "terse",
    ])?;
    let (produced_binary, _) = libtest_artifact(&producer)?;
    let produced_bytes = fs::read(&produced_binary).map_err(|e| e.to_string())?;
    let mut consumer_args = command.split_whitespace().skip(1).collect::<Vec<_>>();
    consumer_args.extend(["--message-format=json", "--", "--list", "--format", "terse"]);
    let consumer = fixture.cargo(&consumer_args)?;
    let (consumed_binary, all_fresh) = libtest_artifact(&consumer)?;
    let names = |output: &Output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.ends_with(": test"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let expected = vec![
        "analysis::scip::admission::tests::ignored_subject: test".to_string(),
        "analysis::scip::admission::tests::native_subject: test".to_string(),
    ];
    if !all_fresh
        || produced_binary != consumed_binary
        || produced_bytes != fs::read(consumed_binary).map_err(|e| e.to_string())?
        || names(&producer) != expected
        || names(&consumer) != expected
    {
        return Err(format!(
            "selector failed exact native reuse; all_fresh={all_fresh}; {}",
            diagnostic(&consumer)
        ));
    }
    // Source changes still go through Cargo's native currentness check. The
    // prior inventory or byte identity cannot suppress a changed input.
    fixture.write("ripr/src/lib.rs", "#[cfg(test)] mod analysis { mod scip { mod admission { mod tests { #[test] fn changed_subject() {} } } } }\n")?;
    let changed = fixture.cargo(&consumer_args)?;
    let (_, all_fresh) = libtest_artifact(&changed)?;
    if all_fresh
        || names(&changed)
            != vec!["analysis::scip::admission::tests::changed_subject: test".to_string()]
    {
        return Err(format!(
            "changed source reused a stale native inventory: {}",
            diagnostic(&changed)
        ));
    }
    Ok(())
}
