use super::super::{capture, field, invoke, tool};
use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Result<Fixture> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = std::env::temp_dir().join(format!("coverage-elf-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).map_err(|e| e.to_string())?;
    for root in ROOTS {
        fs::create_dir_all(path.join(root)).map_err(|e| e.to_string())?;
    }
    Ok(Fixture(path))
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn raw(id: &[u8]) -> Vec<u8> {
    let block = (8 + id.len() + 7) & !7;
    let mut bytes = vec![0; HEADER_BYTES + block];
    bytes[..8].copy_from_slice(&[0x81, 0x72, 0x66, 0x6f, 0x72, 0x70, 0x6c, 0xff]);
    put64(&mut bytes, 8, 10);
    put64(&mut bytes, 16, block as u64);
    put64(&mut bytes, 24, 1000);
    put64(&mut bytes, 120, 2);
    put64(&mut bytes, 128, id.len() as u64);
    bytes[136..136 + id.len()].copy_from_slice(id);
    bytes
}
fn raw_ids(ids: &[Vec<u8>]) -> Vec<u8> {
    let block_len = ids.iter().map(|id| (8 + id.len() + 7) & !7).sum::<usize>();
    let mut bytes = vec![0; HEADER_BYTES + block_len];
    bytes[..8].copy_from_slice(&[0x81, 0x72, 0x66, 0x6f, 0x72, 0x70, 0x6c, 0xff]);
    put64(&mut bytes, 8, 10);
    put64(&mut bytes, 16, block_len as u64);
    let mut at = HEADER_BYTES;
    for id in ids {
        put64(&mut bytes, at, id.len() as u64);
        at += 8;
        bytes[at..at + id.len()].copy_from_slice(id);
        at += (id.len() + 7) & !7;
    }
    bytes
}
fn elf_bytes(id: &[u8], instrumented: bool) -> Vec<u8> {
    let mut bytes = vec![0; 896];
    bytes[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 62);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 32, 64);
    put64(&mut bytes, 40, 512);
    for (at, value) in [(52, 64), (54, 56), (56, 1), (58, 64), (60, 5), (62, 1)] {
        put16(&mut bytes, at, value);
    }
    let note_size = 16 + ((id.len() + 3) & !3);
    put32(&mut bytes, 64, 4);
    put64(&mut bytes, 72, 128);
    put64(&mut bytes, 96, note_size as u64);
    put64(&mut bytes, 104, note_size as u64);
    put32(&mut bytes, 128, 4);
    put32(&mut bytes, 132, id.len() as u32);
    put32(&mut bytes, 136, 3);
    bytes[140..144].copy_from_slice(b"GNU\0");
    bytes[144..144 + id.len()].copy_from_slice(id);
    let names = b"\0.shstrtab\0.note.gnu.build-id\0__llvm_prf_data\0__llvm_prf_cnts\0";
    bytes[192..192 + names.len()].copy_from_slice(names);
    for (index, name_at, kind, offset, length, flags) in [
        (1, 1, 3, 192, names.len() as u64, 0),
        (2, 11, 7, 128, note_size as u64, 2),
        (3, 30, 1, 176, if instrumented { 8 } else { 0 }, 3),
        (4, 46, 1, 184, if instrumented { 8 } else { 0 }, 3),
    ] {
        let at = 512 + index * 64;
        put32(&mut bytes, at, name_at);
        put32(&mut bytes, at + 4, kind);
        put64(&mut bytes, at + 8, flags);
        put64(&mut bytes, at + 24, offset);
        put64(&mut bytes, at + 32, length);
    }
    bytes
}
fn write_executable(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
fn ids(id: &[u8]) -> Vec<String> {
    vec![hex(id)]
}

fn mock_replay(args: &[String]) -> Result<Value> {
    if args.iter().any(|arg| arg == "merge") {
        let listing = args
            .windows(2)
            .find(|pair| pair[0] == "-f")
            .map(|p| PathBuf::from(&p[1]))
            .ok_or("list missing")?;
        let names = fs::read_to_string(listing).map_err(|e| e.to_string())?;
        let stderr = names
            .lines()
            .map(|name| format!("warning: {name}: invalid instrumentation profile data\n"))
            .collect::<String>();
        return Ok(super::super::tool_result(
            Some(1),
            b"",
            stderr.as_bytes(),
            false,
            false,
            false,
        ));
    }
    let stdout = if args == ["rustc", "-vV"] {
        "host: x86_64-unknown-linux-gnu\n"
    } else if args == ["rustc", "--print", "sysroot"] {
        "/test/sysroot\n"
    } else {
        "LLVM test\n"
    };
    Ok(super::super::tool_result(
        Some(0),
        stdout.as_bytes(),
        b"",
        false,
        false,
        false,
    ))
}
#[test]
fn retained_october_14_header_maps_only_instrumented_exact_id_and_preserves_failure() -> Result<()>
{
    // Genuine first 160 bytes supplied from run 38613901957, artifact 11919569558.
    // Archive SHA256: b9c3e8be69a634a904e01755cbf0513a95b293b12ce952f622d36bece996389d.
    // Retained 65536-byte prefix SHA256: 46391932755c51850d3954e70223ce5455c1a341de72fcae932a8b2a7f6e6702.
    // This stimulus contains only the retained header/ID, not the historical full file.
    let retained: [u8; 160] = [
        0x81, 0x72, 0x66, 0x6f, 0x72, 0x70, 0x6c, 0xff, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x17, 0x69, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xaf, 0x3b, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0x40, 0x04,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x88, 0x22, 0xf6, 0xff, 0xff, 0xff, 0xff, 0xff, 0xe8, 0x37,
        0xd8, 0x2e, 0x09, 0xaa, 0xff, 0xff, 0x24, 0xb2, 0xd2, 0xcc, 0xf6, 0x55, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xd0, 0xf6, 0x05, 0x24, 0xb0, 0xf7, 0x07, 0xde, 0xca, 0x8f, 0x76, 0xd9, 0xad, 0x8c,
        0x4c, 0x58, 0xac, 0x98, 0x08, 0xc9, 0x00, 0x00, 0x00, 0x00,
    ];
    let id = &retained[136..156];
    let expected = "d0f60524b0f707deca8f76d9ad8c4c58ac9808c9";
    assert_eq!(hex(id), expected);
    assert_eq!(
        format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(retained)),
        "27ab275bf87b3cad72b9c9b3744affdf31798fb6278c6a0406cb837be13c166f"
    );
    assert_eq!(profile_ids(&retained)["ids"], json!([expected]));
    assert_eq!(profile_ids(&retained)["status"], "available");
    // Independently bounded raw64-v10 declared extent; original EOF was in counters.
    let data_end = 128 + 32 + u64le(&retained, 24)? * 64;
    let counters_end = data_end + u64le(&retained, 40)? * 8;
    let minimum = counters_end + ((u64le(&retained, 72)? + 7) & !7);
    assert_eq!(minimum, 2_647_048);
    assert!(data_end < 2_150_400 && 2_150_400 < counters_end);

    let f = fixture()?;
    let profile = f.0.join("retained-header-123-456_0.profraw");
    fs::write(&profile, retained).map_err(|e| e.to_string())?;
    let list = f.0.join("ripr-swarm-profraw-list");
    fs::write(&list, format!("{}\n", profile.display())).map_err(|e| e.to_string())?;
    // Controlled ELF fixtures test association; neither is the historical producer.
    write_executable(&f.0.join("debug/deps/exact"), &elf_bytes(id, true))?;
    write_executable(&f.0.join("debug/deps/wrong"), &elf_bytes(&[0x42; 20], true))?;
    write_executable(
        &f.0.join("debug/deps/uninstrumented"),
        &elf_bytes(id, false),
    )?;
    let out = f.0.join("evidence");
    let report = capture(&f.0, &list, &out, tool)?;
    assert_eq!(report["merge_replay"]["exit"], 1);
    assert_eq!(report["original_coverage_outcome"], "failure");
    assert_eq!(report["captured"][0]["retained_bytes"], 160);
    assert_eq!(
        report["captured"][0]["binary_ids"]["ids"],
        json!([expected])
    );
    assert_eq!(report["executable_mapping"]["ids"][0]["status"], "matched");
    assert_eq!(report["executable_mapping"]["ids"][0]["ambiguous"], false);
    assert_eq!(
        report["executable_mapping"]["candidates"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        report["executable_mapping"]["candidates"][0]["relative_path"],
        "debug/deps/exact"
    );
    assert_eq!(report["executable_mapping"]["non_candidates"], 1);
    assert_eq!(fs::read(&profile).map_err(|e| e.to_string())?, retained);
    assert_eq!(
        fs::read(out.join("profile-00.prefix.profraw")).map_err(|e| e.to_string())?,
        retained
    );
    assert!(out.join("manifest.json").is_file());
    Ok(())
}

#[test]
fn raw_binary_ids_are_exact_and_unknown_layouts_are_explicit() {
    let id = [0xab; 20];
    let bytes = raw(&id);
    assert_eq!(profile_ids(&bytes)["ids"], json!([hex(&id)]));
    for (name, bad) in [
        ("short", vec![0; 24]),
        ("magic", vec![0; 160]),
        ("version", {
            let mut b = bytes.clone();
            put64(&mut b, 8, 11);
            b
        }),
        ("large", {
            let mut b = bytes.clone();
            put64(&mut b, 16, u64::MAX);
            b
        }),
        ("zero_id", {
            let mut b = bytes.clone();
            put64(&mut b, 128, 0);
            b
        }),
        ("short_id", bytes[..bytes.len() - 1].to_vec()),
    ] {
        assert_eq!(profile_ids(&bad)["status"], "unavailable", "{name}");
    }
}
#[test]
fn profile_id_count_limit_keeps_the_valid_prefix_and_marks_mapping_partial() -> Result<()> {
    let f = fixture()?;
    let valid_ids = (1..=33).map(|n| vec![n; 20]).collect::<Vec<_>>();
    let parsed = profile_ids(&raw_ids(&valid_ids));
    assert_eq!(parsed["status"], "partial");
    assert_eq!(parsed["reason"], "id_count_limit");
    assert_eq!(parsed["truncated"], true);
    assert_eq!(parsed["ids"].as_array().map(Vec::len), Some(32));
    write_executable(
        &f.0.join("debug/deps/first"),
        &elf_bytes(&valid_ids[0], true),
    )?;
    let report = associate(&f.0, &[json!({"binary_ids": parsed})]);
    assert_eq!(report["candidates"].as_array().map(Vec::len), Some(1));
    assert_eq!(report["ids"][0]["status"], "matched");
    assert_eq!(
        report["ids"][1]["status"],
        "unavailable_within_partial_scope"
    );
    assert_eq!(report["profile_ids_incomplete"], true);
    assert_eq!(report["profile_ids_truncated"], false);
    assert_eq!(report["status"], "partial");
    Ok(())
}
#[test]
fn unavailable_profile_id_input_prevents_a_definitive_negative_mapping() -> Result<()> {
    let f = fixture()?;
    let known = profile_ids(&raw(&[0x42; 20]));
    let unsupported = profile_ids(&[0; HEADER_BYTES]);
    let report = associate(
        &f.0,
        &[
            json!({"binary_ids": known}),
            json!({"binary_ids": unsupported}),
        ],
    );
    assert_eq!(report["profile_ids_incomplete"], true);
    assert_eq!(report["status"], "partial");
    assert_eq!(
        report["ids"][0]["status"],
        "unavailable_within_partial_scope"
    );
    Ok(())
}
#[test]
fn exact_instrumented_matches_preserve_duplicate_ambiguity() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    write_executable(&f.0.join("debug/deps/one"), &elf_bytes(&id, true))?;
    write_executable(&f.0.join("debug/deps/two"), &elf_bytes(&id, true))?;
    let report = associate_with_limits(&f.0, &ids(&id), LIMITS);
    assert_eq!(report["ids"][0]["status"], "matched");
    assert_eq!(report["ids"][0]["ambiguous"], true);
    assert_eq!(report["candidates"].as_array().map(Vec::len), Some(2));
    assert_eq!(report["files_inspected"], 2);
    Ok(())
}
#[test]
fn wrong_id_and_same_id_without_instrumentation_do_not_match() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    write_executable(&f.0.join("debug/deps/wrong"), &elf_bytes(&[0x41; 20], true))?;
    let clean = associate_with_limits(&f.0, &ids(&id), LIMITS);
    assert_eq!(
        clean["ids"][0]["status"],
        "not_found_within_inspected_scope"
    );
    write_executable(
        &f.0.join("debug/deps/not-instrumented"),
        &elf_bytes(&id, false),
    )?;
    let partial = associate_with_limits(&f.0, &ids(&id), LIMITS);
    assert_eq!(partial["candidates"], json!([]));
    assert_eq!(partial["status"], "inspected_scope");
    assert_eq!(
        partial["ids"][0]["status"],
        "not_found_within_inspected_scope"
    );
    assert_eq!(partial["non_candidates"], 1);
    assert_eq!(partial["candidates_unavailable"], 0);
    assert_eq!(partial["error_samples"], json!([]));
    Ok(())
}
#[test]
fn malformed_and_platform_elf_headers_and_notes_are_bounded() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    let path = f.0.join("debug/deps/subject");
    let original = elf_bytes(&id, true);
    let malformed = [
        vec![0; 20],
        {
            let mut b = original.clone();
            b[4] = 1;
            b
        },
        {
            let mut b = original.clone();
            b[5] = 2;
            b
        },
        {
            let mut b = original.clone();
            put16(&mut b, 60, u16::MAX);
            b
        },
        {
            let mut b = original.clone();
            put64(&mut b, 40, u64::MAX);
            b
        },
        {
            let mut b = original.clone();
            put32(&mut b, 132, u32::MAX);
            b
        },
        {
            let mut b = original.clone();
            put64(&mut b, 512 + 2 * 64 + 32, u64::MAX);
            b
        },
        {
            let mut b = original.clone();
            put32(&mut b, 64, 1);
            b
        },
    ];
    for bytes in malformed {
        write_executable(&path, &bytes)?;
        let report = associate_with_limits(&f.0, &ids(&id), LIMITS);
        assert_eq!(report["candidates"], json!([]));
        assert_eq!(report["candidates_unavailable"], 1);
        assert_eq!(report["status"], "partial");
        assert_eq!(
            report["ids"][0]["status"],
            "unavailable_within_partial_scope"
        );
        assert!(report["read_bytes"].as_u64().unwrap_or(u64::MAX) <= 65536);
    }
    Ok(())
}
#[test]
fn every_scan_limit_is_visible_even_without_a_successful_candidate() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    for n in 0..4 {
        write_executable(&f.0.join(format!("debug/deps/{n}")), &elf_bytes(&id, true))?;
    }
    for limited in [
        Limits {
            entries: 1,
            ..LIMITS
        },
        Limits { files: 1, ..LIMITS },
        Limits {
            bytes: 63,
            ..LIMITS
        },
        Limits {
            matches: 1,
            ..LIMITS
        },
        Limits {
            elapsed: Duration::ZERO,
            ..LIMITS
        },
    ] {
        let report = associate_with_limits(&f.0, &ids(&id), limited);
        assert_eq!(report["truncated"], true);
        assert_ne!(report["status"], "inspected_scope");
        assert!(report["read_bytes"].as_u64().unwrap_or(0) <= limited.bytes as u64);
    }
    Ok(())
}
#[test]
fn outside_roots_symlinks_and_changed_file_identity_are_refused() -> Result<()> {
    let f = fixture()?;
    let outside = fixture()?;
    let id = [0x42; 20];
    write_executable(
        &outside.0.join("debug/deps/external"),
        &elf_bytes(&id, true),
    )?;
    std::os::unix::fs::symlink(
        outside.0.join("debug/deps/external"),
        f.0.join("debug/deps/link"),
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(
        associate_with_limits(&f.0, &ids(&id), LIMITS)["candidates"],
        json!([])
    );
    let root_link = f.0.join("linked-root");
    std::os::unix::fs::symlink(&outside.0, &root_link).map_err(|e| e.to_string())?;
    assert_eq!(
        associate_with_limits(&root_link, &ids(&id), LIMITS)["status"],
        "unavailable"
    );
    assert!(
        matches!(elf::directory(&f.0.join("../")), Err(reason) if reason == "outside_or_non_normal_root")
    );
    let path = f.0.join("debug/deps/rebound");
    write_executable(&path, &elf_bytes(&id, true))?;
    let expected = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    fs::rename(&path, f.0.join("saved")).map_err(|e| e.to_string())?;
    write_executable(&path, &elf_bytes(&id, true))?;
    let mut budget = elf::Budget {
        bytes: 0,
        limit: LIMITS.bytes,
        deadline: Instant::now() + LIMITS.elapsed,
    };
    assert_eq!(
        elf::inspect(&path, &expected, &mut budget).err().as_deref(),
        Some("identity_changed_or_nonregular")
    );
    assert_eq!(budget.bytes, 0);
    Ok(())
}
#[test]
fn fifo_replacement_is_nonblocking_and_never_read() -> Result<()> {
    let f = fixture()?;
    let path = f.0.join("debug/deps/rebound");
    write_executable(&path, &elf_bytes(&[0x42; 20], true))?;
    let expected = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    fs::rename(&path, f.0.join("saved")).map_err(|e| e.to_string())?;
    let argv = vec!["mkfifo".to_owned(), path.to_string_lossy().into_owned()];
    assert_eq!(tool(&argv)?["exit"], 0);
    let mut budget = elf::Budget {
        bytes: 0,
        limit: LIMITS.bytes,
        deadline: Instant::now() + LIMITS.elapsed,
    };
    let started = Instant::now();
    assert_eq!(
        elf::inspect(&path, &expected, &mut budget).err().as_deref(),
        Some("identity_changed_or_nonregular")
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(budget.bytes, 0);
    Ok(())
}
#[test]
fn mapping_failure_cannot_abort_the_original_profile_manifest() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    let profile = f.0.join("worker-123-456_0.profraw");
    let data = raw(&id);
    fs::write(&profile, &data).map_err(|e| e.to_string())?;
    let list = f.0.join("ripr-swarm-profraw-list");
    fs::write(&list, format!("{}\n", profile.display())).map_err(|e| e.to_string())?;
    let malformed = f.0.join("debug/deps/bad");
    write_executable(&malformed, b"not an ELF")?;
    let out = f.0.join("evidence");
    let report = capture(&f.0, &list, &out, mock_replay)?;
    assert_eq!(
        report["captured"][0]["binary_ids"]["ids"],
        json!([hex(&id)])
    );
    assert_eq!(report["original_coverage_outcome"], "failure");
    assert_eq!(report["merge_replay"]["exit"], 1);
    assert_eq!(report["profile_list_source_recheck"]["status"], "unchanged");
    assert_eq!(report["executable_mapping"]["status"], "partial");
    assert_eq!(fs::read(&profile).map_err(|e| e.to_string())?, data);
    assert!(out.join("manifest.json").is_file());
    Ok(())
}
#[test]
fn real_rust_llvm_binary_id_matches_sparse_instrumented_elf_metadata() -> Result<()> {
    let f = fixture()?;
    let source = f.0.join("writer.rs");
    fs::write(&source,b"fn main() { let n=std::env::args().count(); if n>1 { println!(\"many\"); } else { println!(\"one\"); } }").map_err(|e|e.to_string())?;
    let executable = f.0.join("debug/deps/writer");
    let compile = tool(&[
        "rustc".into(),
        "--edition=2024".into(),
        "-C".into(),
        "instrument-coverage".into(),
        "-C".into(),
        "strip=symbols".into(),
        "-C".into(),
        "link-arg=-Wl,--build-id=sha1".into(),
        source.to_string_lossy().into_owned(),
        "-o".into(),
        executable.to_string_lossy().into_owned(),
    ])?;
    assert_eq!(compile["exit"], 0, "{compile}");
    let full = f.0.join("full.profraw");
    let run = tool(&[
        "env".into(),
        format!("LLVM_PROFILE_FILE={}", full.display()),
        executable.to_string_lossy().into_owned(),
    ])?;
    assert_eq!(run["exit"], 0);
    let full_bytes = super::super::read_prefix(&full, super::super::PREFIX_BYTES)?;
    let observed = profile_ids(&full_bytes);
    assert_eq!(observed["status"], "available");
    assert_eq!(observed["ids"].as_array().map(Vec::len), Some(1));
    let id = observed["ids"][0]
        .as_str()
        .ok_or("real profile id missing")?;
    let sysroot = invoke(&tool, &["rustc", "--print", "sysroot"]);
    assert_eq!(sysroot["exit"], 0);
    let version = invoke(&tool, &["rustc", "-vV"]);
    assert_eq!(version["exit"], 0);
    let host = field(&version, "stdout")
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .ok_or("host missing")?;
    let profdata = Path::new(field(&sysroot, "stdout").trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin/llvm-profdata");
    let show = tool(&[
        profdata.to_string_lossy().into_owned(),
        "show".into(),
        "--binary-ids".into(),
        full.to_string_lossy().into_owned(),
    ])?;
    assert_eq!(show["exit"], 0, "{show}");
    assert!(field(&show, "stdout").contains(id));
    let short = f.0.join("worker-123-456_0.profraw");
    let kept =
        HEADER_BYTES + usize::try_from(u64le(&full_bytes, 16)?).map_err(|e| e.to_string())?;
    fs::write(&short, &full_bytes[..kept]).map_err(|e| e.to_string())?;
    let list = f.0.join("ripr-swarm-profraw-list");
    fs::write(&list, format!("{}\n", short.display())).map_err(|e| e.to_string())?;
    let report = capture(&f.0, &list, &f.0.join("evidence"), tool)?;
    assert_eq!(report["merge_replay"]["exit"], 1);
    assert_eq!(report["executable_mapping"]["ids"][0]["status"], "matched");
    assert_eq!(
        report["executable_mapping"]["candidates"][0]["relative_path"],
        "debug/deps/writer"
    );
    assert_eq!(
        report["executable_mapping"]["candidates"][0]["gnu_build_ids"],
        json!([id])
    );
    assert_eq!(report["original_coverage_outcome"], "failure");
    assert_eq!(
        fs::read(&short).map_err(|e| e.to_string())?,
        full_bytes[..kept]
    );
    assert_eq!(
        super::super::read_prefix(&full, super::super::PREFIX_BYTES)?,
        full_bytes
    );
    eprintln!(
        "real LLVM association: build-id={id}, retained={kept}, mapping={}, original=failure, replay=1",
        report["executable_mapping"]["ids"][0]["status"]
    );
    Ok(())
}

#[test]
fn final_mapping_json_budget_includes_profile_scope_metadata() -> Result<()> {
    let f = fixture()?;
    let primary = vec![1; 20];
    let mut bytes = elf_bytes(&primary, true);
    bytes.resize(8192, 0);
    let table = bytes[512..832].to_vec();
    let names = bytes[192..254].to_vec();
    bytes[7424..7744].copy_from_slice(&table);
    bytes[4096..4158].copy_from_slice(&names);
    put64(&mut bytes, 40, 7424);
    for (index, offset) in [(1, 4096), (3, 4608), (4, 4616)] {
        put64(&mut bytes, 7424 + index * 64 + 24, offset);
    }
    let mut notes = Vec::new();
    for id in std::iter::once(primary).chain((0..23).map(|n| vec![64 + n; 128])) {
        notes.extend_from_slice(&4_u32.to_le_bytes());
        notes.extend_from_slice(&(id.len() as u32).to_le_bytes());
        notes.extend_from_slice(&3_u32.to_le_bytes());
        notes.extend_from_slice(b"GNU\0");
        notes.extend_from_slice(&id);
        while !notes.len().is_multiple_of(4) {
            notes.push(0);
        }
    }
    bytes[128..128 + notes.len()].copy_from_slice(&notes);
    put64(&mut bytes, 96, notes.len() as u64);
    put64(&mut bytes, 104, notes.len() as u64);
    put64(&mut bytes, 7424 + 2 * 64 + 32, notes.len() as u64);
    let captured = vec![
        json!({"binary_ids": {"status": "available", "ids": (1..=32).map(|n|hex(&[n;20])).collect::<Vec<_>>()}}),
    ];
    let mut paths = Vec::new();
    for n in 0..4 {
        let path = f.0.join(format!("debug/deps/f{n}"));
        write_executable(&path, &bytes)?;
        paths.push(path);
    }
    let mut near_limit = false;
    let mut rejected = false;
    for padding in 0..149 {
        for (index, path) in paths.iter_mut().enumerate() {
            let next =
                f.0.join(format!("debug/deps/f{index}{}", "x".repeat(padding)));
            if next != *path {
                fs::rename(&*path, &next).map_err(|e| e.to_string())?;
                *path = next;
            }
        }
        let report = associate(&f.0, &captured);
        let encoded = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        assert!(
            encoded.len() <= JSON_BYTES,
            "final mapping JSON exceeded its declared bound: {}",
            encoded.len()
        );
        if report["status"] == "inspected_scope" && encoded.len() > JSON_BYTES - 200 {
            near_limit = true;
        }
        if report["reason"] == "json_limit" {
            assert_eq!(report["truncated"], true);
            assert_eq!(report["ids"][0]["status"], "unavailable");
            rejected = true;
            break;
        }
    }
    assert!(
        near_limit && rejected,
        "must exercise accepted and rejected sides of final JSON boundary"
    );
    Ok(())
}
