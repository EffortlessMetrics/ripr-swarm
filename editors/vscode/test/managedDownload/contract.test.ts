import * as assert from 'assert';
import * as fs from 'fs';
import { test } from 'node:test';
import { admitSubject, CASES, digest, retainHostCase, servedBytes, TARGET, checkHostSocketPath, withHostUserData } from './contract';

// Explicit receipt fixtures exercise admission only, not archive qualification.
function fixture() {
  const archive = Buffer.from('fixture archive bytes');
  const source = 'a'.repeat(40);
  const tree = 'b'.repeat(40);
  const file = `ripr-server-v0.11.0-${TARGET}.tar.gz`;
  const manifest = Buffer.from(JSON.stringify({ schema_version: '2', product_version: '0.11.0',
    distribution_generation: 'd'.repeat(64), source_repository: 'EffortlessMetrics/ripr-swarm',
    build_identity: { candidate_sha: source, candidate_tree: tree, repository: 'EffortlessMetrics/ripr-swarm' },
    target_set: { targets: [TARGET], digest: 'e'.repeat(64) }, assets: {
      [TARGET]: { subject: file, archive_format: 'tar.gz', archive_size: archive.length, sha256: digest(archive),
        executable: { path: 'ripr', size: 12, sha256: 'c'.repeat(64) } }
    } }));
  const receipt = { schema_version: 4, status: 'qualified_payload_and_archive_shape', candidate_sha: source,
    version: '0.11.0', manifest_sha256: digest(manifest), archive_readback_verified: true,
    build_identities: [{ target: TARGET, version: '0.11.0', archive_size_bytes: archive.length,
      producer_executable_sha256: 'c'.repeat(64), archive_file: file, candidate_sha: source, candidate_tree: tree,
      archive_sha256: digest(archive), archive_readback_verified: true }],
    payload_identities: [{ target: TARGET, version: '0.11.0', source: { commit_sha: source, tree_sha: tree },
      files: [{ path: 'ripr', role: 'executable', size_bytes: 12, sha256: 'c'.repeat(64) }] }] };
  return { archive, source, tree, manifest, receipt };
}

test('admits exactly the receipted source, archive and executable identities', () => {
  const f = fixture();
  assert.deepStrictEqual(admitSubject(f.manifest, f.archive, f.receipt, f.source, f.tree), {
    version: '0.11.0', sourceRepository: 'EffortlessMetrics/ripr-swarm', archiveFile: `ripr-server-v0.11.0-${TARGET}.tar.gz`,
    archiveSha256: digest(f.archive), executableSha256: 'c'.repeat(64)
  });
});

test('refuses rehashed wrong manifest subjects, archive and executable identities', () => {
  const f = fixture();
  for (const patch of [{ subject: 'other.tar.gz' }, { subject: undefined, url: 'https://example.invalid/archive' },
    { archive_format: 'zip' }, { archive_size: f.archive.length + 1 }, { sha256: '0'.repeat(64) },
    ...[{ path: 'other' }, { size: 13 }, { sha256: '0'.repeat(64) }].map((change) =>
      ({ executable: { path: 'ripr', size: 12, sha256: 'c'.repeat(64), ...change } }))]) {
    const parsed = JSON.parse(f.manifest.toString());
    Object.assign(parsed.assets[TARGET], patch);
    const bytes = Buffer.from(JSON.stringify(parsed));
    assert.throws(() => admitSubject(bytes, f.archive, { ...f.receipt, manifest_sha256: digest(bytes) }, f.source, f.tree));
  }
});

test('refuses nearby source and tree, unqualified receipt, and missing readback', () => {
  const f = fixture();
  assert.throws(() => admitSubject(f.manifest, f.archive, f.receipt, 'd'.repeat(40), f.tree));
  assert.throws(() => admitSubject(f.manifest, f.archive, f.receipt, f.source, 'd'.repeat(40)));
  for (const patch of [{ candidate_sha: 'd'.repeat(40) }, { candidate_tree: 'd'.repeat(40) }, { repository: 'other/repo' }]) {
    const parsed = JSON.parse(f.manifest.toString());
    Object.assign(parsed.build_identity, patch);
    const bytes = Buffer.from(JSON.stringify(parsed));
    assert.throws(() => admitSubject(bytes, f.archive, { ...f.receipt, manifest_sha256: digest(bytes) }, f.source, f.tree));
  }
  for (const patch of [{ status: 'not_qualified' }, { archive_readback_verified: false }]) {
    assert.throws(() => admitSubject(f.manifest, f.archive, { ...f.receipt, ...patch }, f.source, f.tree));
  }
});

