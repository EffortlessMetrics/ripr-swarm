//! Native strip compatibility and exact ELF-section controls (#4978).

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use super::Result;
use super::process::{CoverageChild, LINE_BYTES, write_record};

const NATIVE_OUTPUT_BYTES: u64 = 256 * 1024;
const STRIP_PROBE: &str = "probe_runs_the_hot_branch";
const STRIP_ACK: &str = "ripr-strip-probe: asserted hot branch";
const STRIP_SOURCE: &str = r#"#[inline(never)]
pub fn score(flag: bool) -> u32 {
    if flag {
        17
    } else {
        23
    }
}

#[test]
fn probe_runs_the_hot_branch() {
    assert_eq!(score(std::hint::black_box(true)), 17);
    println!("ripr-strip-probe: asserted hot branch");
}
"#;

struct StripFixture {
    root: PathBuf,
    removed: bool,
}

impl StripFixture {
    fn new() -> Result<Self> {
        Ok(Self {
            root: crate::test_support::scratch("coverage-strip")?,
            removed: false,
        })
    }

    fn remove(&mut self) -> Result<()> {
        fs::remove_dir_all(&self.root).map_err(|error| error.to_string())?;
        self.removed = true;
        Ok(())
    }
}

impl Drop for StripFixture {
    fn drop(&mut self) {
        // Any error or panic may leave uncertain child custody. Retain its
        // files; only explicit success after confirmed cleanup removes them.
        if !self.removed {
            let _ = writeln!(
                io::stderr().lock(),
                "retain native strip fixture: {}",
                self.root.display()
            );
        }
    }
}

fn native_arguments(arguments: &[&str]) -> Vec<OsString> {
    arguments
        .iter()
        .map(|value| OsString::from(*value))
        .collect()
}

