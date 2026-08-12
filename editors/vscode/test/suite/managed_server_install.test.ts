import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  installManagedServer,
  ManagedServerInstallOperations,
  ManagedServerInstallRequest,
  readManagedServerInstallation
} from '../../src/managedServerInstall';

suite('Managed Server Installation', () => {
  let root: string;

  setup(async () => {
    root = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ripr-managed-install-'));
  });

  teardown(async () => {
    await fs.promises.rm(root, { recursive: true, force: true });
  });

  test('cache admission requires a matching complete receipt and executable digest', async () => {
    const request = installRequest(root, '1.2.3');
    const finalDir = path.join(request.serversRoot, request.version, request.platformTarget);
    await fs.promises.mkdir(finalDir, { recursive: true });
    await fs.promises.writeFile(path.join(finalDir, request.executableName), 'partial binary');

    assert.strictEqual(await readManagedServerInstallation(request), undefined);

    const installed = await installManagedServer(request, operations('binary-v1', '1.2.3'));
    assert.strictEqual(installed.receipt.installationState, 'complete');
    assert.strictEqual(installed.receipt.requestedVersion, '1.2.3');
    assert.strictEqual(await fs.promises.readFile(installed.executablePath, 'utf8'), 'binary-v1');

    await fs.promises.writeFile(installed.executablePath, 'tampered');
    assert.strictEqual(await readManagedServerInstallation(request), undefined);
  });

  test('concurrent installs stage once and converge on one completed installation', async () => {
    const request = installRequest(root, '2.0.0');
    let extractCalls = 0;
    const shared = operations('converged', '2.0.0', async () => {
      extractCalls += 1;
      await new Promise((resolve) => setTimeout(resolve, 100));
    });

    const [first, second] = await Promise.all([
      installManagedServer(request, shared),
      installManagedServer(request, shared)
    ]);

    assert.strictEqual(extractCalls, 1);
    assert.strictEqual(first.executablePath, second.executablePath);
    assert.deepStrictEqual(first.receipt, second.receipt);
    const siblings = await fs.promises.readdir(path.join(request.serversRoot, request.version));
    assert.deepStrictEqual(siblings, [request.platformTarget]);
  });

  test('failed upgrade leaves no eligible partial cache and preserves the prior usable version', async () => {
    const priorRequest = installRequest(root, '3.0.0');
    const prior = await installManagedServer(priorRequest, operations('prior-known-good', '3.0.0'));
    const upgradeRequest = installRequest(root, '3.1.0');
    const failing = operations('upgrade', '3.1.0');
    failing.probeExecutable = async () => {
      throw new Error('probe rejected upgrade');
    };

    await assert.rejects(installManagedServer(upgradeRequest, failing), /probe rejected upgrade/);

    const retained = await readManagedServerInstallation(priorRequest);
    assert.ok(retained);
    assert.strictEqual(retained.executablePath, prior.executablePath);
    assert.strictEqual(await fs.promises.readFile(retained.executablePath, 'utf8'), 'prior-known-good');
    assert.strictEqual(await readManagedServerInstallation(upgradeRequest), undefined);
    const versionEntries = await fs.promises.readdir(path.join(upgradeRequest.serversRoot, upgradeRequest.version));
    assert.deepStrictEqual(versionEntries, []);
  });
});

function installRequest(root: string, version: string): ManagedServerInstallRequest {
  return {
    serversRoot: path.join(root, 'servers'),
    version,
    platformTarget: 'test-target',
    executableName: process.platform === 'win32' ? 'ripr.exe' : 'ripr',
    archiveExtension: 'test'
  };
}

function operations(
  executableContents: string,
  version: string,
  beforeExtract?: () => Promise<void>
): ManagedServerInstallOperations {
  const bytes = Buffer.from(`archive:${executableContents}`);
  return {
    resolveArchive: async () => ({
      manifestVersion: version,
      expectedSha256: crypto.createHash('sha256').update(bytes).digest('hex'),
      bytes
    }),
    extractArchive: async (_archivePath, destination) => {
      await beforeExtract?.();
      const executableName = process.platform === 'win32' ? 'ripr.exe' : 'ripr';
      await fs.promises.writeFile(path.join(destination, executableName), executableContents);
    },
    probeExecutable: async () => 'ripr 1.2.3'
  };
}
