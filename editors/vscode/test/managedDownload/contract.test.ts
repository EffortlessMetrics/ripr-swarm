import * as assert from 'assert';
import { test } from 'node:test';
import { admitSubject, CASES, digest, retainHostCase, servedBytes, TARGET } from './contract';

// Explicit receipt fixtures exercise admission only, not archive qualification.
function fixture() {
  const archive = Buffer.from('fixture archive bytes');
  const source = 'a'.repeat(40);
  const tree = 'b'.repeat(40);
  const file = `ripr-server-v0.11.0-${TARGET}.tar.gz`;
  const manifest = Buffer.from(JSON.stringify({ version: '0.11.0', assets: {
    [TARGET]: { url: `https://example.invalid/${file}`, sha256: digest(archive) }
  } }));
  const receipt = { schema_version: 4, status: 'qualified_payload_and_archive_shape', candidate_sha: source,
    version: '0.11.0', manifest_sha256: digest(manifest), archive_readback_verified: true,
    build_identities: [{ target: TARGET, archive_file: file, candidate_sha: source, candidate_tree: tree,
      archive_sha256: digest(archive), archive_readback_verified: true }],
    payload_identities: [{ target: TARGET, version: '0.11.0', source: { commit_sha: source, tree_sha: tree },
      files: [{ path: 'ripr', role: 'executable', sha256: 'c'.repeat(64) }] }] };
  return { archive, source, tree, manifest, receipt };
}

test('admits exactly the receipted source, archive and executable identities', () => {
  const f = fixture();
  assert.deepStrictEqual(admitSubject(f.manifest, f.archive, f.receipt, f.source, f.tree), {
    version: '0.11.0', archiveFile: `ripr-server-v0.11.0-${TARGET}.tar.gz`,
    archiveSha256: digest(f.archive), executableSha256: 'c'.repeat(64)
  });
});

test('refuses nearby source and tree, unqualified receipt, and missing readback', () => {
  const f = fixture();
  assert.throws(() => admitSubject(f.manifest, f.archive, f.receipt, 'd'.repeat(40), f.tree));
  assert.throws(() => admitSubject(f.manifest, f.archive, f.receipt, f.source, 'd'.repeat(40)));
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