fn read_native_output(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(NATIVE_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > NATIVE_OUTPUT_BYTES {
        return Err("native strip control exceeded its output budget".into());
    }
    Ok(bytes)
}

fn native_output(
    root: &Path,
    label: &str,
    program: &OsStr,
    arguments: &[OsString],
    environment: &[(&str, OsString)],
) -> Result<Output> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    let capture = |path: &Path| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())
    };
    let mut command = Command::new(program);
    command
        .current_dir(root)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(capture(&stdout)?)
        .stderr(capture(&stderr)?);
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = CoverageChild::spawn(command)?;
    let started = Instant::now();
    loop {
        let exited = child.exited().map_err(|error| error.to_string())?;
        let oversized = [&stdout, &stderr].iter().try_fold(false, |large, path| {
            fs::metadata(path)
                .map(|metadata| large || metadata.len() > NATIVE_OUTPUT_BYTES)
                .map_err(|error| error.to_string())
        })?;
        if oversized || started.elapsed() >= Duration::from_secs(30) {
            let (status, cleanup) = child.cleanup();
            return Err(format!(
                "native strip control {label} exceeded output/time bound: {status:?}; {cleanup:?}"
            ));
        }
        if exited {
            let (status, cleanup) = child.cleanup();
            if let Some(error) = cleanup {
                return Err(format!("native strip control {label}: {error}"));
            }
            let output = Output {
                status: status.ok_or("native strip control status unavailable")?,
                stdout: read_native_output(&stdout)?,
                stderr: read_native_output(&stderr)?,
            };
            if !output.status.success() {
                let prefix = |bytes: &[u8]| {
                    String::from_utf8_lossy(&bytes[..bytes.len().min(8192)]).into_owned()
                };
                return Err(format!(
                    "native strip control {label}: {}; stdout: {}; stderr: {}",
                    output.status,
                    prefix(&output.stdout),
                    prefix(&output.stderr)
                ));
            }
            return Ok(output);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn native_text(output: &Output) -> Result<&str> {
    std::str::from_utf8(&output.stdout).map_err(|error| error.to_string())
}

fn strip_observation(value: Value) -> Result<()> {
    let record = format!("\nripr-coverage-strip {value}\n");
    if record.len() > LINE_BYTES {
        return Err("native strip observation exceeded its record bound".into());
    }
    write_record(&mut io::stdout().lock(), &record).map_err(|error| error.to_string())
}

fn strip_text_observation(phase: &str, mode: &str, source: &str) -> Result<()> {
    let mut remaining = source.chars();
    for part in 0..8 {
        let text: String = remaining.by_ref().take(512).collect();
        if text.is_empty() {
            break;
        }
        let more = remaining.clone().next().is_some();
        strip_observation(json!({
            "phase": phase,
            "mode": mode,
            "part": part,
            "source_bytes": source.len(),
            "text": text,
            "more": more,
            "truncated": part == 7 && more,
        }))?;
    }
    Ok(())
}

struct StripTools {
    rustc: PathBuf,
    cargo: PathBuf,
    profdata: PathBuf,
    cov: PathBuf,
    readobj: PathBuf,
    versions: BTreeMap<&'static str, String>,
}

fn strip_tools(root: &Path) -> Result<StripTools> {
    // As in the existing required LLVM control, Linux tool discovery failure
    // is a proof failure. There is no success/ignore fallback for missing tools.
    let sysroot = native_output(
        root,
        "sysroot",
        OsStr::new("rustc"),
        &native_arguments(&["--print", "sysroot"]),
        &[],
    )?;
    let sysroot = PathBuf::from(native_text(&sysroot)?.trim());
    if !sysroot.is_absolute() {
        return Err("native strip control requires an absolute compiler sysroot".into());
    }
    let rustc = sysroot.join("bin/rustc");
    let cargo = sysroot.join("bin/cargo");
    let identity = native_output(
        root,
        "rustc-version",
        rustc.as_os_str(),
        &native_arguments(&["-vV"]),
        &[],
    )?;
    let identity = native_text(&identity)?.trim().to_owned();
    let host = identity
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("native strip control compiler host missing")?;
    let llvm = sysroot.join("lib/rustlib").join(host).join("bin");
    let mut tools = StripTools {
        rustc,
        cargo,
        profdata: llvm.join("llvm-profdata"),
        cov: llvm.join("llvm-cov"),
        readobj: llvm.join("llvm-readobj"),
        versions: BTreeMap::from([("rustc", identity)]),
    };
    for (name, path) in [
        ("cargo", &tools.cargo),
        ("llvm-profdata", &tools.profdata),
        ("llvm-cov", &tools.cov),
        ("llvm-readobj", &tools.readobj),
    ] {
        let output = native_output(
            root,
            &format!("{name}-version"),
            path.as_os_str(),
            &native_arguments(&["--version"]),
            &[],
        )?;
        tools
            .versions
            .insert(name, native_text(&output)?.trim().into());
    }
    Ok(tools)
}

fn strip_executable(root: &Path, tools: &StripTools, mode: &str) -> Result<PathBuf> {
    let target = root.join(format!("target-{mode}"));
    let environment = [
        ("CARGO_TARGET_DIR", target.as_os_str().to_owned()),
        ("CARGO_BUILD_JOBS", OsString::from("1")),
        ("CARGO_INCREMENTAL", OsString::from("0")),
        ("CARGO_TERM_COLOR", OsString::from("never")),
        ("CARGO_PROFILE_TEST_STRIP", OsString::from(mode)),
        ("RUSTC", tools.rustc.as_os_str().to_owned()),
        ("RUSTC_WRAPPER", OsString::new()),
        ("RUSTC_WORKSPACE_WRAPPER", OsString::new()),
        ("RUSTFLAGS", OsString::new()),
        (
            "CARGO_ENCODED_RUSTFLAGS",
            OsString::from("-C\u{1f}instrument-coverage\u{1f}-C\u{1f}link-arg=-Wl,--build-id=sha1"),
        ),
    ];
    let output = native_output(
        root,
        &format!("{mode}-build"),
        tools.cargo.as_os_str(),
        &native_arguments(&[
            "test",
            "--offline",
            "--lib",
            "--no-run",
            "-vv",
            "--message-format=json",
        ]),
        &environment,
    )?;
    let compiler = std::str::from_utf8(&output.stderr).map_err(|error| error.to_string())?;
    let invocations: Vec<_> = compiler
        .lines()
        .filter(|line| {
            line.contains("Running `")
                && line.contains("--crate-name ripr_coverage_strip_fixture ")
                && line.contains("--test")
        })
        .collect();
    assert_eq!(invocations.len(), 1, "one actual Cargo test compilation");
    let invocation = invocations[0];
    strip_text_observation("compiler", mode, invocation)?;
    assert!(invocation.contains("-C instrument-coverage"));
    assert!(invocation.contains("-C debuginfo=line-tables-only"));
    assert!(!invocation.contains("-C strip=symbols"));
    assert_eq!(
        invocation.contains("-C strip=debuginfo"),
        mode == "debuginfo"
    );

    let mut executables = Vec::new();
    for line in native_text(&output)?.lines() {
        let artifact: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if artifact["reason"] == "compiler-artifact"
            && artifact["target"]["name"] == "ripr_coverage_strip_fixture"
            && artifact["profile"]["test"] == true
            && let Some(path) = artifact["executable"].as_str()
        {
            executables.push(PathBuf::from(path));
        }
    }
    assert_eq!(executables.len(), 1, "one intended Cargo test executable");
    let executable = fs::canonicalize(&executables[0]).map_err(|error| error.to_string())?;
    let target = fs::canonicalize(target).map_err(|error| error.to_string())?;
    assert!(
        executable.starts_with(target),
        "artifact belongs to this variant"
    );
    Ok(executable)
}

fn one_build_id<'a>(ids: impl Iterator<Item = &'a str>) -> Result<String> {
    let ids: Vec<_> = ids.filter(|id| !id.is_empty()).collect();
    if ids.len() != 1 || ids[0].len() != 40 || !ids[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "native strip control requires one SHA-1 build ID: {ids:?}"
        ));
    }
    Ok(ids[0].to_ascii_lowercase())
}

