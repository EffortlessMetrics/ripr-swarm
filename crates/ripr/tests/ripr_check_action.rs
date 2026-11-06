//! Behavioural controls for the `ripr-check` composite Action (#11659).
//!
//! The Action's `run:` blocks are extracted from the YAML and executed under
//! bash with stand-in `curl`, `ripr` and `git` remotes, so install routing and
//! base derivation are exercised without the network or a real release.
#![cfg(unix)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const ACTION: &str = include_str!("../../../.github/actions/ripr-check/action.yml");

/// The body of a step's `run: |` block (de-indented), found by step name.
fn run_block(step: &str) -> TestResult<String> {
    let lines: Vec<&str> = ACTION.lines().collect();
    let header = format!("- name: {step}");
    let start = lines
        .iter()
        .position(|line| line.trim() == header)
        .ok_or_else(|| format!("missing step {step}"))?;
    let run = lines[start..]
        .iter()
        .position(|line| line.trim() == "run: |")
        .ok_or_else(|| format!("step {step} has no run block"))?
        + start;
    let indent = lines[run + 1].len() - lines[run + 1].trim_start().len();
    let mut body = Vec::new();
    for line in &lines[run + 1..] {
        if line.trim().is_empty() {
            body.push("");
        } else if line.len() - line.trim_start().len() >= indent {
            body.push(&line[indent..]);
        } else {
            break;
        }
    }
    Ok(body.join("\n"))
}

fn scratch(label: &str) -> TestResult<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "ripr-check-action-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn stand_in(dir: &Path, name: &str, script: &str) -> TestResult<()> {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Passes the host tools the blocks need through, but never `cargo`.
fn tool_dir(root: &Path) -> TestResult<PathBuf> {
    let bin = root.join("bin");
    fs::create_dir_all(&bin)?;
    for tool in [
        "mkdir",
        "awk",
        "tar",
        "git",
        "sha256sum",
        "shasum",
        "tr",
        "sed",
        "sort",
        "uniq",
        "grep",
        "unzip",
        "cp",
        "basename",
        "chmod",
    ] {
        let real = ["/usr/bin", "/bin", "/usr/local/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists());
        if let Some(real) = real {
            stand_in(&bin, tool, &format!("exec {} \"$@\"", real.display()))?;
        }
    }
    Ok(bin)
}

struct Run {
    output: Output,
}

impl Run {
    fn ok(&self) -> bool {
        self.output.status.success()
    }
    fn text(&self) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        )
    }
}

fn bash(script: &str, dir: &Path, path: &Path, env: &[(&str, &str)]) -> TestResult<Run> {
    let mut command = Command::new("/bin/bash");
    command
        .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script])
        .current_dir(dir)
        .env_clear()
        .env("PATH", path)
        .env("HOME", dir)
        .env("RUNNER_TEMP", dir.join("runner-temp"))
        .env("GITHUB_PATH", dir.join("github-path"));
    for (key, value) in env {
        command.env(key, value);
    }
    fs::create_dir_all(dir.join("runner-temp"))?;
    Ok(Run {
        output: command.output()?,
    })
}

fn install(version: &str, os: &str, arch: &str, root: &Path, bin: &Path) -> TestResult<Run> {
    bash(
        &run_block("Install ripr")?,
        root,
        bin,
        &[
            ("RIPR_INPUT_VERSION", version),
            ("RUNNER_OS", os),
            ("RUNNER_ARCH", arch),
        ],
    )
}

#[test]
fn malformed_versions_are_refused_before_any_download() -> TestResult {
    let root = scratch("version")?;
    let bin = tool_dir(&root)?;
    stand_in(
        &bin,
        "curl",
        &format!("echo called >> {}", root.join("curl.log").display()),
    )?;
    for bad in ["0.10.0; rm -rf /", "0.10", "latest", "", "1.2.3.4", "$(id)"] {
        let run = install(bad, "Linux", "X64", &root, &bin)?;
        assert!(!run.ok(), "{bad:?} must be refused: {}", run.text());
        assert!(
            run.text().contains("version must look like"),
            "{}",
            run.text()
        );
    }
    assert!(
        !root.join("curl.log").exists(),
        "a bad version reached curl"
    );
    Ok(())
}