test('refuses changed raw manifest even when parsed JSON is identical, and changed archive bytes', () => {
  const f = fixture();
  const manifest = Buffer.concat([f.manifest, Buffer.from(' ')]);
  assert.deepStrictEqual(JSON.parse(manifest.toString()), JSON.parse(f.manifest.toString()));
  assert.throws(() => admitSubject(manifest, f.archive, f.receipt, f.source, f.tree));
  assert.throws(() => admitSubject(f.manifest, Buffer.concat([f.archive, Buffer.from('x')]), f.receipt, f.source, f.tree));
});

test('refuses duplicate target identity and missing executable identity', () => {
  const f = fixture();
  f.receipt.build_identities.push(f.receipt.build_identities[0]);
  assert.throws(() => admitSubject(f.manifest, f.archive, f.receipt, f.source, f.tree));
  f.receipt.build_identities.pop();
  f.receipt.payload_identities[0].files = [];
  assert.throws(() => admitSubject(f.manifest, f.archive, f.receipt, f.source, f.tree));
});

test('tamper controls change only their selected byte subject and preserve manifest parseability', () => {
  const f = fixture();
  assert.strictEqual(CASES.length, 3);
  const valid = servedBytes('valid', f.manifest, f.archive);
  assert.deepStrictEqual(valid, { manifest: f.manifest, archive: f.archive });
  const manifest = servedBytes('manifest_tampered', f.manifest, f.archive);
  assert.notStrictEqual(digest(manifest.manifest), digest(f.manifest));
  assert.deepStrictEqual(JSON.parse(manifest.manifest.toString()), JSON.parse(f.manifest.toString()));
  assert.deepStrictEqual(manifest.archive, f.archive);
  const archive = servedBytes('archive_tampered', f.manifest, f.archive);
  assert.deepStrictEqual(archive.manifest, f.manifest);
  assert.strictEqual(archive.archive.length, f.archive.length);
  assert.notStrictEqual(digest(archive.archive), digest(f.archive));
});

test('retains failed host execution separately from success and unstarted cases', async () => {
  const records = CASES.map((kind) => ({ case: kind, attempted: false, executed: 0, outcome: 'not_run' }));
  const host = { case: 'valid', selected: 1, executed: 1, outcome: 'failure', error: 'intentional host assertion' };
  await assert.rejects(retainHostCase(records[0], async () => {
    throw new Error(host.error);
  }, () => host), /intentional host assertion/);
  assert.strictEqual(records[0].executed, 1);
  assert.strictEqual(records[0].outcome, 'failure');
  assert.strictEqual(records[0].attempted, true);
  assert.deepStrictEqual((records[0] as any).host, host);
  assert.strictEqual(records[1].executed, 0);
  assert.strictEqual(records[1].outcome, 'not_run');
});

test('hosted IPC length is measured in bytes at the Linux pathname boundary', { skip: process.platform !== 'linux' }, () => {
  const root = '/opt/ci-host/work/ripr-swarm/ripr-swarm/target/ripr/managed-download/run-X0p2x0';
  checkHostSocketPath(root + '/valid/user');
  for (const kind of ['manifest_tampered', 'archive_tampered']) {
    assert.throws(() => checkHostSocketPath(root + '/' + kind + '/user'), /exceeds 107 bytes/);
  }
  const exact = '/' + 'a'.repeat(91);
  checkHostSocketPath(exact);
  assert.throws(() => checkHostSocketPath(exact + 'a'), /exceeds 107 bytes/);
  assert.throws(() => checkHostSocketPath('/' + 'é'.repeat(46)), /exceeds 107 bytes/);
});

test('each host gets a private short profile that is removed on success or failure', { skip: process.platform !== 'linux' }, async () => {
  const profiles: string[] = [];
  for (const kind of CASES) {
    const run = withHostUserData(async (userData) => {
      assert.ok(!profiles.includes(userData));
      profiles.push(userData);
      assert.strictEqual(fs.statSync(userData).mode & 0o777, 0o700);
      checkHostSocketPath(userData);
      if (kind === 'manifest_tampered') { throw new Error('host failed before activation'); }
    });
    if (kind === 'manifest_tampered') { await assert.rejects(run, /before activation/); }
    else { await run; }
    assert.ok(!fs.existsSync(profiles[profiles.length - 1]));
  }
  assert.strictEqual(profiles.length, CASES.length);
});
