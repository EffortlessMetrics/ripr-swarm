import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  combineActiveManagedServerIdentity,
  installManagedServer,
  ManagedServerInstallOperations,
  ManagedServerInstallRequest,
  readManagedServerInstallation,
  reclaimStaleLock,
  sweepOrphanInstallDirs
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

  test('rejects traversal and rooted version forms before any filesystem mutation', async () => {
    const invalidVersions = [
      '../escaped',
      '..\\..\\escaped',
      '1.2.3/../../escaped',
      '1.2.3\\..\\escaped',
      '/tmp/escaped',
      '\\server\\share',
      '\\\\server\\share',
      ['C:', '\\escaped'].join(''),
      '01.2.3',
      '.',
      '..'
    ];

    for (const version of invalidVersions) {
      const request = installRequest(root, version);
      await assert.rejects(installManagedServer(request, operations('never', version)), /Invalid ripr server version/);
    }

    assert.strictEqual(fs.existsSync(path.join(root, 'servers')), false);
    assert.strictEqual(fs.existsSync(path.join(root, 'escaped')), false);
    const valid = await installManagedServer(
      installRequest(root, '4.0.0-rc.1+build.7'),
      operations('valid-prerelease', '4.0.0-rc.1+build.7')
    );
    assert.strictEqual(valid.receipt.requestedVersion, '4.0.0-rc.1+build.7');
  });

  test('never reclaims a live fresh lock or a replaced owner', async () => {
    const request = installRequest(root, '5.0.0');
    const versionDir = path.join(request.serversRoot, request.version);
    const finalDir = path.join(versionDir, request.platformTarget);
    const lockPath = `${finalDir}.install.lock`;
    await fs.promises.mkdir(versionDir, { recursive: true });
    await fs.promises.mkdir(lockPath);
    const ownerPath = path.join(lockPath, 'owner');
    await fs.promises.writeFile(ownerPath, `${process.pid}:live-owner\n`);
    let extractionCalls = 0;
    const blocked = operations('must-not-install', '5.0.0');
    blocked.lockWaitMs = 80;
    blocked.lockPollMs = 10;
    blocked.extractArchive = async () => {
      extractionCalls += 1;
    };

    const attempt = installManagedServer(request, blocked);
    await new Promise((resolve) => setTimeout(resolve, 20));
    await fs.promises.writeFile(ownerPath, 'fresh-replacement-owner\n');
    await assert.rejects(attempt, /Timed out waiting for managed server install lock/);

    assert.strictEqual(extractionCalls, 0);
    assert.strictEqual(await fs.promises.readFile(ownerPath, 'utf8'), 'fresh-replacement-owner\n');
    assert.strictEqual(await readManagedServerInstallation(request), undefined);
  });

  test('cache hit is bound to the admitted manifest digest (#9304)', async () => {
    const digestA = 'a'.repeat(64);
    const digestB = 'b'.repeat(64);
    const request = { ...installRequest(root, '7.0.0'), admittedManifestSha256: digestA };
    const installed = await installManagedServer(request, operations('bound', '7.0.0', undefined, undefined, digestA));
    assert.strictEqual(installed.receipt.manifestSha256, digestA);

    // Positive control: same admitted digest hits with zero resolver calls.
    let resolveCalls = 0;
    const counting = operations('bound', '7.0.0', undefined, undefined, digestA);
    const inner = counting.resolveArchive;
    counting.resolveArchive = async () => {
      resolveCalls += 1;
      return inner();
    };
    assert.ok(await readManagedServerInstallation(request));
    await installManagedServer(request, counting);
    assert.strictEqual(resolveCalls, 0);

    // Negative: a different admitted digest misses, and re-install re-resolves.
    const rotated = { ...request, admittedManifestSha256: digestB };
    assert.strictEqual(await readManagedServerInstallation(rotated), undefined);
    const reinstalled = await installManagedServer(rotated, operations('rotated', '7.0.0', undefined, undefined, digestB));
    assert.strictEqual(reinstalled.receipt.manifestSha256, digestB);
    assert.strictEqual(await fs.promises.readFile(reinstalled.executablePath, 'utf8'), 'rotated');
  });

  test('legacy receipt without a manifest digest misses once a digest is admitted, hits when none is (#9304)', async () => {
    const plain = installRequest(root, '7.1.0');
    const installed = await installManagedServer(plain, operations('legacy', '7.1.0'));
    assert.strictEqual(installed.receipt.manifestSha256, undefined);
    assert.ok(await readManagedServerInstallation(plain), 'no admitted digest: explicit offline trust');
    assert.strictEqual(
      await readManagedServerInstallation({ ...plain, admittedManifestSha256: 'c'.repeat(64) }),
      undefined
    );
  });

  test('fresh install rejects a manifest whose digest is not the admitted one (#9304)', async () => {
    const request = { ...installRequest(root, '7.2.0'), admittedManifestSha256: 'a'.repeat(64) };
    await assert.rejects(
      installManagedServer(request, operations('wrong', '7.2.0', undefined, undefined, 'b'.repeat(64))),
      /admitted manifest SHA-256/
    );
    await assert.rejects(
      installManagedServer(request, operations('missing', '7.2.0')),
      /admitted manifest SHA-256/
    );
    assert.strictEqual(await readManagedServerInstallation(request), undefined);
  });

  test('install reclaims a lock left by a dead pid and sweeps orphan directories (#9304)', async () => {
    const request = installRequest(root, '8.0.0');
    const versionDir = path.join(request.serversRoot, request.version);
    const finalDir = path.join(versionDir, request.platformTarget);
    const lockPath = `${finalDir}.install.lock`;
    await fs.promises.mkdir(lockPath, { recursive: true });
    await fs.promises.writeFile(path.join(lockPath, 'owner'), `${deadPid()}:dead\n`);
    const orphanStage = `${finalDir}.install-1-x`;
    const orphanDisplaced = `${finalDir}.replaced-1-x`;
    const unrelated = path.join(versionDir, 'other-target.install-1-x');
    for (const dir of [orphanStage, orphanDisplaced, unrelated]) {
      await fs.promises.mkdir(dir);
    }
    const attempt = operations('after-crash', '8.0.0');
    attempt.lockWaitMs = 2_000;
    const installed = await installManagedServer(request, attempt);
    assert.strictEqual(await fs.promises.readFile(installed.executablePath, 'utf8'), 'after-crash');
    assert.strictEqual(fs.existsSync(orphanStage), false);
    assert.strictEqual(fs.existsSync(orphanDisplaced), false);
    assert.strictEqual(fs.existsSync(unrelated), true, 'other targets are never swept');
    assert.strictEqual(fs.existsSync(finalDir), true, 'active install is never swept');
    assert.strictEqual(fs.existsSync(lockPath), false);
  });

  test('stale heartbeat reclaims a live-pid lock; fresh heartbeat and live pid does not (#9304)', async () => {
    const request = installRequest(root, '8.1.0');
    const versionDir = path.join(request.serversRoot, request.version);
    const lockPath = path.join(versionDir, `${request.platformTarget}.install.lock`);
    await fs.promises.mkdir(lockPath, { recursive: true });
    await fs.promises.writeFile(path.join(lockPath, 'owner'), `${process.pid}:live\n`);

    assert.strictEqual(await reclaimStaleLock(lockPath, request.serversRoot), false);
    assert.ok(fs.existsSync(lockPath));

    const old = new Date(Date.now() - 60 * 60_000);
    await fs.promises.utimes(lockPath, old, old);
    assert.strictEqual(await reclaimStaleLock(lockPath, request.serversRoot), true);
    assert.strictEqual(fs.existsSync(lockPath), false);
    assert.deepStrictEqual(await fs.promises.readdir(versionDir), []);
  });

  test('concurrent contenders over one dead-pid lock install exactly once (#9304)', async () => {
    const request = installRequest(root, '8.2.0');
    const finalDir = path.join(request.serversRoot, request.version, request.platformTarget);
    const lockPath = `${finalDir}.install.lock`;
    await fs.promises.mkdir(lockPath, { recursive: true });
    await fs.promises.writeFile(path.join(lockPath, 'owner'), `${deadPid()}:dead\n`);
    let extractCalls = 0;
    const shared = operations('once', '8.2.0', async () => {
      extractCalls += 1;
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    shared.lockWaitMs = 5_000;
    const results = await Promise.all([1, 2, 3].map(() => installManagedServer(request, shared)));
    assert.strictEqual(extractCalls, 1);
    assert.strictEqual(new Set(results.map((r) => r.executablePath)).size, 1);
  });

  test('orphan sweep removes orphans beside the active directory but never the active one (#9304)', async () => {
    const request = installRequest(root, '8.3.0');
    const installed = await installManagedServer(request, operations('keep', '8.3.0'));
    const finalDir = path.dirname(installed.executablePath);
    const orphan = `${finalDir}.install-9-z`;
    const stale = `${finalDir}.install.lock.stale-9-z`;
    const replaced = `${finalDir}.replaced-9-z`;
    for (const dir of [orphan, stale, replaced]) {
      await fs.promises.mkdir(dir);
    }
    await sweepOrphanInstallDirs(request.serversRoot, finalDir);
    assert.strictEqual(fs.existsSync(orphan), false);
    assert.strictEqual(fs.existsSync(stale), false);
    assert.strictEqual(fs.existsSync(replaced), true, '.replaced-* is not swept by default');
    assert.ok(await readManagedServerInstallation(request));
  });

  test('a recoverable .replaced-* is kept when the fetch fails and swept after success (#9304)', async () => {
    const request = installRequest(root, '8.4.0');
    const finalDir = path.join(request.serversRoot, request.version, request.platformTarget);
    const replaced = `${finalDir}.replaced-1-r`;
    await fs.promises.mkdir(replaced, { recursive: true });
    const failing = operations('x', '8.4.0');
    failing.resolveArchive = async () => {
      throw new Error('offline');
    };
    await assert.rejects(installManagedServer(request, failing), /offline/);
    assert.strictEqual(fs.existsSync(replaced), true);
    await installManagedServer(request, operations('ok', '8.4.0'));
    assert.strictEqual(fs.existsSync(replaced), false);
  });

  test('reclaim restores a lock whose owner changed during the move, or leaves it aside (#9304)', async () => {
    const request = installRequest(root, '8.5.0');
    const versionDir = path.join(request.serversRoot, request.version);
    const lockPath = path.join(versionDir, `${request.platformTarget}.install.lock`);
    await fs.promises.mkdir(lockPath, { recursive: true });
    await fs.promises.writeFile(path.join(lockPath, 'owner'), `${deadPid()}:dead\n`);
    const swap = async () => {
      await fs.promises.writeFile(path.join(lockPath, 'owner'), `${process.pid}:new-owner\n`);
    };
    assert.strictEqual(await reclaimStaleLock(lockPath, request.serversRoot, Date.now(), { beforeMove: swap }), false);
    assert.strictEqual((await fs.promises.readFile(path.join(lockPath, 'owner'), 'utf8')).trim(), `${process.pid}:new-owner`);

    // Restore failure: a new lock appears at the path; the moved-aside live lock is kept.
    await fs.promises.rm(lockPath, { recursive: true });
    await fs.promises.mkdir(lockPath);
    await fs.promises.writeFile(path.join(lockPath, 'owner'), `${deadPid()}:dead\n`);
    const blocker = async () => {
      await fs.promises.mkdir(lockPath);
      await fs.promises.writeFile(path.join(lockPath, 'owner'), `${process.pid}:blocker\n`);
    };
    assert.strictEqual(
      await reclaimStaleLock(lockPath, request.serversRoot, Date.now(), { beforeMove: swap, afterMove: blocker }),
      false
    );
    const asides = (await fs.promises.readdir(versionDir)).filter((e) => e.includes('.install.lock.stale-'));
    assert.strictEqual(asides.length, 1, 'the possibly live lock is left for the stale sweep');
  });

  test('missing or unparseable owner is reclaimed only when the heartbeat is stale (#9304)', async () => {
    const request = installRequest(root, '8.6.0');
    const versionDir = path.join(request.serversRoot, request.version);
    const old = new Date(Date.now() - 60 * 60_000);
    for (const owner of [undefined, 'garbage\n']) {
      const lockPath = path.join(versionDir, `${request.platformTarget}.install.lock`);
      await fs.promises.mkdir(lockPath, { recursive: true });
      if (owner !== undefined) {
        await fs.promises.writeFile(path.join(lockPath, 'owner'), owner);
      }
      assert.strictEqual(await reclaimStaleLock(lockPath, request.serversRoot), false, 'fresh mtime is not stale');
      await fs.promises.utimes(lockPath, old, old);
      assert.strictEqual(await reclaimStaleLock(lockPath, request.serversRoot), true);
      assert.strictEqual(fs.existsSync(lockPath), false);
    }
  });

  test('an installer that lost its lock mid-stage does not promote or release the new owner (#9304)', async () => {
    const request = installRequest(root, '8.7.0');
    const finalDir = path.join(request.serversRoot, request.version, request.platformTarget);
    const lockPath = `${finalDir}.install.lock`;
    const takeover = operations('takeover', '8.7.0', async () => {
      await fs.promises.writeFile(path.join(lockPath, 'owner'), `${process.pid}:other\n`);
    });
    await assert.rejects(installManagedServer(request, takeover), /lock was lost/);
    assert.strictEqual((await fs.promises.readFile(path.join(lockPath, 'owner'), 'utf8')).trim(), `${process.pid}:other`);
    assert.strictEqual(fs.existsSync(finalDir), false, 'no promotion without the lock');
  });

  test('a valid cache hit clears a leftover .replaced-* unless the lock is held (#9304)', async () => {
    const request = installRequest(root, '8.8.0');
    const installed = await installManagedServer(request, operations('hit', '8.8.0'));
    const finalDir = path.dirname(installed.executablePath);
    const leftover = `${finalDir}.replaced-1-q`;
    const lockPath = `${finalDir}.install.lock`;
    await fs.promises.mkdir(leftover);
    await fs.promises.mkdir(lockPath);
    await fs.promises.writeFile(path.join(lockPath, 'owner'), `${process.pid}:live\n`);
    await installManagedServer(request, operations('hit', '8.8.0'));
    assert.strictEqual(fs.existsSync(leftover), true, 'busy lock: left alone');
    assert.strictEqual(fs.existsSync(lockPath), true, 'busy lock is not disturbed');
    await fs.promises.rm(lockPath, { recursive: true });
    await installManagedServer(request, operations('hit', '8.8.0'));
    assert.strictEqual(fs.existsSync(leftover), false);
    assert.strictEqual(fs.existsSync(lockPath), false);
    assert.ok(await readManagedServerInstallation(request));
  });

  test('prerelease requests admit the generation-keyed manifest version through staging and cache read-back', async () => {
    const request = installRequest(root, '2.1.0-rc.1');
    const installed = await installManagedServer(request, operations('rc-generation-binary', '2.1.0'));

    assert.strictEqual(installed.receipt.requestedVersion, '2.1.0-rc.1');
    assert.strictEqual(installed.receipt.manifestVersion, '2.1.0');

    const reread = await readManagedServerInstallation(request);
    assert.ok(reread, 'a completed generation-keyed RC install must remain cache-eligible');
    assert.strictEqual(reread.receipt.manifestVersion, '2.1.0');
    assert.strictEqual(reread.receipt.requestedVersion, '2.1.0-rc.1');
  });

  test('active probe version remains authoritative over receipt-time identity', async () => {
    const installation = await installManagedServer(
      installRequest(root, '6.0.0'),
      operations('identity-binary', '6.0.0', undefined, 'ripr receipt-time')
    );
    const active = { binaryVersion: 'ripr active-probe' };

    const combined = combineActiveManagedServerIdentity(active, installation);

    assert.strictEqual(installation.receipt.binaryVersion, 'ripr receipt-time');
    assert.strictEqual(combined.binaryVersion, 'ripr active-probe');
    assert.strictEqual(combined.assetDigest, installation.receipt.archiveSha256);
    assert.strictEqual(combined.installationState, 'complete');
  });

  test('authoritative provisioning guide matches the completed receipt contract', async () => {
    const repoRoot = path.resolve(__dirname, '../../../../..');
    const guide = await fs.promises.readFile(path.join(repoRoot, 'docs', 'SERVER_PROVISIONING.md'), 'utf8');
    const normalizedGuide = guide.replace(/\s+/g, ' ');

    assert.ok(guide.includes('install-receipt.json'));
    assert.ok(guide.includes('`installationState: "complete"`'));
    assert.ok(guide.includes('current executable SHA-256 matches'));
    assert.ok(normalizedGuide.includes('not a producer provenance attestation'));
    assert.strictEqual(guide.includes('sha256.txt'), false);
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
  beforeExtract?: (() => Promise<void>) | undefined,
  reportedVersion = 'ripr 1.2.3',
  manifestSha256?: string
): ManagedServerInstallOperations {
  const bytes = Buffer.from(`archive:${executableContents}`);
  return {
    resolveArchive: async () => ({
      manifestVersion: version,
      expectedSha256: crypto.createHash('sha256').update(bytes).digest('hex'),
      bytes,
      ...(manifestSha256 !== undefined ? { manifestSha256 } : {})
    }),
    extractArchive: async (_archivePath, destination) => {
      await beforeExtract?.();
      const executableName = process.platform === 'win32' ? 'ripr.exe' : 'ripr';
      await fs.promises.writeFile(path.join(destination, executableName), executableContents);
    },
    probeExecutable: async () => reportedVersion
  };
}

/** A pid that is not running: probe a high pid range for ESRCH. */
function deadPid(): number {
  for (let candidate = 2_000_000_000; candidate < 2_000_000_100; candidate += 1) {
    try {
      process.kill(candidate, 0);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ESRCH') {
        return candidate;
      }
    }
  }
  throw new Error('no dead pid candidate found');
}