#[test]
fn a_matching_ripr_on_path_skips_the_download() -> TestResult {
    let root = scratch("on-path")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo 'ripr 9.9.9 (abc)'")?;
    stand_in(
        &bin,
        "curl",
        &format!("echo called >> {}", root.join("curl.log").display()),
    )?;
    let run = install("9.9.9", "Linux", "X64", &root, &bin)?;
    assert!(run.ok(), "{}", run.text());
    assert!(run.text().contains("already on PATH"), "{}", run.text());
    assert!(!root.join("curl.log").exists());

    // A different installed version is not trusted.
    let run = install("9.9.8", "Linux", "X64", &root, &bin)?;
    assert!(root.join("curl.log").exists(), "{}", run.text());
    Ok(())
}

#[test]
fn a_checksum_mismatch_fails_before_extraction() -> TestResult {
    let root = scratch("checksum")?;
    let bin = tool_dir(&root)?;
    // Writes an archive and a .sha256 that does not match it.
    stand_in(
        &bin,
        "curl",
        "for last; do :; done; f=${last##*/}; case \"$last\" in *.sha256) echo 0000 > \"$f\";; *) echo not-a-tarball > \"$f\";; esac",
    )?;
    let run = install("0.10.0", "Linux", "X64", &root, &bin)?;
    assert!(!run.ok(), "{}", run.text());
    assert!(
        run.text().contains("does not match its published SHA-256"),
        "{}",
        run.text()
    );
    assert!(
        !root.join("github-path").exists(),
        "PATH must not be extended after a mismatch"
    );
    Ok(())
}

#[test]
fn a_runner_without_an_archive_or_cargo_says_what_to_do() -> TestResult {
    let root = scratch("no-cargo")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "curl", "exit 1")?;
    let run = install("0.10.0", "Linux", "ARM32", &root, &bin)?;
    assert!(!run.ok(), "{}", run.text());
    assert!(
        run.text()
            .contains("no prebuilt ripr 0.10.0 for Linux-ARM32"),
        "{}",
        run.text()
    );
    assert!(run.text().contains("no cargo"), "{}", run.text());
    Ok(())
}

#[test]
fn an_unreachable_archive_falls_back_to_a_pinned_cargo_install() -> TestResult {
    let root = scratch("fallback")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "curl", "exit 1")?;
    stand_in(
        &bin,
        "cargo",
        &format!(
            "echo \"$@\" > {}\n\
             printf '#!/bin/sh\\necho \"ripr 0.10.0 (built)\"\\n' > {bin}/ripr\n\
             chmod +x {bin}/ripr",
            root.join("cargo.args").display(),
            bin = bin.display()
        ),
    )?;
    let run = install("0.10.0", "Linux", "X64", &root, &bin)?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        fs::read_to_string(root.join("cargo.args"))?.trim(),
        "install ripr --version 0.10.0 --locked"
    );
    Ok(())
}

/// #6245: the Windows x64 zip route, ARM64 refusal and fail-closed archive
/// handling, replayed like the generated workflow's `Install ripr` step.
/// `curl` serves fixtures from `served/`; `winroot/System32/tar.exe` is an
/// `unzip`-backed stand-in for the bsdtar Windows ships and does not reproduce
/// its own path sanitizing.
mod zip_route {
    use super::*;

