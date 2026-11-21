import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import { admitManifestBytes } from '../../src/manifestTrust';
import { createHash } from 'crypto';

export const TARGET = 'x86_64-unknown-linux-gnu';
export const CASES = ['valid', 'manifest_tampered', 'archive_tampered'] as const;
export type DownloadCase = typeof CASES[number];
export const digest = (bytes: Buffer): string => createHash('sha256').update(bytes).digest('hex');

// On Linux VS Code builds its main IPC socket under --user-data-dir. The
// kernel allows 107 pathname bytes plus the trailing NUL in sockaddr_un.
export function checkHostSocketPath(userData: string): void {
  const socket = path.join(userData, '1.14-main.sock');
  assert.ok(Buffer.byteLength(socket, 'utf8') <= 107, `VS Code IPC path exceeds 107 bytes: ${socket}`);
}

export async function withHostUserData(action: (userData: string) => Promise<void>): Promise<void> {
  // The rehearsal is Linux-only. A shared temp parent is safe here because
  // mkdtemp atomically creates a new private (0700) directory for every case.
  const userData = fs.mkdtempSync('/tmp/ripr-vsc-');
  try {
    checkHostSocketPath(userData);
    await action(userData);
  } finally {
    fs.rmSync(userData, { recursive: true });
  }
}

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
  sourceRepository: string;
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
  const manifest = admitManifestBytes(manifestBytes, receipt.manifest_sha256);
  // The host's parser returns runtime fields. The rehearsal also binds the
  // producer's source coordinates to this exact checkout and qualification.
  const provenance = JSON.parse(manifestBytes.toString('utf8')).build_identity;
  assert.strictEqual(provenance.candidate_sha, source);
  assert.strictEqual(provenance.candidate_tree, tree);
  assert.strictEqual(provenance.repository, manifest.sourceRepository);
  const version = manifest.productVersion;
  assert.match(version, /^\d+\.\d+\.\d+$/);
  assert.strictEqual(receipt.version, version);
  const asset = manifest.assets[TARGET];
  assert.ok(asset, `missing qualified platform ${TARGET}`);
  const builds = receipt.build_identities.filter((row: any) => row.target === TARGET);
  const payloads = receipt.payload_identities.filter((row: any) => row.target === TARGET);
  assert.strictEqual(builds.length, 1);
  assert.strictEqual(payloads.length, 1);
  const build = builds[0];
  const payload = payloads[0];
  const archiveFile = `ripr-server-v${version}-${TARGET}.tar.gz`;
  assert.strictEqual(asset.subject, archiveFile);
  assert.strictEqual(asset.archiveFormat, 'tar.gz');
  assert.strictEqual(asset.archiveSize, archive.length);
  assert.strictEqual(build.version, version);
  assert.strictEqual(build.archive_size_bytes, archive.length);
  assert.strictEqual(build.archive_file, archiveFile);
  assert.strictEqual(build.candidate_sha, source);
  assert.strictEqual(build.candidate_tree, tree);
  assert.strictEqual(build.archive_readback_verified, true);
  assert.strictEqual(build.archive_sha256, asset.sha256);
  assert.strictEqual(asset.sha256, digest(archive));
  assert.strictEqual(payload.source.commit_sha, source);
  assert.strictEqual(payload.source.tree_sha, tree);
  assert.strictEqual(payload.version, version);
  const executables = payload.files.filter((row: any) => row.role === 'executable' && row.path === 'ripr');
  assert.strictEqual(executables.length, 1);
  assert.match(executables[0].sha256, /^[0-9a-f]{64}$/);
  assert.strictEqual(asset.executable.path, executables[0].path);
  assert.strictEqual(asset.executable.size, executables[0].size_bytes);
  assert.strictEqual(asset.executable.sha256, executables[0].sha256);
  assert.strictEqual(build.producer_executable_sha256, asset.executable.sha256);
  return { version, sourceRepository: manifest.sourceRepository, archiveFile, archiveSha256: asset.sha256,
    executableSha256: executables[0].sha256 };
}

export function servedBytes(kind: DownloadCase, manifest: Buffer, archive: Buffer): { manifest: Buffer; archive: Buffer } {
  // Whitespace preserves parsed JSON, so the first control specifically
  // challenges raw-byte admission rather than JSON parsing.
  return {
    manifest: kind === 'manifest_tampered' ? Buffer.concat([manifest, Buffer.from(' ')]) : manifest,
    archive: kind === 'archive_tampered' ? Buffer.from(archive).fill(archive[0] ^ 1, 0, 1) : archive
  };
}
