//! Real portable-consumer receipt paths, including native Windows (#8067).
//!
//! This std-only harness can also run with rustc --test against an explicitly
//! frozen CARGO_BIN_EXE_ripr. That proves transport behavior for that payload;
//! it does not qualify the current source tree or replace Cargo integration CI.

use std::path::Path;
use std::process::Command;

const JOURNEY: &str = r#"
import hashlib, json, os, pathlib, shutil, subprocess, sys, tempfile

workspace = pathlib.Path(sys.argv[1]).resolve()
payload = pathlib.Path(sys.argv[2]).resolve()
sha = lambda data: hashlib.sha256(data).hexdigest()
payload_sha = sha(payload.read_bytes())
expected_payload = os.environ.get('RIPR_PORTABILITY_EXPECTED_PAYLOAD_SHA256')
if expected_payload:
    assert payload_sha == expected_payload.lower(), (payload_sha, expected_payload)

def digest(root):
    # Independent contract formula; never ask run.py to supply its own oracle.
    entries = sorted((p.relative_to(root).as_posix(), sha(p.read_bytes()))
                     for p in root.rglob('*') if p.is_file()
                     and not {'target', '.git'}.intersection(p.parent.relative_to(root).parts))
    return sha('\n'.join(name + '=' + value for name, value in entries).encode())

retained = os.environ.get('RIPR_PORTABILITY_PROOF_ROOT')
temporary = None if retained else tempfile.TemporaryDirectory(prefix='ripr receipt paths ')
root = pathlib.Path(retained or temporary.name).resolve()
root.mkdir(parents=True, exist_ok=True)
original = root / 'original location with spaces'
packet, subject = original / 'packet files', original / 'subject files'
packet.mkdir(parents=True)
shutil.copytree(workspace / 'fixtures/boundary_gap/input', subject)
diff = original / 'change with spaces.diff'
shutil.copyfile(workspace / 'fixtures/boundary_gap/diff.patch', diff)
binary_name = 'ripr.exe' if os.name == 'nt' else 'ripr'
shutil.copyfile(payload, packet / binary_name)
if os.name != 'nt':
    (packet / binary_name).chmod(0o755)
shutil.copyfile(workspace / 'tools/python/portable-ripr-consumer/run.py', packet / 'run.py')
script_sha = sha((packet / 'run.py').read_bytes())
packet_sha = sha('\n'.join(sorted([binary_name + '=' + payload_sha,
                                   'run.py=' + script_sha])).encode())