fn owned_lcov_lines(text: &str, source: &Path) -> Result<BTreeMap<u64, u64>> {
    let mut records = text.split("end_of_record").filter(|record| {
        record.lines().any(|line| {
            line.strip_prefix("SF:")
                .is_some_and(|path| Path::new(path) == source)
        })
    });
    let record = records.next().ok_or("owned source missing from LCOV")?;
    if records.next().is_some() {
        return Err("duplicate owned source in LCOV".into());
    }
    let mut lines = BTreeMap::new();
    for line in record.lines().filter_map(|line| line.strip_prefix("DA:")) {
        let mut fields = line.split(',');
        let number = fields.next().ok_or("LCOV line missing")?;
        let count = fields.next().ok_or("LCOV count missing")?;
        let number = number.parse::<u64>().map_err(|error| error.to_string())?;
        let count = count.parse::<u64>().map_err(|error| error.to_string())?;
        if lines.insert(number, count).is_some() {
            return Err("duplicate LCOV source line".into());
        }
    }
    if lines.is_empty() {
        return Err("owned LCOV source has no line counters".into());
    }
    Ok(lines)
}

fn owned_coverage(export: &Value, source: &Path) -> Result<Value> {
    let data = export["data"].as_array().ok_or("coverage data missing")?;
    if data.len() != 1 {
        return Err("expected one binary coverage export".into());
    }
    let files = data[0]["files"]
        .as_array()
        .ok_or("coverage files missing")?;
    let files: Vec<_> = files
        .iter()
        .filter(|file| {
            file["filename"]
                .as_str()
                .is_some_and(|path| Path::new(path) == source)
        })
        .collect();
    if files.len() != 1 {
        return Err("expected exactly one owned source coverage record".into());
    }
    let functions = data[0]["functions"]
        .as_array()
        .ok_or("coverage functions missing")?;
    let mut owned = BTreeMap::new();
    for name in ["score", STRIP_PROBE] {
        let functions: Vec<_> = functions
            .iter()
            .filter(|function| {
                function["name"]
                    .as_str()
                    .is_some_and(|symbol| symbol.contains(name))
                    && function["filenames"].as_array().is_some_and(|paths| {
                        paths
                            .iter()
                            .any(|path| path.as_str().is_some_and(|path| Path::new(path) == source))
                    })
            })
            .collect();
        if functions.len() != 1 {
            return Err(format!("expected exactly one owned {name} function"));
        }
        let mut function = (*functions[0]).clone();
        assert_eq!(function["count"], 2, "{name}: two real invocations");
        assert!(
            function["regions"]
                .as_array()
                .is_some_and(|regions| !regions.is_empty())
        );
        // Cargo may change the mangled crate hash between profile settings.
        // Retain every source filename, region and counter in the comparison.
        function
            .as_object_mut()
            .ok_or("function record is not an object")?
            .remove("name");
        owned.insert(name, function);
    }
    Ok(json!({"file": files[0], "functions": owned}))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct DebugSection {
    name: String,
    section_type: String,
    size_bytes: u64,
    flags: String,
}

fn debug_section_name(name: &str) -> bool {
    name.starts_with(".debug") || name.starts_with(".zdebug")
}

fn parse_debug_sections(text: &str) -> Result<Vec<DebugSection>> {
    let headers = text
        .lines()
        .filter(|line| line.trim() == "Section Headers:")
        .count();
    if headers != 1 {
        return Err("expected one GNU ELF section table".into());
    }
    let lines = text
        .lines()
        .skip_while(|line| line.trim() != "Section Headers:")
        .skip(1);
    let mut rows = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut sections = Vec::new();
    let mut ended = false;
    for line in lines.map(str::trim).filter(|line| !line.is_empty()) {
        if line == "Key to Flags:" {
            ended = true;
            break;
        }
        let (index, row) = line
            .strip_prefix('[')
            .and_then(|line| line.split_once(']'))
            .ok_or("malformed GNU ELF section row")?;
        if index.trim() == "Nr" {
            continue;
        }
        let index = index
            .trim()
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        if !rows.insert(index) || rows.len() > 256 {
            return Err("duplicate or excessive GNU ELF section rows".into());
        }
        let fields: Vec<_> = row.split_whitespace().collect();
        let name = fields.first().ok_or("empty GNU ELF section row")?;
        if !debug_section_name(name) {
            if fields.iter().any(|field| debug_section_name(field)) {
                return Err("misplaced GNU ELF debug section name".into());
            }
            continue;
        }
        if !matches!(fields.len(), 9 | 10) {
            return Err("malformed GNU ELF debug section fields".into());
        }
        let flags = if fields.len() == 10 { fields[6] } else { "" };
        if name.len() > 128 || fields[1].len() > 32 || flags.len() > 16 {
            return Err("GNU ELF debug section text exceeds bounds".into());
        }
        if !names.insert((*name).to_owned()) || sections.len() == 32 {
            return Err("duplicate or excessive GNU ELF debug sections".into());
        }
        for field in &fields[2..6] {
            u64::from_str_radix(field, 16).map_err(|error| error.to_string())?;
        }
        for field in &fields[fields.len() - 3..] {
            field.parse::<u64>().map_err(|error| error.to_string())?;
        }
        sections.push(DebugSection {
            name: (*name).to_owned(),
            section_type: fields[1].to_owned(),
            size_bytes: u64::from_str_radix(fields[4], 16).map_err(|error| error.to_string())?,
            flags: flags.to_owned(),
        });
    }
    if !ended || rows.is_empty() {
        return Err("incomplete or empty GNU ELF section table".into());
    }
    Ok(sections)
}

fn nonempty_dwarf_line_table(section: &DebugSection) -> bool {
    section.name == ".debug_line"
        && section.section_type == "PROGBITS"
        && section.size_bytes > 0
        && !section.flags.contains('A')
}

fn permitted_debug_loader(section: &DebugSection) -> bool {
    // Rust emits pretty-printer loader records here and keeps a volatile
    // reference. This allocated, read-only data is not a DWARF line table.
    // Do not generalize lld's ALLOC exception to arbitrary debug sections.
    section.name == ".debug_gdb_scripts"
        && section.section_type == "PROGBITS"
        && section.size_bytes > 0
        && section.flags.contains('A')
        && !section.flags.contains('W')
        && !section.flags.contains('X')
}

#[test]
fn debug_sections_require_dwarf_removal_and_exact_allocated_loader_marker() -> Result<()> {
    let header = "  [Nr] Name Type Address Off Size ES Flg Lk Inf Al\n";
    let null = "  [ 0] NULL 0000000000000000 000000 000000 00 0 0 0\n";
    let line = "  [ 2] .debug_line PROGBITS 0000000000000000 000040 000020 00 0 0 1\n";
    let loader = "  [ 3] .debug_gdb_scripts PROGBITS 0000000000000040 000060 000022 01 AMS 0 0 1\n";
    let table = format!("Section Headers:\n{header}{null}{line}{loader}Key to Flags:\n");
    let sections = parse_debug_sections(&table)?;
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].size_bytes, 32, "GNU sizes are hexadecimal");
    assert!(nonempty_dwarf_line_table(&sections[0]));
    assert!(!permitted_debug_loader(&sections[0]));
    assert!(permitted_debug_loader(&sections[1]));
    assert!(!nonempty_dwarf_line_table(&sections[1]));
    let no_debug = parse_debug_sections(&format!("Section Headers:\n{null}Key to Flags:\n"))?;
    assert!(
        no_debug.is_empty(),
        "the unnamed NULL row is not a debug section"
    );
    let lookalike =
        parse_debug_sections(&table.replace(".debug_gdb_scripts", ".debug_gdb_scripts_extra"))?;
    assert_eq!(lookalike[1].name, ".debug_gdb_scripts_extra");
    assert!(!permitted_debug_loader(&lookalike[1]));

    let mut allocated_dwarf = sections[0].clone();
    allocated_dwarf.flags = "A".into();
    assert!(!permitted_debug_loader(&allocated_dwarf));
    assert!(!nonempty_dwarf_line_table(&allocated_dwarf));
    let mut empty_dwarf = sections[0].clone();
    empty_dwarf.size_bytes = 0;
    assert!(!nonempty_dwarf_line_table(&empty_dwarf));
    for name in [
        ".debug_info",
        ".zdebug_info",
        ".debug_gdb_scripts_extra",
        ".debug_unknown",
    ] {
        let mut wrong = sections[1].clone();
        wrong.name = name.into();
        assert!(!permitted_debug_loader(&wrong), "{name}");
    }
    for flags in ["", "MS", "WA", "AX"] {
        let mut wrong = sections[1].clone();
        wrong.flags = flags.into();
        assert!(!permitted_debug_loader(&wrong), "{flags}");
    }
    for (section_type, size_bytes) in [("NOBITS", 34), ("PROGBITS", 0)] {
        let mut wrong = sections[1].clone();
        wrong.section_type = section_type.into();
        wrong.size_bytes = size_bytes;
        assert!(!permitted_debug_loader(&wrong));
    }
    for broken in [
        table.replace("Section Headers:\n", ""),
        table.replace("Key to Flags:\n", ""),
        table.replace("000020", "invalid"),
        table.replace(" 0 0 1", " 0 0"),
        table.replace("[ 2]", "[invalid]"),
        table.replace(".debug_line", "other .debug_line"),
        table.replace(loader, &format!("{loader}{loader}")),
        table.replace("[ 3] .debug_gdb_scripts", "[ 3] .debug_line"),
        "Section Headers:\nKey to Flags:\n".into(),
    ] {
        assert!(parse_debug_sections(&broken).is_err(), "{broken}");
    }
    Ok(())
}

