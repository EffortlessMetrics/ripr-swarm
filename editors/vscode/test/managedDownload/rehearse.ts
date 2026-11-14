import * as assert from 'assert';
import { execFileSync } from 'child_process';
import * as fs from 'fs';
import * as https from 'https';
import * as path from 'path';
import { downloadAndUnzipVSCode, resolveCliArgsFromVSCodeExecutablePath, runTests } from '@vscode/test-electron';
import { admitSubject, CASES, digest, retainHostCase, servedBytes, TARGET } from './contract';

const repo = path.resolve(__dirname, '../../../../..');
const editor = path.join(repo, 'editors/vscode');

function command(program: string, args: string[], cwd = repo, env = process.env): string {
  return execFileSync(program, args, { cwd, env, encoding: 'utf8', timeout: 600_000,
    maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
}

async function main(): Promise<void> {
  assert.strictEqual(process.platform, 'linux', 'this rehearsal currently selects Linux x64 only');
  assert.strictEqual(process.arch, 'x64');
  assert.notStrictEqual(process.env.NODE_TLS_REJECT_UNAUTHORIZED, '0');
  const source = command('git', ['rev-parse', 'HEAD']).trim();
  const tree = command('git', ['rev-parse', 'HEAD^{tree}']).trim();
  const dist = path.resolve(process.env.RIPR_MANAGED_DOWNLOAD_DIST ?? path.join(repo, 'dist'));
  const qualificationBytes = fs.readFileSync(path.join(dist, 'server-archive-qualification-receipt.json'));
  const qualification = JSON.parse(qualificationBytes.toString('utf8'));
  assert.match(qualification.version, /^\d+\.\d+\.\d+$/);
  const manifestName = `ripr-server-manifest-v${qualification.version}.json`;
  const originalManifest = fs.readFileSync(path.join(dist, manifestName));
  const archiveName = `ripr-server-v${qualification.version}-${TARGET}.tar.gz`;
  const archive = fs.readFileSync(path.join(dist, archiveName));
  const subject = admitSubject(originalManifest, archive, qualification, source, tree);
  const output = path.resolve(process.env.RIPR_MANAGED_DOWNLOAD_OUTPUT ??
    path.join(repo, 'target/ripr/managed-download'));
  fs.mkdirSync(output, { recursive: true });
  // Never reset a previous run's profile/cache or evidence.
  const root = fs.mkdtempSync(path.join(output, 'run-'));
  const descriptorPath = path.join(editor, 'src/serverDescriptor.ts');
  const descriptor = fs.readFileSync(descriptorPath);
  const compiledDescriptors = ['serverDescriptor.js', 'serverDescriptor.js.map'].map((name) => {
    const file = path.join(editor, 'out/src', name);
    return { file, bytes: fs.existsSync(file) ? fs.readFileSync(file) : undefined };
  });
  const builtVsix = path.join(editor, 'dist', `ripr-${subject.version}.vsix`);
  const originalVsix = fs.existsSync(builtVsix) ? fs.readFileSync(builtVsix) : undefined;
  const key = path.join(root, 'key.pem');
  const cert = path.join(root, 'cert.pem');
  let manifestBytes: Buffer = Buffer.alloc(0);
  let archiveBytes: Buffer = archive;
  let manifestRequests = 0;
  let archiveRequests = 0;
  let server: https.Server | undefined;
  const receipt: any = { schema_version: 1, kind: 'packaged_managed_download_rehearsal',
    source, tree, qualification_receipt_sha256: digest(qualificationBytes),
    original_manifest_sha256: digest(originalManifest), ...subject,
    selected: CASES.length, executed: 0, outcome: 'failure',
    cases: CASES.map((kind) => ({ case: kind, attempted: false, executed: 0, outcome: 'not_run' })),
    release_qualification_claimed: false, publication_performed: false };
  try {
    command('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-sha256', '-days', '1', '-nodes',
      '-keyout', key, '-out', cert, '-subj', '/CN=127.0.0.1', '-addext', 'subjectAltName=IP:127.0.0.1']);
    fs.chmodSync(key, 0o600);
    server = https.createServer({ key: fs.readFileSync(key), cert: fs.readFileSync(cert) }, (request, response) => {
      if (request.url === `/${manifestName}`) {
        manifestRequests += 1;
        response.end(manifestBytes);
      } else if (request.url === `/${archiveName}`) {
        archiveRequests += 1;
        response.end(archiveBytes);
      } else {
        response.writeHead(404).end();
      }
    });
    await new Promise<void>((resolve, reject) => {
      server!.once('error', reject);
      server!.listen(0, '127.0.0.1', resolve);
    });
    const address = server.address();
    assert.ok(address && typeof address !== 'string');
    const mirror = `https://127.0.0.1:${address.port}`;
    // Keep the qualified archive bytes/digest. Only placement URLs change in
    // this separate local-mirror manifest, before raw-byte descriptor binding.
    const mirrorManifest = JSON.parse(originalManifest.toString('utf8'));
    for (const asset of Object.values(mirrorManifest.assets) as Array<{ url: string }>) {
      asset.url = `${mirror}/${new URL(asset.url).pathname.split('/').pop()}`;
    }
    const admittedManifest = Buffer.from(`${JSON.stringify(mirrorManifest, null, 2)}\n`);
    const manifestPath = path.join(root, manifestName);
    fs.writeFileSync(manifestPath, admittedManifest);
    const manifestSha256 = digest(admittedManifest);
    receipt.mirror_manifest_sha256 = manifestSha256;
    command('cargo', ['xtask', 'release-bind-server-descriptor', '--version', subject.version,
      '--manifest', manifestPath, '--manifest-sha256', manifestSha256]);
    command('npm', ['run', 'compile'], editor);
    command('npm', ['run', 'package'], editor);
    command('cargo', ['xtask', 'vscode-verify-vsix', '--', builtVsix]);
    const vsix = path.join(root, `ripr-${subject.version}.vsix`);
    fs.copyFileSync(builtVsix, vsix, fs.constants.COPYFILE_EXCL);
    const vsixSha256 = digest(fs.readFileSync(vsix));
    receipt.vsix_sha256 = vsixSha256;
    const vscodeExecutablePath = await downloadAndUnzipVSCode({ cachePath: path.join(output, 'vscode-cache') });
    const [cli, ...cliArgs] = resolveCliArgsFromVSCodeExecutablePath(vscodeExecutablePath, { reuseMachineInstall: true });
    const environment = { ...process.env, NODE_EXTRA_CA_CERTS: cert,
      RIPR_TEST_SERVER_PATH: undefined, RIPR_TEST_PACKAGED_SERVER_PATH: undefined };
    for (const kind of CASES) {
      const before = { manifest: manifestRequests, archive: archiveRequests };
      const caseRecord = receipt.cases.find((row: any) => row.case === kind);
      const profile = path.join(root, kind);
      const receiptPath = path.join(profile, 'host.json');
      try {
        await retainHostCase(caseRecord, async () => {
          const served = servedBytes(kind, admittedManifest, archive);
          manifestBytes = served.manifest;
          archiveBytes = served.archive;
          const userData = path.join(profile, 'user');
          const extensions = path.join(profile, 'extensions');
          const workspace = path.join(profile, 'workspace');
          fs.mkdirSync(path.join(userData, 'User'), { recursive: true });
          fs.mkdirSync(extensions);
          fs.mkdirSync(workspace);
          fs.writeFileSync(path.join(workspace, 'Cargo.toml'), '[package]\nname="managed-download-fixture"\nversion="0.0.0"\nedition="2024"\n');
          fs.mkdirSync(path.join(workspace, 'src'));
          fs.writeFileSync(path.join(workspace, 'src/lib.rs'), 'pub fn answer() -> u32 { 42 }\n');
          fs.writeFileSync(path.join(userData, 'User/settings.json'), JSON.stringify({
            'ripr.server.path': '', 'ripr.server.autoDownload': true,
            'ripr.server.version': subject.version, 'ripr.server.downloadBaseUrl': mirror
          }));
          command(cli, [...cliArgs, '--no-sandbox', '--user-data-dir', userData,
            '--extensions-dir', extensions, '--install-extension', vsix], repo, environment);
          const installed = fs.readdirSync(extensions, { withFileTypes: true })
            .filter((entry) => entry.isDirectory() && entry.name === `effortlessmetrics.ripr-${subject.version}`);
          assert.strictEqual(installed.length, 1, 'CLI must install the actual VSIX');
          const installedExtension = path.join(extensions, installed[0].name);
          assert.ok(fs.readFileSync(path.join(installedExtension, 'out/src/serverDescriptor.js'), 'utf8').includes(manifestSha256));
          const inputPath = path.join(profile, 'input.json');
          fs.writeFileSync(inputPath, JSON.stringify({ kind, ...subject, manifestSha256, mirror,
            userData, installedExtension, receiptPath }));
          await runTests({ vscodeExecutablePath, reuseMachineInstall: true, extensionDevelopmentPath: installedExtension,
            extensionTestsPath: path.join(__dirname, 'host.js'),
            extensionTestsEnv: { ...environment, RIPR_MANAGED_DOWNLOAD_INPUT: inputPath },
            // This switch affects only the new disposable test profile, exactly as
            // runTest.ts's trusted host. User profiles and trust state are untouched.
            launchArgs: ['--no-sandbox', '--disable-gpu-sandbox', '--disable-workspace-trust',
              '--skip-welcome', '--skip-release-notes', '--disable-updates',
              '--user-data-dir', userData, '--extensions-dir', extensions, workspace] });
          const observed = JSON.parse(fs.readFileSync(receiptPath, 'utf8'));
          assert.strictEqual(observed.case, kind);
          assert.strictEqual(observed.selected, 1);
          assert.strictEqual(observed.executed, 1);
          assert.strictEqual(observed.outcome, 'success');
          assert.strictEqual(manifestRequests - before.manifest, 1);
          assert.strictEqual(archiveRequests - before.archive, kind === 'manifest_tampered' ? 0 : 1);
          assert.strictEqual(digest(fs.readFileSync(vsix)), vsixSha256);
        }, () => fs.existsSync(receiptPath) ? JSON.parse(fs.readFileSync(receiptPath, 'utf8')) : undefined);
      } finally {
        receipt.executed += caseRecord.executed;
        caseRecord.manifest_requests = manifestRequests - before.manifest;
        caseRecord.archive_requests = archiveRequests - before.archive;
      }
    }
    receipt.outcome = 'success';
  } finally {
    fs.writeFileSync(descriptorPath, descriptor);
    for (const saved of compiledDescriptors) {
      if (saved.bytes) { fs.writeFileSync(saved.file, saved.bytes); }
      else { fs.rmSync(saved.file, { force: true }); }
    }
    if (originalVsix) { fs.writeFileSync(builtVsix, originalVsix); }
    else { fs.rmSync(builtVsix, { force: true }); }
    if (server) {
      await new Promise<void>((resolve, reject) => {
        server!.close((error) => error ? reject(error) : resolve());
        server!.closeAllConnections();
      });
    }
    // Never retain the disposable CA private key in uploaded evidence.
    fs.rmSync(key, { force: true });
    fs.writeFileSync(path.join(root, 'rehearsal.json'), `${JSON.stringify(receipt, null, 2)}\n`);
    console.log(`Managed download rehearsal evidence: ${root}`);
  }
}

if (require.main === module) {
  main().catch((error) => { console.error(error); process.exitCode = 1; });
}
