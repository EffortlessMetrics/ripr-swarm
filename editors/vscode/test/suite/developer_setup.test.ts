import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { FixtureProducerError, runFixtureProducer } from '../actionableQueueFixture';
import { requireEditorDisplay, verifyHostPrerequisites } from '../hostPrerequisites';

interface SetupReport {
  requirement_sources: Array<{ path: string; sha256: string }>;
  requirements: { rust_channel: string; node_major: number; typescript: string };
  checks: Array<{ check_id: string; status: string; observed_fact: unknown }>;
  known_blockers: string[];
  full_development_ready: false;
  fresh_task: string;
}

const reportSource = path.resolve(__dirname, '../../../test/developerSetup.cjs');
const diagnostic = require(reportSource) as {
  collectDeveloperSetup: (options: Record<string, unknown>) => SetupReport;
  renderDeveloperSetup: (report: SetupReport) => string;
};

function alive(pid: number): boolean {
  try { process.kill(pid, 0); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ESRCH') return false;
    throw error;
  }
  if (process.platform === 'linux') {
    try {
      const stat = fs.readFileSync('/proc/' + pid + '/stat', 'utf8');
      return !['Z', 'X'].includes(stat.slice(stat.lastIndexOf(')') + 2, stat.lastIndexOf(')') + 3));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
  return true;
}

async function waitFor(predicate: () => boolean, message: string): Promise<void> {
  const deadline = Date.now() + 10_000;
  while (!predicate()) {
    assert.ok(Date.now() < deadline, message);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
}

suite('Developer setup capability boundary', function () {
  this.timeout(60_000);
  let root: string;
  const ownedPids = new Set<number>();
  const nativeBudget = process.platform === 'win32' ? 25_000 : 5000;

  setup(() => {
    fs.mkdirSync(os.tmpdir(), { recursive: true });
    root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr developer setup '));
    for (const directory of ['editors/vscode', '.github/workflows', '.cargo']) {
      fs.mkdirSync(path.join(root, directory), { recursive: true });
    }
    fs.writeFileSync(path.join(root, 'rust-toolchain.toml'),
      '[toolchain]\nchannel = "1.96.2"\ncomponents = ["rustfmt", "clippy"]\n');
    fs.writeFileSync(path.join(root, 'editors/vscode/package.json'),
      JSON.stringify({ engines: { vscode: '^1.95.0' } }));
    fs.writeFileSync(path.join(root, 'editors/vscode/package-lock.json'), JSON.stringify({ packages: {
      'node_modules/typescript': { version: '6.1.1' },
      'node_modules/@vscode/test-electron': { version: '2.5.2' }
    } }));
    fs.writeFileSync(path.join(root, '.github/workflows/vscode-e2e.yml'), '      node-version: 25\n');
    fs.writeFileSync(path.join(root, '.github/workflows/rust-gates.yml'),
      '      components: llvm-tools-preview\n      uses: taiki-e/install-action@' + 'a'.repeat(40) + '\n      tool: nextest\n');
    fs.writeFileSync(path.join(root, '.cargo/config.toml'), ['TEMP', 'TMP', 'TMPDIR']
      .map(name => name + ' = { value = "target", relative = true, force = true }\n').join(''));
  });

  teardown(async () => {
    for (const name of ['primary.pid', 'descendant.pid']) {
      const file = path.join(root, name);
      if (fs.existsSync(file)) ownedPids.add(Number(fs.readFileSync(file, 'utf8')));
    }
    for (const pid of ownedPids) {
      if (alive(pid)) process.kill(pid, 'SIGKILL');
      await waitFor(() => !alive(pid), 'owned setup process remained active during teardown');
    }
    ownedPids.clear();
    fs.rmSync(root, { recursive: true, force: true });
  });

  function collect(env: NodeJS.ProcessEnv = {}) {
    return diagnostic.collectDeveloperSetup({ root, env, platform: 'linux', nodeVersion: '25.1.0' });
  }

  test('reads changed source requirements and leaves execution capabilities unclaimed', () => {
    const report = collect({ DISPLAY: ':321', RUSTUP_TOOLCHAIN: 'selected-toolchain' });
    assert.strictEqual(report.requirements.rust_channel, '1.96.2');
    assert.strictEqual(report.requirements.node_major, 25);
    assert.strictEqual(report.requirements.typescript, '6.1.1');
    assert.strictEqual(report.requirement_sources.length, 6);
    assert.ok(report.requirement_sources.every(source => /^[a-f0-9]{64}$/.test(source.sha256)));
    for (const id of ['source_build_doctor', 'compiler_matched_llvm_profile', 'editor_host_acquisition',
      'editor_native_host', 'repository_policy', 'genuine_editor_producer', 'full_development_validation']) {
      assert.strictEqual(report.checks.find(check => check.check_id === id)?.status, 'not_run', id);
    }
    assert.strictEqual(report.checks.find(check => check.check_id === 'rust_activation')?.status, 'not_proven');
    assert.strictEqual(report.checks.find(check => check.check_id === 'editor_display')?.status, 'not_proven');
    assert.strictEqual(report.full_development_ready, false);
    assert.strictEqual(report.fresh_task, 'not_proven');
    const human = diagnostic.renderDeveloperSetup(report);
    for (const check of report.checks) assert.ok(human.includes(check.check_id + ': ' + check.status));
  });

  test('missing display and unsupported or oversized authorities fail visibly before downstream work', () => {
    let report = collect();
    assert.deepStrictEqual(report.known_blockers, ['editor_display']);
    assert.strictEqual(report.checks.find(check => check.check_id === 'genuine_editor_producer')?.status, 'not_run');
    fs.writeFileSync(path.join(root, 'rust-toolchain.toml'), 'channel = ["not the supported scalar"]\n');
    report = collect({ DISPLAY: ':321' });
    assert.ok(report.known_blockers.includes('source_requirements'));
    fs.writeFileSync(path.join(root, 'rust-toolchain.toml'), 'x'.repeat(256 * 1024 + 1));
    report = collect({ DISPLAY: ':321' });
    assert.ok(report.known_blockers.includes('source_requirements'));
  });

  test('source frontdoor succeeds when writes network and subprocess APIs are forbidden', async () => {
    const guard = path.join(root, 'read only guard.cjs');
    fs.writeFileSync(guard, [
      'const fs = require("node:fs");',
      'const deny = () => { throw new Error("unexpected diagnostic side effect"); };',
      'for (const name of ["writeFileSync","appendFileSync","mkdirSync","rmSync","renameSync","unlinkSync"]) fs[name] = deny;',
      'const open = fs.openSync; fs.openSync = (p,f,...rest) => { if (typeof f !== "number" || (f & (fs.constants.O_WRONLY|fs.constants.O_RDWR|fs.constants.O_CREAT|fs.constants.O_TRUNC|fs.constants.O_APPEND))) deny(); return open(p,f,...rest); };',
      'const cp = require("node:child_process"); for (const name of ["spawn","spawnSync","exec","execSync","execFile","execFileSync","fork"]) cp[name] = deny;',
      'for (const name of ["node:http","node:https"]) { const network = require(name); network.request = deny; network.get = deny; }',
      'const report = require(' + JSON.stringify(reportSource) + ').collectDeveloperSetup({ root: ' + JSON.stringify(root) + ', env: { DISPLAY: ":321" }, platform: "linux", nodeVersion: "25.1.0" });',
      'process.stdout.write(JSON.stringify(report));'
    ].join('\n'));
    const output = await runFixtureProducer(process.execPath, [guard], {
      cwd: root, env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' }, timeoutMs: nativeBudget
    });
    const report: SetupReport = JSON.parse(output);
    assert.deepStrictEqual(report.known_blockers, []);
    assert.strictEqual(report.full_development_ready, false);
    assert.strictEqual(fs.existsSync(path.join(root, 'target')), false);
  });

  test('display check distinguishes absent Linux display from other platforms without launching anything', () => {
    assert.throws(() => requireEditorDisplay('linux', {}), /live display/);
    assert.doesNotThrow(() => requireEditorDisplay('linux', { DISPLAY: ':321' }));
    assert.doesNotThrow(() => requireEditorDisplay('win32', {}));
    assert.doesNotThrow(() => requireEditorDisplay('darwin', {}));
  });

  test('report bounds observed text and never renders proxy or CA values', () => {
    const report = collect({ DISPLAY: ':321', RUSTUP_TOOLCHAIN: 'x'.repeat(100_000),
      HTTPS_PROXY: 'https://private-user:private-password@example.invalid',
      NODE_EXTRA_CA_CERTS: '/private-ca-value', CARGO_TARGET_DIR: 'y'.repeat(100_000) });
    const json = JSON.stringify(report);
    assert.ok(Buffer.byteLength(json) < 16 * 1024);
    assert.ok(!json.includes('private-user'));
    assert.ok(!json.includes('private-password'));
    assert.ok(!json.includes('private-ca-value'));
  });

  function writeSentinel(env: NodeJS.ProcessEnv, nonce = env.RIPR_SETUP_HOST_NONCE) {
    assert.ok(env.RIPR_SETUP_HOST_RECEIPT);
    fs.writeFileSync(env.RIPR_SETUP_HOST_RECEIPT, JSON.stringify({ schema_version: 1, nonce,
      vscode_version: '1.96.2', electron_version: '33.2.1', platform: process.platform, smoke_cases: 1 }));
  }

  test('requires native success and one current host sentinel and preserves the observed version', async () => {
    const host = path.join(root, 'editor host.exe');
    let calls = 0;
    const receipt = await verifyHostPrerequisites(host, root, { env: { DISPLAY: ':321' },
      runProcess: async (command, args, options) => {
        calls += 1;
        assert.strictEqual(command, host);
        assert.ok(args.some(arg => arg.startsWith('--extensionTestsPath=') && arg.endsWith('setupHostSmoke.js')));
        assert.ok(args.some(arg => arg.startsWith('--extensionDevelopmentPath=') && arg.endsWith('empty extension')));
        assert.ok(args.includes('--disable-extensions'));
        assert.ok(options.signal);
        writeSentinel(options.env);
        return '';
      } });
    assert.strictEqual(calls, 1);
    assert.strictEqual(receipt.vscode_version, '1.96.2');
    assert.strictEqual(receipt.native_exit_code, 0);
    assert.strictEqual(receipt.smoke_cases, 1);
    assert.strictEqual(receipt.full_editor_suite, 'not_run');
    assert.deepStrictEqual(JSON.parse(fs.readFileSync(path.join(root, 'setup-editor-host.json'), 'utf8')), receipt);
    assert.ok(!fs.readdirSync(root).some(name => name.startsWith('setup-host-')));
  });

  for (const kind of ['absent', 'wrong_nonce', 'native_failure'] as const) {
    test('refuses ' + kind + ' host evidence and removes an earlier success receipt', async () => {
      fs.writeFileSync(path.join(root, 'setup-editor-host.json'), '{"status":"pass","nonce":"old"}');
      await assert.rejects(verifyHostPrerequisites('controlled-host', root, { env: { DISPLAY: ':321' },
        runProcess: async (_command, _args, options) => {
          if (kind === 'wrong_nonce') writeSentinel(options.env, 'old');
          if (kind === 'native_failure') {
            writeSentinel(options.env);
            throw new FixtureProducerError('process_failure', 'native failure', '', 'missing native library', 17);
          }
          return '';
        } }), /failed before producer work/);
      assert.strictEqual(fs.existsSync(path.join(root, 'setup-editor-host.json')), false);
      assert.ok(fs.statSync(path.join(root, 'setup-editor-host.log')).size > 0);
      if (kind === 'native_failure') {
        const log = fs.readFileSync(path.join(root, 'setup-editor-host.log'), 'utf8');
        assert.ok(log.includes('status=17'));
        assert.ok(log.includes('missing native library'));
      }
      assert.ok(!fs.readdirSync(root).some(name => name.startsWith('setup-host-')));
    });
  }

  for (const mode of ['timeout', 'cancel'] as const) {
    test(mode + ' of the public host prerequisite terminates its actual owned descendant', async () => {
      const descendant = path.join(root, 'descendant.js');
      const primary = path.join(root, 'host primary.js');
      fs.writeFileSync(descendant,
        'process.on("SIGTERM", () => {}); require("fs").writeFileSync("descendant.pid", String(process.pid)); setInterval(() => {}, 1000);');
      fs.writeFileSync(primary, [
        'const fs = require("fs");',
        'require("child_process").spawn(process.execPath, [' + JSON.stringify(descendant) + '], { stdio: "inherit", detached: process.platform === "win32" });',
        'process.on("SIGTERM", () => {}); fs.writeFileSync("primary.pid", String(process.pid));',
        mode === 'timeout'
          ? 'const timer = setInterval(() => { if (fs.existsSync("descendant.pid")) { clearInterval(timer); process.exit(0); } }, 10);'
          : 'setInterval(() => {}, 1000);'
      ].join('\n'));
      const cancellation = new AbortController();
      let settled = false;
      const outcome = verifyHostPrerequisites('controlled-native-host', path.join(root, 'artifacts'), {
        timeoutMs: nativeBudget, signal: cancellation.signal, env: { ...process.env, DISPLAY: ':321' },
        runProcess: (_command, _args, options) => runFixtureProducer(process.execPath, [primary], {
          ...options, onProgress: () => {}, cwd: root, env: { ...options.env, ELECTRON_RUN_AS_NODE: '1' }
        })
      }).then(() => undefined, error => error as Error).finally(() => { settled = true; });
      await waitFor(() => fs.existsSync(path.join(root, 'primary.pid'))
        && fs.existsSync(path.join(root, 'descendant.pid')), 'native setup fixture did not start');
      const primaryPid = Number(fs.readFileSync(path.join(root, 'primary.pid'), 'utf8'));
      const descendantPid = Number(fs.readFileSync(path.join(root, 'descendant.pid'), 'utf8'));
      ownedPids.add(primaryPid); ownedPids.add(descendantPid);
      if (mode === 'timeout') await waitFor(() => !alive(primaryPid), 'primary did not exit');
      else assert.ok(alive(primaryPid), 'primary was already inactive before cancellation');
      assert.ok(alive(descendantPid), 'descendant was already inactive before the owner acted');
      assert.strictEqual(settled, false, 'host prerequisite settled before pipe-owning descendant cleanup');
      if (mode === 'cancel') cancellation.abort('controlled host cancellation');
      const error = await outcome;
      assert.ok(error instanceof Error);
      assert.ok(error.cause instanceof FixtureProducerError);
      assert.strictEqual(error.cause.kind, mode === 'timeout' ? 'timeout' : 'cancelled');
      await waitFor(() => !alive(primaryPid) && !alive(descendantPid), 'host process tree remained active');
      assert.strictEqual(fs.existsSync(path.join(root, 'artifacts/setup-editor-host.json')), false);
    });
  }
});