    const VERSION: &str = "0.10.0";
    const TARGET: &str = "x86_64-pc-windows-msvc";

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// One stored zip entry with unix mode bits (`0o120777` builds a symlink).
    pub struct Entry<'a> {
        pub name: &'a str,
        pub data: &'a [u8],
        pub mode: u32,
    }

    pub fn file<'a>(name: &'a str, data: &'a [u8]) -> Entry<'a> {
        Entry {
            name,
            data,
            mode: 0o100_755,
        }
    }

    fn u16le(out: &mut Vec<u8>, value: usize) -> TestResult {
        out.extend_from_slice(&u16::try_from(value)?.to_le_bytes());
        Ok(())
    }

    fn u32le(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    /// A hand-rolled writer keeps the replay free of a `zip` tool and can name
    /// entries (`..`, absolute, duplicate) that archivers refuse to create.
    pub fn zip_bytes(entries: &[Entry<'_>]) -> TestResult<Vec<u8>> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for entry in entries {
            let offset = u32::try_from(out.len())?;
            let crc = crc32(entry.data);
            let size = u32::try_from(entry.data.len())?;
            u32le(&mut out, 0x0403_4b50);
            u16le(&mut out, 20)?;
            out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
            u32le(&mut out, crc);
            u32le(&mut out, size);
            u32le(&mut out, size);
            u16le(&mut out, entry.name.len())?;
            u16le(&mut out, 0)?;
            out.extend_from_slice(entry.name.as_bytes());
            out.extend_from_slice(entry.data);

            u32le(&mut central, 0x0201_4b50);
            u16le(&mut central, 0x0314)?;
            u16le(&mut central, 20)?;
            central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
            u32le(&mut central, crc);
            u32le(&mut central, size);
            u32le(&mut central, size);
            u16le(&mut central, entry.name.len())?;
            central.extend_from_slice(&[0; 8]);
            u32le(&mut central, entry.mode << 16);
            u32le(&mut central, offset);
            central.extend_from_slice(entry.name.as_bytes());
        }
        let central_offset = u32::try_from(out.len())?;
        let central_size = u32::try_from(central.len())?;
        out.extend_from_slice(&central);
        u32le(&mut out, 0x0605_4b50);
        out.extend_from_slice(&[0; 4]);
        u16le(&mut out, entries.len())?;
        u16le(&mut out, entries.len())?;
        u32le(&mut out, central_size);
        u32le(&mut out, central_offset);
        u16le(&mut out, 0)?;
        Ok(out)
    }

    fn zip_asset() -> String {
        format!("ripr-server-v{VERSION}-{TARGET}.zip")
    }

    fn release() -> String {
        format!("https://github.com/EffortlessMetrics/ripr/releases/download/v{VERSION}")
    }

    fn sha256_of(root: &Path, bytes: &[u8]) -> TestResult<String> {
        let path = root.join("digest.tmp");
        fs::write(&path, bytes)?;
        let out = Command::new("sha256sum").arg(&path).output()?;
        Ok(String::from_utf8(out.stdout)?
            .split_whitespace()
            .next()
            .ok_or("sha256sum printed nothing")?
            .to_string())
    }

    struct Fake {
        root: PathBuf,
        bin: PathBuf,
        served: PathBuf,
        curl_log: PathBuf,
        cargo_log: PathBuf,
    }

    impl Fake {
        fn new(label: &str) -> TestResult<Fake> {
            let root = scratch(label)?;
            let bin = tool_dir(&root)?;
            let served = root.join("served");
            fs::create_dir_all(&served)?;
            fs::create_dir_all(root.join("winroot/System32"))?;
            let curl_log = root.join("curl.log");
            let cargo_log = root.join("cargo.log");
            stand_in(
                &bin,
                "curl",
                &format!(
                    "for a in \"$@\"; do case \"$a\" in http*) url=\"$a\";; esac; done\n\
                     echo \"$url\" >> '{}'\n\
                     f='{}'/\"$(basename \"$url\")\"\n\
                     [ -f \"$f\" ] || exit 22\n\
                     cp \"$f\" \"$(basename \"$url\")\"",
                    curl_log.display(),
                    served.display()
                ),
            )?;
            stand_in(
                &bin,
                "cargo",
                &format!("echo \"$@\" >> '{}'", cargo_log.display()),
            )?;
            // Ambient ripr that must never be the one that is run.
            stand_in(&bin, "ripr.exe", "echo 'ripr 0.10.0 AMBIENT-RIPR-WAS-RUN'")?;
            stand_in(
                &root.join("winroot/System32"),
                "tar.exe",
                "case \"$1\" in\n\
                 -tf) exec unzip -Z1 \"$2\" ;;\n\
                 -xf) exec unzip -q \"$2\" -d \"$4\" ;;\n\
                 *) echo \"unexpected tar.exe args: $*\" >&2; exit 64 ;;\n\
                 esac",
            )?;
            Ok(Fake {
                root,
                bin,
                served,
                curl_log,
                cargo_log,
            })
        }

        /// Serve the asset and a `.sha256` naming its real digest.
        fn serve_verified(&self, asset: &str, bytes: &[u8]) -> TestResult<String> {
            fs::write(self.served.join(asset), bytes)?;
            let digest = sha256_of(&self.root, bytes)?;
            fs::write(
                self.served.join(format!("{asset}.sha256")),
                format!("{digest}\n"),
            )?;
            Ok(digest)
        }

        fn run(&self, os: &str, arch: &str) -> TestResult<Run> {
            let systemroot = self.root.join("winroot");
            bash(
                &run_block("Install ripr")?,
                &self.root,
                &self.bin,
                &[
                    ("RIPR_INPUT_VERSION", VERSION),
                    ("RUNNER_OS", os),
                    ("RUNNER_ARCH", arch),
                    ("SYSTEMROOT", systemroot.to_str().ok_or("path")?),
                ],
            )
        }

        fn extracted(&self) -> Vec<String> {
            fs::read_dir(self.root.join("runner-temp/ripr-bin"))
                .map(|dir| {
                    dir.filter_map(|entry| entry.ok())
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        }

        fn curl_calls(&self) -> String {
            fs::read_to_string(&self.curl_log).unwrap_or_default()
        }

        fn cargo_calls(&self) -> String {
            fs::read_to_string(&self.cargo_log).unwrap_or_default()
        }
    }

    fn good_exe() -> String {
        format!("#!/bin/sh\necho \"ripr {VERSION} via $0\"")
    }

    fn payload(exe: &[u8]) -> TestResult<Vec<u8>> {
        zip_bytes(&[
            file("LICENSE-APACHE", b"a"),
            file("LICENSE-MIT", b"m"),
            file("README-server.txt", b"r"),
            file("ripr.exe", exe),
        ])
    }

    /// Fail-closed: exit 1, nothing extracted, no cargo, no ambient ripr.
    fn assert_rejected(fake: &Fake, run: &Run, label: &str, message: &str) {
        let text = run.text();
        assert_eq!(run.output.status.code(), Some(1), "{label}: {text}");
        assert!(text.contains(message), "{label}: {text}");
        assert!(fake.extracted().is_empty(), "{label}: extracted");
        assert!(fake.cargo_calls().is_empty(), "{label}: fell back to cargo");
        assert!(!text.contains("AMBIENT"), "{label}: {text}");
        assert!(
            !fake.root.join("evil.exe").exists(),
            "{label}: escaped the archive root"
        );
    }

    #[test]
    fn windows_x64_installs_the_zip_and_runs_it_by_absolute_path() -> TestResult {
        let fake = Fake::new("zip-ok")?;
        let asset = zip_asset();
        let digest = fake.serve_verified(&asset, &payload(good_exe().as_bytes())?)?;
        let run = fake.run("Windows", "X64")?;
        let text = run.text();
        assert!(run.ok(), "{text}");
        let exe = fake.root.join("runner-temp/ripr-bin/ripr.exe");
        assert!(text.contains(&format!("via {}", exe.display())), "{text}");
        assert!(!text.contains("AMBIENT"), "{text}");
        assert!(
            text.contains(&format!(
                "::notice title=ripr install::version {VERSION}, target {TARGET}, archive {asset}, sha256 {digest}, executable {}",
                exe.display()
            )),
            "{text}"
        );
        let release = release();
        assert_eq!(
            fake.curl_calls(),
            format!("{release}/{asset}\n{release}/{asset}.sha256\n")
        );
        assert!(fake.cargo_calls().is_empty(), "cargo ran on the zip route");
        assert!(fs::read_to_string(fake.root.join("github-path"))?.contains("ripr-bin"));
        Ok(())
    }

    #[test]
    fn windows_arm64_is_an_explicit_error_with_no_download_or_cargo() -> TestResult {
        let fake = Fake::new("zip-arm64")?;
        fake.serve_verified(&zip_asset(), &payload(good_exe().as_bytes())?)?;
        let run = fake.run("Windows", "ARM64")?;
        let text = run.text();
        assert_eq!(run.output.status.code(), Some(1), "{text}");
        assert!(
            text.contains("::error::Unsupported runner Windows-ARM64:"),
            "{text}"
        );
        assert!(fake.curl_calls().is_empty() && fake.cargo_calls().is_empty());
        assert!(fake.extracted().is_empty());
        Ok(())
    }

    #[test]
    fn a_zip_checksum_that_is_wrong_empty_or_missing_is_final() -> TestResult {
        let asset = zip_asset();
        let bytes = payload(good_exe().as_bytes())?;

        let mismatch = Fake::new("zip-mismatch")?;
        fs::write(mismatch.served.join(&asset), &bytes)?;
        fs::write(
            mismatch.served.join(format!("{asset}.sha256")),
            format!("{}\n", "0".repeat(64)),
        )?;
        let run = mismatch.run("Windows", "X64")?;
        assert_rejected(
            &mismatch,
            &run,
            "mismatch",
            "does not match its published SHA-256",
        );

        let empty = Fake::new("zip-empty-sum")?;
        fs::write(empty.served.join(&asset), &bytes)?;
        fs::write(empty.served.join(format!("{asset}.sha256")), "")?;
        let run = empty.run("Windows", "X64")?;
        assert_rejected(
            &empty,
            &run,
            "empty",
            "does not match its published SHA-256",
        );

        let missing = Fake::new("zip-missing-sum")?;
        fs::write(missing.served.join(&asset), &bytes)?;
        let run = missing.run("Windows", "X64")?;
        assert_rejected(
            &missing,
            &run,
            "missing",
            "refusing to install it unverified",
        );
        Ok(())
    }

    #[test]
    fn unsafe_or_malformed_zip_listings_are_rejected_before_extraction() -> TestResult {
        let exe = good_exe();
        let shapes: Vec<(&str, Vec<u8>, &str)> = vec![
            (
                "missing-exe",
                zip_bytes(&[file("LICENSE-MIT", b"m")])?,
                "does not contain ripr.exe at its root",
            ),
            (
                "nested",
                zip_bytes(&[file("ripr-server/ripr.exe", exe.as_bytes())])?,
                "outside a flat archive root",
            ),
            (
                "traversal",
                zip_bytes(&[file("../evil.exe", b"x"), file("ripr.exe", exe.as_bytes())])?,
                "outside a flat archive root",
            ),
            (
                "absolute",
                zip_bytes(&[
                    file("/tmp/ripr-evil", b"x"),
                    file("ripr.exe", exe.as_bytes()),
                ])?,
                "outside a flat archive root",
            ),
            (
                "drive-letter",
                zip_bytes(&[file("C:evil.exe", b"x"), file("ripr.exe", exe.as_bytes())])?,
                "outside a flat archive root",
            ),
            (
                "backslash",
                zip_bytes(&[file("..\\evil.exe", b"x"), file("ripr.exe", exe.as_bytes())])?,
                "outside a flat archive root",
            ),
            (
                "duplicate",
                zip_bytes(&[
                    file("ripr.exe", exe.as_bytes()),
                    file("ripr.exe", b"#!/bin/sh\necho evil"),
                ])?,
                "duplicate entry",
            ),
            (
                "not-a-zip",
                b"this is not a zip archive".to_vec(),
                "is not a readable zip archive",
            ),
        ];
        for (label, bytes, message) in shapes {
            let fake = Fake::new(&format!("zip-{label}"))?;
            fake.serve_verified(&zip_asset(), &bytes)?;
            let run = fake.run("Windows", "X64")?;
            assert_rejected(&fake, &run, label, message);
        }

        // A symlink entry named ripr.exe passes the listing but is not a
        // regular file once extracted.
        let link = Fake::new("zip-symlink")?;
        link.serve_verified(
            &zip_asset(),
            &zip_bytes(&[Entry {
                name: "ripr.exe",
                data: b"/bin/sh",
                mode: 0o120_777,
            }])?,
        )?;
        let run = link.run("Windows", "X64")?;
        let text = run.text();
        assert_eq!(run.output.status.code(), Some(1), "{text}");
        assert!(
            text.contains("did not extract a regular file named ripr.exe"),
            "{text}"
        );
        assert!(link.cargo_calls().is_empty() && !text.contains("AMBIENT"));
        Ok(())
    }

    #[test]
    fn a_very_large_listing_cannot_hide_a_traversal_entry() -> TestResult {
        let exe = good_exe();
        let names: Vec<String> = (0..60_000)
            .map(|n| format!("filler-entry-{n:06}.txt"))
            .collect();
        let mut entries = vec![file("../x", b"x"), file("ripr.exe", exe.as_bytes())];
        entries.extend(names.iter().map(|name| file(name, b"")));
        let bad = Fake::new("zip-large-bad")?;
        bad.serve_verified(&zip_asset(), &zip_bytes(&entries)?)?;
        let run = bad.run("Windows", "X64")?;
        assert_rejected(&bad, &run, "large", "outside a flat archive root");

        // A large clean listing still finds ripr.exe and installs.
        let mut entries = vec![file("ripr.exe", exe.as_bytes())];
        entries.extend(names.iter().map(|name| file(name, b"")));
        let clean = Fake::new("zip-large-clean")?;
        clean.serve_verified(&zip_asset(), &zip_bytes(&entries)?)?;
        let run = clean.run("Windows", "X64")?;
        assert!(run.ok(), "{}", run.text());
        Ok(())
    }

    #[test]
    fn the_reported_version_must_equal_the_pin_not_merely_start_with_it() -> TestResult {
        for reported in [
            "0.10.00".to_string(),
            "0.10.0-rc.1".to_string(),
            "0.10.0.9".to_string(),
            "0.0.1".to_string(),
        ] {
            let fake = Fake::new("zip-version")?;
            fake.serve_verified(
                &zip_asset(),
                &payload(format!("#!/bin/sh\necho 'ripr {reported} (abc)'").as_bytes())?,
            )?;
            let run = fake.run("Windows", "X64")?;
            let text = run.text();
            assert_eq!(run.output.status.code(), Some(1), "{reported}: {text}");
            assert!(
                text.contains(&format!("not ripr {VERSION}")),
                "{reported}: {text}"
            );
            assert!(fake.cargo_calls().is_empty(), "{reported}");
        }

        // The pin with a build suffix is accepted.
        let ok = Fake::new("zip-version-suffix")?;
        ok.serve_verified(
            &zip_asset(),
            &payload(format!("#!/bin/sh\necho 'ripr {VERSION} (abc 2026-01-01)'").as_bytes())?,
        )?;
        let run = ok.run("Windows", "X64")?;
        assert!(run.ok(), "{}", run.text());
        Ok(())
    }

    #[test]
    fn a_rejected_tar_gz_checksum_does_not_fall_back_to_cargo() -> TestResult {
        let fake = Fake::new("targz-mismatch")?;
        let asset = format!("ripr-server-v{VERSION}-x86_64-unknown-linux-gnu.tar.gz");
        fs::write(fake.served.join(&asset), b"not-a-tarball")?;
        fs::write(
            fake.served.join(format!("{asset}.sha256")),
            format!("{}\n", "0".repeat(64)),
        )?;
        let run = fake.run("Linux", "X64")?;
        assert_rejected(&fake, &run, "targz", "does not match its published SHA-256");
        Ok(())
    }
}

fn git(dir: &Path, args: &[&str]) -> TestResult {
    let out = Command::new("git")
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .current_dir(dir)
        .output()?;
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// A clone of a two-branch remote that has only the feature branch fetched,
/// like a single-branch `actions/checkout`.
fn single_branch_clone(root: &Path) -> TestResult<PathBuf> {
    let origin = root.join("origin");
    fs::create_dir_all(&origin)?;
    git(&origin, &["init", "-q", "-b", "main"])?;
    fs::write(origin.join("a.txt"), "1")?;
    git(&origin, &["add", "."])?;
    git(&origin, &["commit", "-qm", "base"])?;
    git(&origin, &["checkout", "-qb", "feature"])?;
    fs::write(origin.join("a.txt"), "2")?;
    git(&origin, &["commit", "-qam", "change"])?;
    let work = root.join("work");
    git(
        root,
        &[
            "clone",
            "-q",
            "--single-branch",
            "--branch",
            "feature",
            origin.to_str().ok_or("path")?,
            work.to_str().ok_or("path")?,
        ],
    )?;
    Ok(work)
}

fn check(work: &Path, bin: &Path, base: &str, base_ref: Option<&str>) -> TestResult<Run> {
    let mut env = vec![
        ("RIPR_INPUT_BASE", base),
        ("RIPR_INPUT_MODE", "draft"),
        ("RIPR_INPUT_FORMAT", "human"),
    ];
    if let Some(base_ref) = base_ref {
        env.push(("GITHUB_BASE_REF", base_ref));
    }
    bash(&run_block("Run ripr check")?, work, bin, &env)
}

fn recorded_argv(work: &Path) -> TestResult<String> {
    Ok(fs::read_to_string(work.join("ripr.argv"))?
        .trim()
        .to_string())
}

#[test]
fn a_pull_request_base_is_derived_and_fetched_when_the_checkout_lacks_it() -> TestResult {
    let root = scratch("base")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let missing = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main^{commit}"])
        .current_dir(&work)
        .status()?;
    assert!(!missing.success(), "fixture must start without origin/main");

    let run = check(&work, &bin, "", Some("main"))?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        recorded_argv(&work)?,
        "check --root . --mode draft --format human --base origin/main"
    );
    let fetched = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main^{commit}"])
        .current_dir(&work)
        .status()?;
    assert!(fetched.success(), "the base branch was not fetched");
    Ok(())
}

#[test]
fn outside_a_pull_request_the_default_base_is_left_to_ripr() -> TestResult {
    let root = scratch("no-base")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let run = check(&work, &bin, "", None)?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        recorded_argv(&work)?,
        "check --root . --mode draft --format human"
    );
    Ok(())
}

#[test]
fn an_explicit_base_wins_over_the_pull_request_base() -> TestResult {
    let root = scratch("explicit")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let run = check(&work, &bin, "origin/feature~1", Some("main"))?;
    assert!(run.ok(), "{}", run.text());
    assert_eq!(
        recorded_argv(&work)?,
        "check --root . --mode draft --format human --base origin/feature~1"
    );
    Ok(())
}

#[test]
fn an_unfetchable_base_warns_and_leaves_the_message_to_ripr() -> TestResult {
    let root = scratch("unfetchable")?;
    let bin = tool_dir(&root)?;
    stand_in(&bin, "ripr", "echo \"$@\" > \"$PWD/ripr.argv\"")?;
    let work = single_branch_clone(&root)?;
    let run = check(&work, &bin, "", Some("no-such-branch"))?;
    assert!(run.ok(), "{}", run.text());
    assert!(
        run.text()
            .contains("could not fetch the pull request base branch"),
        "{}",
        run.text()
    );
    assert!(recorded_argv(&work)?.ends_with("--base origin/no-such-branch"));
    Ok(())
}