host_platform = sys.argv[3]
manifest = {
    'schema': 'ripr.portable_consumer.packet/v1', 'packet_version': '1',
    'packet_digest': packet_sha,
    'native_payload': {'relative_path': binary_name, 'sha256': payload_sha,
                       'version': 'test-payload', 'build_identity': payload_sha,
                       'source_route': 'explicit-binary', 'platform': host_platform},
    'consumer': {'script': 'run.py', 'sha256': script_sha, 'python_requires': '>=3.11'},
    'operations': ['check'], 'timeout_seconds_default': 60,
}
(packet / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
subject_sha = digest(subject)
foreign = root / 'foreign cwd with spaces'
foreign.mkdir()
receipts = []

def consume(location, label, forward=False, success=True):
    packet = location / 'packet files'
    subject = location / 'subject files'
    out = root / ('receipt output ' + label)
    def argument(path):
        value = str(path)
        return value.replace('\\', '/') if forward else value
    args = [sys.executable, argument(packet / 'run.py'), '--packet', argument(packet),
            '--subject-root', argument(subject), '--out', argument(out),
            '--diff', argument(location / 'change with spaces.diff'),
            '--foreign-cwd', argument(foreign), '--subject-digest', subject_sha,
            '--require-nonzero-subjects']
    consumer_env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1')
    if os.name == 'nt':
        # Python falls back to this standard variable when WMI is unavailable.
        architecture = {'windows-x86_64': 'AMD64', 'windows-aarch64': 'ARM64'}[host_platform]
        consumer_env['PROCESSOR_ARCHITECTURE'] = consumer_env.get('PROCESSOR_ARCHITECTURE') or architecture
    result = subprocess.run(args, cwd=foreign, capture_output=True, timeout=90,
                            env=consumer_env)
    receipt_file = out / 'packet-consumption-receipt.json'
    assert receipt_file.is_file(), (result.returncode, result.stderr)
    receipt = json.loads(receipt_file.read_text(encoding='utf-8'))
    assert receipt['schema'] == 'ripr.portable_consumer.receipt/v1', receipt
    assert receipt['binary']['build_identity'] == payload_sha, receipt
    assert receipt['binary']['source_route'] == 'explicit-binary', receipt
    assert receipt['binary']['sha256'] == payload_sha, receipt
    assert receipt['packet']['packet_digest'] == packet_sha, receipt
    assert receipt['consumer']['sha256'] == script_sha, receipt
    assert pathlib.Path(receipt['subject']['root']) == subject.resolve(), receipt
    assert pathlib.Path(receipt['packet']['root']) == packet.resolve(), receipt
    if success:
        assert result.returncode == 0 and receipt['classification'] == 'complete', receipt
        assert receipt['selected_subject_count'] > 0, receipt
        assert receipt['subject']['digest'] == subject_sha, receipt
        assert receipt['exit_code'] == 0, receipt
        assert pathlib.Path(receipt['cwd']) == foreign.resolve(), receipt
        assert pathlib.Path(receipt['binary']['path']) == (packet / binary_name).resolve(), receipt
        assert receipt['argv'][0] == 'check', receipt
        assert pathlib.Path(receipt['argv'][receipt['argv'].index('--root') + 1]) == subject.resolve(), receipt
        for name in ('stdout', 'stderr'):
            assert receipt[name + '_digest'] == sha((out / (name + '.bin')).read_bytes()), receipt
    else:
        assert result.returncode != 0 and receipt['classification'] == 'subject_identity_drift', receipt
        assert receipt['exit_code'] is None and receipt['argv'] == [], receipt
        assert receipt['stdout_digest'] is None and receipt['stderr_digest'] is None, receipt
        assert receipt['subject']['digest'] != subject_sha, receipt
    receipts.append({'label': label, 'receipt': str(receipt_file),
                     'sha256': sha(receipt_file.read_bytes()), 'exit': result.returncode})

consume(original, 'native separators')
if os.name == 'nt':
    consume(original, 'forward separators', forward=True)
relocated = root / 'relocated directory with spaces'
shutil.copytree(original, relocated)
assert digest(relocated / 'subject files') == subject_sha
assert (relocated / 'packet files/manifest.json').read_bytes() == (packet / 'manifest.json').read_bytes()
consume(relocated, 'relocated native separators')
# The old path must not supply evidence for the relocated, changed subject.
changed = relocated / 'subject files/src/lib.rs'
changed.write_bytes(changed.read_bytes() + b'\n// identity drift control\n')
consume(relocated, 'changed subject refused', success=False)
assert digest(subject) == subject_sha
consume(original, 'original subject remains valid')
print(json.dumps({'payload_sha256': payload_sha, 'consumer_sha256': script_sha,
                  'packet_digest': packet_sha, 'subject_digest': subject_sha,
                  'platform': host_platform, 'receipts': receipts}, indent=2))
if temporary:
    temporary.cleanup()
"#;

#[test]
fn native_receipt_paths_preserve_identity_across_spaces_and_relocation() -> Result<(), String> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("workspace root: {error}"))?;
    let python = ["python3", "python"]
        .into_iter()
        .find_map(|candidate| {
            Command::new(candidate)
                .args([
                    "-c",
                    "import sys; assert sys.version_info >= (3,11); print(sys.executable)",
                ])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|path| path.trim().to_string())
        })
        .ok_or("Python >=3.11 is required for the native portable-consumer journey")?;
    let output = Command::new(python)
        .args(["-c", JOURNEY])
        .arg(workspace)
        .arg(env!("CARGO_BIN_EXE_ripr"))
        .arg(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .map_err(|error| format!("portable receipt journey: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "journey {}: stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
