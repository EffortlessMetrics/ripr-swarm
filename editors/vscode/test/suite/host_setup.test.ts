import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { main, type TestHostRuntime } from '../runTest';

// Exercise the retained runner with real workspace staging and failure
// finalization. Only the host/producer/process-exit boundaries are substituted;
// these controls do not claim to run the genuine Rust producer or VS Code.
suite('VS Code test host setup', () => {
  const environmentKeys = [
    'RIPR_TEST_ARTIFACT_ROOT', 'RIPR_TEST_EXTENSION_PATH',
    'RIPR_TEST_WORKSPACE_PATH', 'RIPR_TEST_WORKSPACE_TRUST',
    'RIPR_TEST_MULTIROOT_ONLY', 'RIPR_TEST_ACTIONABLE_QUEUE_ROOT',
    'RIPR_TEST_BASE_REF', 'RIPR_TEST_CLIPBOARD_CAPTURE_PATH',
    'RIPR_TEST_SERVER_PATH', 'RIPR_TEST_PACKAGED_SERVER_PATH',
    'RIPR_TEST_REQUIRE_RAN', 'RIPR_TEST_MULTIROOT', 'RIPR_TEST_GREP',
    'RIPR_TEST_MULTIROOT_PATH', 'RIPR_TEST_MULTIROOT_RECEIPT',
  ];
  let previousEnvironment: Map<string, string | undefined>;
  let root: string;
  let artifactRoot: string;
  let vscodeExecutablePath: string;
  let queueRoot: string;
  let events: string[];
  let producerCalls: Array<{ repoRoot: string; exportPrefix: string }>;
  let hostCalls: Array<{ executable: string | undefined; queue: string | undefined }>;
  let exits: number[];
  let runtime: TestHostRuntime;

  setup(() => {
    previousEnvironment = new Map(environmentKeys.map(key => [key, process.env[key]]));
    for (const key of environmentKeys) {
      delete process.env[key];
    }
    fs.mkdirSync(os.tmpdir(), { recursive: true });
    root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-host-setup-'));
    artifactRoot = path.join(root, 'artifacts');
    const templatePath = path.join(root, 'template');
    fs.mkdirSync(templatePath);
    fs.writeFileSync(path.join(templatePath, 'Cargo.toml'),
      '[package]\nname = "boundary_gap_fixture"\nversion = "0.1.0"\nedition = "2024"\n');
    const serverPath = path.join(root, 'ripr-server');
    fs.writeFileSync(serverPath, 'not executed by these setup controls\n');
    vscodeExecutablePath = path.join(root, 'vscode-host');
    queueRoot = path.join(root, 'producer-queue');
    fs.mkdirSync(queueRoot);
    process.env.RIPR_TEST_ARTIFACT_ROOT = artifactRoot;
    process.env.RIPR_TEST_WORKSPACE_PATH = templatePath;
    process.env.RIPR_TEST_SERVER_PATH = serverPath;
    events = [];
    producerCalls = [];
    hostCalls = [];
    exits = [];
    runtime = {
      downloadVSCode: async (cachePath, extensionDevelopmentPath) => {
        assert.strictEqual(cachePath, path.join(artifactRoot, 'vscode-test-cache'));
        assert.strictEqual(extensionDevelopmentPath, path.resolve(__dirname, '../../..'));
        events.push('download');
        return vscodeExecutablePath;
      },
      stageGenuineActionableQueue: (repoRoot, exportPrefix) => {
        events.push('producer');
        producerCalls.push({ repoRoot, exportPrefix });
        return queueRoot;
      },
      runTests: async (options) => {
        events.push('host');
        hostCalls.push({ executable: options.vscodeExecutablePath,
          queue: process.env.RIPR_TEST_ACTIONABLE_QUEUE_ROOT });
        return 0;
      },
      runUntrustedTests: async (executable) => {
        events.push('untrusted-host');
        hostCalls.push({ executable, queue: process.env.RIPR_TEST_ACTIONABLE_QUEUE_ROOT });
      },
      exit: (code) => { exits.push(code); },
    };
  });

  teardown(() => {
    for (const [key, value] of previousEnvironment) {
      if (value === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = value;
      }
    }
    fs.rmSync(root, { recursive: true, force: true });
  });

  function assertFailure(message: string) {
    assert.deepStrictEqual(exits, [1]);
    const retention = JSON.parse(fs.readFileSync(path.join(artifactRoot,
      'vscode-test-host-diagnostics', 'retention.json'), 'utf8'));
    assert.strictEqual(retention.retained_because, 'test_host_failure');
    assert.strictEqual(retention.error.message, message);
  }

  test('a rejected host download invokes zero producers and retains the original failure', async () => {
    runtime.downloadVSCode = async () => {
      events.push('download');
      throw new Error('host download blocked by test network policy');
    };

    await main(runtime);

    assert.deepStrictEqual(events, ['download']);
    assert.strictEqual(producerCalls.length, 0);
    assert.strictEqual(hostCalls.length, 0);
    assert.strictEqual(process.env.RIPR_TEST_ACTIONABLE_QUEUE_ROOT, undefined);
    assertFailure('host download blocked by test network policy');
  });

  test('awaits host acquisition before exactly one producer and reuses that host for both runs', async () => {
    let releaseHost!: (executable: string) => void;
    const acquiredHost = new Promise<string>((resolve) => { releaseHost = resolve; });
    runtime.downloadVSCode = async () => {
      events.push('download-start');
      const executable = await acquiredHost;
      events.push('download-complete');
      return executable;
    };

    const run = main(runtime);
    const beforeAcquisition = [...events];
    releaseHost(vscodeExecutablePath);
    await run;

    assert.deepStrictEqual(beforeAcquisition, ['download-start']);
    assert.deepStrictEqual(events, ['download-start', 'download-complete', 'producer', 'host', 'host']);
    assert.deepStrictEqual(producerCalls, [{
      repoRoot: path.resolve(__dirname, '../../../../..'),
      exportPrefix: path.join(artifactRoot, `actionable-queue-${process.pid}`),
    }]);
    assert.deepStrictEqual(hostCalls, [
      { executable: vscodeExecutablePath, queue: queueRoot },
      { executable: vscodeExecutablePath, queue: queueRoot },
    ]);
    assert.deepStrictEqual(exits, []);
    assert.ok(!fs.existsSync(path.join(artifactRoot, 'vscode-test-host-diagnostics')));
  });

  test('a producer failure after acquisition prevents host execution and retains that failure', async () => {
    runtime.stageGenuineActionableQueue = () => {
      events.push('producer');
      throw new Error('genuine producer failed its receipt validation');
    };

    await main(runtime);

    assert.deepStrictEqual(events, ['download', 'producer']);
    assert.strictEqual(hostCalls.length, 0);
    assert.strictEqual(process.env.RIPR_TEST_ACTIONABLE_QUEUE_ROOT, undefined);
    assertFailure('genuine producer failed its receipt validation');
  });

  test('the untrusted path acquires its host without invoking the producer', async () => {
    process.env.RIPR_TEST_WORKSPACE_TRUST = 'untrusted';

    await main(runtime);

    assert.deepStrictEqual(events, ['download', 'untrusted-host']);
    assert.strictEqual(producerCalls.length, 0);
    assert.deepStrictEqual(hostCalls, [{ executable: vscodeExecutablePath, queue: undefined }]);
    assert.deepStrictEqual(exits, []);
  });

  test('the multiroot-only path acquires its host without invoking the producer', async () => {
    process.env.RIPR_TEST_MULTIROOT_ONLY = '1';

    await main(runtime);

    assert.deepStrictEqual(events, ['download', 'host']);
    assert.strictEqual(producerCalls.length, 0);
    assert.deepStrictEqual(hostCalls, [{ executable: vscodeExecutablePath, queue: undefined }]);
    assert.deepStrictEqual(exits, []);
  });

  test('a caller-supplied queue is preserved without invoking another producer', async () => {
    const suppliedQueue = path.join(root, 'supplied-queue');
    fs.mkdirSync(suppliedQueue);
    process.env.RIPR_TEST_ACTIONABLE_QUEUE_ROOT = suppliedQueue;

    await main(runtime);

    assert.deepStrictEqual(events, ['download', 'host', 'host']);
    assert.strictEqual(producerCalls.length, 0);
    assert.deepStrictEqual(hostCalls, [
      { executable: vscodeExecutablePath, queue: suppliedQueue },
      { executable: vscodeExecutablePath, queue: suppliedQueue },
    ]);
    assert.deepStrictEqual(exits, []);
  });
});