struct StripObservation {
    bytes: u64,
    build_id: String,
    lines: BTreeMap<u64, u64>,
    coverage: Value,
}

fn observe_strip_variant(root: &Path, tools: &StripTools, mode: &str) -> Result<StripObservation> {
    let executable = strip_executable(root, tools, mode)?;
    let mut profiles = Vec::new();
    for run in 0..2 {
        let profile = root.join(format!("{mode}-{run}.profraw"));
        assert!(
            !profile.exists(),
            "each invocation starts without a profile"
        );
        let output = native_output(
            root,
            &format!("{mode}-run-{run}"),
            executable.as_os_str(),
            &native_arguments(&["--exact", STRIP_PROBE, "--nocapture"]),
            &[("LLVM_PROFILE_FILE", profile.as_os_str().to_owned())],
        )?;
        let text = native_text(&output)?;
        assert_eq!(
            text.matches(STRIP_ACK).count(),
            1,
            "post-assertion acknowledgement"
        );
        let passed = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
        assert_eq!(
            text.matches(passed).count(),
            1,
            "exact nonempty libtest selection"
        );
        assert!(
            output.stderr.is_empty(),
            "profile write diagnostics are not success"
        );
        let bytes = fs::metadata(&profile)
            .map_err(|error| error.to_string())?
            .len();
        assert!(
            (1..=1024 * 1024).contains(&bytes),
            "nonempty bounded raw profile"
        );
        profiles.push(profile.into_os_string());
    }
    let merged = root.join(format!("{mode}.profdata"));
    let mut arguments = native_arguments(&["merge", "-sparse"]);
    arguments.extend(profiles);
    arguments.push("-o".into());
    arguments.push(merged.as_os_str().to_owned());
    let merge = native_output(
        root,
        &format!("{mode}-merge"),
        tools.profdata.as_os_str(),
        &arguments,
        &[],
    )?;
    assert!(
        merge.stderr.is_empty(),
        "strict merge has no discarded-profile warning"
    );

    let image = native_output(
        root,
        &format!("{mode}-elf"),
        tools.readobj.as_os_str(),
        &[
            OsString::from("--sections"),
            OsString::from("--notes"),
            OsString::from("--elf-output-style=GNU"),
            executable.as_os_str().to_owned(),
        ],
        &[],
    )?;
    assert!(
        image.stderr.is_empty(),
        "ELF inspection warnings are not success"
    );
    let image = native_text(&image)?;
    let debug_sections = match parse_debug_sections(image) {
        Ok(sections) => sections,
        Err(error) => {
            strip_text_observation("elf_parse_failure", mode, image)?;
            return Err(error);
        }
    };
    strip_observation(json!({
        "phase": "elf_sections",
        "mode": mode,
        "debug_section_count": debug_sections.len(),
    }))?;
    for section in &debug_sections {
        strip_observation(json!({
            "phase": "elf_section",
            "mode": mode,
            "section": section,
            "permitted_loader": permitted_debug_loader(section),
        }))?;
    }
    if mode == "none" {
        assert!(
            debug_sections.iter().any(nonempty_dwarf_line_table),
            "baseline has a nonempty, nonallocated PROGBITS DWARF line table"
        );
    } else {
        assert!(
            debug_sections.iter().all(permitted_debug_loader),
            "candidate retained an unsupported debug section; see named section receipts"
        );
    }
    let build_id = one_build_id(
        image
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Build ID: ")),
    )?;
    let ids = native_output(
        root,
        &format!("{mode}-ids"),
        tools.profdata.as_os_str(),
        &[
            OsString::from("show"),
            OsString::from("--binary-ids"),
            merged.as_os_str().to_owned(),
        ],
        &[],
    )?;
    assert!(ids.stderr.is_empty(), "profile ID warnings are not success");
    let mut ids = native_text(&ids)?.lines();
    if !ids.any(|line| line.trim() == "Binary IDs:") {
        return Err("merged profile binary IDs missing".into());
    }
    assert_eq!(one_build_id(ids.map(str::trim))?, build_id);

    let source = fs::canonicalize(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
    let arguments = [
        OsString::from("export"),
        executable.as_os_str().to_owned(),
        OsString::from("-instr-profile"),
        merged.as_os_str().to_owned(),
    ];
    let export = native_output(
        root,
        &format!("{mode}-json"),
        tools.cov.as_os_str(),
        &arguments,
        &[],
    )?;
    assert!(
        export.stderr.is_empty(),
        "coverage export warnings are not success"
    );
    let export: Value =
        serde_json::from_slice(&export.stdout).map_err(|error| error.to_string())?;
    let coverage = owned_coverage(&export, &source)?;
    let mut arguments = arguments.to_vec();
    arguments.push("-format=lcov".into());
    let lcov = native_output(
        root,
        &format!("{mode}-lcov"),
        tools.cov.as_os_str(),
        &arguments,
        &[],
    )?;
    assert!(
        lcov.stderr.is_empty(),
        "LCOV export warnings are not success"
    );
    let lines = owned_lcov_lines(native_text(&lcov)?, &source)?;
    assert_eq!(lines.get(&4), Some(&2), "the hot return executed twice");
    assert_eq!(lines.get(&6), Some(&0), "the cold return never executed");
    Ok(StripObservation {
        bytes: fs::metadata(executable)
            .map_err(|error| error.to_string())?
            .len(),
        build_id,
        lines,
        coverage,
    })
}

#[test]
fn native_test_profile_strip_preserves_coverage_and_reduces_fixture_size() -> Result<()> {
    let mut fixture = StripFixture::new()?;
    fs::create_dir(fixture.root.join("src")).map_err(|error| error.to_string())?;
    fs::write(fixture.root.join("src/lib.rs"), STRIP_SOURCE).map_err(|error| error.to_string())?;
    fs::write(
        fixture.root.join("Cargo.toml"),
        concat!(
            "[package]\nname = \"ripr_coverage_strip_fixture\"\n",
            "version = \"0.0.0\"\nedition = \"2024\"\n",
            "[workspace]\n[profile.dev]\ndebug = \"line-tables-only\"\n",
        ),
    )
    .map_err(|error| error.to_string())?;
    let tools = strip_tools(&fixture.root)?;
    let baseline = observe_strip_variant(&fixture.root, &tools, "none")?;
    let stripped = observe_strip_variant(&fixture.root, &tools, "debuginfo")?;
    assert_eq!(
        baseline.lines, stripped.lines,
        "all owned source line counts"
    );
    assert_eq!(
        baseline.coverage, stripped.coverage,
        "source regions and function counts"
    );
    assert!(
        stripped.bytes < baseline.bytes,
        "the fixture must actually shrink"
    );
    // Direct bounded output remains visible in the mandatory libtest preflight.
    // Fixture savings do not predict the full workspace's allocation reduction.
    let record = format!(
        "\nripr-coverage-strip {}\n",
        json!({
            "native_test_profile_strip": "passed",
            "tools": tools.versions,
            "baseline_bytes": baseline.bytes,
            "stripped_bytes": stripped.bytes,
            "baseline_build_id": baseline.build_id,
            "stripped_build_id": stripped.build_id,
            "invocations_per_variant": 2,
            "hot_line_count": 2,
            "cold_line_count": 0,
        })
    );
    assert!(record.len() <= LINE_BYTES);
    fixture.remove()?;
    write_record(&mut io::stdout().lock(), &record).map_err(|error| error.to_string())?;
    Ok(())
}
