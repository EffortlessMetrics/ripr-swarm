import * as assert from 'assert';
import { createHash } from 'crypto';

export const TARGET = 'x86_64-unknown-linux-gnu';
export const CASES = ['valid', 'manifest_tampered', 'archive_tampered'] as const;
export type DownloadCase = typeof CASES[number];
export const digest = (bytes: Buffer): string => createHash('sha256').update(bytes).digest('hex');

export async function retainHostCase(record: any, action: () => Promise<void>, readHost: () => any): Promise<void> {
  record.attempted = true;
  try {
    await action();
    record.outcome = 'success';
  } catch (error) {
    record.outcome = 'failure';
    record.error = String(error);
    throw error;
  } finally {
    const host = readHost();
    if (host !== undefined) {
      record.host = host;
      assert.strictEqual(host.case, record.case);
      assert.strictEqual(host.selected, 1);
      assert.strictEqual(host.executed, 1);
      record.executed = host.executed;
    }
  }
}

export interface RehearsalSubject {
  version: string;
  archiveFile: string;
  archiveSha256: string;
  executableSha256: string;
}

// These are existing archive-workflow receipts, not a second qualification
// authority. Require their exact source and byte identities before rehearsal.
export function admitSubject(manifestBytes: Buffer, archive: Buffer, receipt: any,
  source: string, tree: string): RehearsalSubject {
  assert.strictEqual(receipt.schema_version, 4);
  assert.strictEqual(receipt.status, 'qualified_payload_and_archive_shape');
  assert.strictEqual(receipt.candidate_sha, source);
  assert.strictEqual(receipt.manifest_sha256, digest(manifestBytes));
  assert.strictEqual(receipt.archive_readback_verified, true);
  const manifest = JSON.parse(manifestBytes.toString('utf8'));
  assert.match(manifest.version, /^\d+\.\d+\.\d+$/);
  assert.strictEqual(receipt.version, manifest.version);
  const builds = receipt.build_identities.filter((row: any) => row.target === TARGET);
  const payloads = receipt.payload_identities.filter((row: any) => row.target === TARGET);
  assert.strictEqual(builds.length, 1);
  assert.strictEqual(payloads.length, 1);
  const build = builds[0];
  const payload = payloads[0];
  const archiveFile = `ripr-server-v${manifest.version}-${TARGET}.tar.gz`;
  assert.strictEqual(new URL(manifest.assets[TARGET].url).pathname.split('/').pop(), archiveFile);
  assert.strictEqual(build.archive_file, archiveFile);
  assert.strictEqual(build.candidate_sha, source);
  assert.strictEqual(build.candidate_tree, tree);
  assert.strictEqual(build.archive_readback_verified, true);
  assert.strictEqual(build.archive_sha256, digest(archive));
  assert.strictEqual(manifest.assets[TARGET].sha256, digest(archive));
  assert.strictEqual(payload.source.commit_sha, source);
  assert.strictEqual(payload.source.tree_sha, tree);
  assert.strictEqual(payload.version, manifest.version);
  const executables = payload.files.filter((row: any) => row.role === 'executable' && row.path === 'ripr');
  assert.strictEqual(executables.length, 1);
  assert.match(executables[0].sha256, /^[0-9a-f]{64}$/);
  return { version: manifest.version, archiveFile, archiveSha256: digest(archive),
    executableSha256: executables[0].sha256 };
}

export function servedBytes(kind: DownloadCase, manifest: Buffer, archive: Buffer): { manifest: Buffer; archive: Buffer } {
  // Whitespace preserves parsed JSON, so the first control specifically
  // challenges raw-byte admission rather than JSON parsing.
  return {
    manifest: kind === 'manifest_tampered' ? Buffer.concat([manifest, Buffer.from(' ')]) : manifest,
    archive: kind === 'archive_tampered' ? Buffer.concat([archive, Buffer.from('tampered')]) : archive
  };
}
