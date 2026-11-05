import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { FixtureProducerError, runFixtureProducer } from '../actionableQueueFixture';

const editorSource = path.resolve(__dirname, '../../..');
const diagnosticSource = path.join(editorSource, 'test/developerSetup.cjs');
const profileTest = 'coverage_diagnostics::linux::binary_ids::tests::real_rust_llvm_binary_id_matches_sparse_instrumented_elf_metadata';
const shellLiteral = (value: string): string => "'" + value.replace(/'/g, "'\\''") + "'";
interface FullResult {
  status: string;
  run_id: string;
  full_development_ready: boolean;
  fresh_task: string;
  exit_code: number;
  evidence_directory: string;
  tools: Record<string, string>;
  source: { head: string; cleanliness: string };
  compiler_selection: { selector: string; source: string };
  failure?: { stage_id: string; kind: string; native_status: number | null; native_signal: string | null };
  stages: Array<{ stage_id: string; status: string; commands: Array<{ argv: string[]; cwd: string }>;
    log?: string; counts?: unknown; canonical_invocations?: number }>;
}
interface Call { tool: string; args: string[]; cwd: string; selected: boolean; proxy_preserved: boolean; cargo_target: string }

suite('Explicit full-development coordinator', function () {
  this.timeout(60_000);
  let root: string;
  let editor: string;
  let artifacts: string;
  let env: NodeJS.ProcessEnv;
  let npmCli: string;

  setup(function () {
    // The public preset deliberately refuses other native platforms. Existing
    // process-owner controls retain their separate Windows acceptance.
    if (process.platform !== 'linux') { this.skip(); return; }
    fs.mkdirSync(os.tmpdir(), { recursive: true });
    root = fs.mkdtempSync(path.join(os.tmpdir(), "ripr full setup ' $ literal "));
    editor = path.join(root, 'editors/vscode');
    artifacts = path.join(root, 'artifacts with spaces');
    for (const directory of ['tools', '.github/workflows', '.cargo', 'tool bin', 'artifacts with spaces',
      'editors/vscode/test', 'editors/vscode/src', 'editors/vscode/out/test',
      'editors/vscode/node_modules/typescript/bin', 'editors/vscode/node_modules/@vscode/test-electron',
      'editors/vscode/node_modules/.bin']) fs.mkdirSync(path.join(root, directory), { recursive: true });
    for (const relative of ['test/developerSetup.cjs', 'test/hostDiagnostics.ts', 'src/lspCompatibility.ts']) {
      fs.copyFileSync(path.join(editorSource, relative), path.join(editor, relative));
    }
    const npmCandidates = [process.env.npm_execpath
      ? path.join(path.dirname(fs.realpathSync.native(process.env.npm_execpath)), 'npm-cli.js') : undefined,
    path.join(path.dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js'),
    path.resolve(path.dirname(process.execPath), '../lib/node_modules/npm/bin/npm-cli.js')];
    npmCli = npmCandidates.find(candidate => candidate && fs.existsSync(candidate)) ?? '';
    assert.ok(npmCli, 'Installed npm is required to exercise the actual public command chain');
    const scripts = JSON.parse(fs.readFileSync(path.join(editorSource, 'package.json'), 'utf8')).scripts;
    fs.writeFileSync(path.join(editor, 'package.json'), JSON.stringify({ engines: { vscode: '^1.91.0' }, scripts }));
    fs.writeFileSync(path.join(editor, 'package-lock.json'), JSON.stringify({ packages: {
      'node_modules/typescript': { version: '5.9.3' },
      'node_modules/@vscode/test-electron': { version: '2.5.2' }
    } }));
    fs.writeFileSync(path.join(editor, 'node_modules/typescript/package.json'), '{"version":"5.9.3"}');
    fs.writeFileSync(path.join(editor, 'node_modules/@vscode/test-electron/package.json'), '{"version":"2.5.2"}');
    fs.writeFileSync(path.join(editor, 'node_modules/.bin/vsce'), 'controlled installed packaging prerequisite');
    fs.writeFileSync(path.join(root, 'rust-toolchain.toml'), 'channel = "1.95.0"\ncomponents = ["clippy", "rustfmt"]\n');
    fs.writeFileSync(path.join(root, '.github/workflows/vscode-e2e.yml'),
      '      node-version: ' + process.versions.node.split('.')[0] + '\n');
    fs.writeFileSync(path.join(root, '.github/workflows/rust-gates.yml'),
      '      components: llvm-tools-preview\n      uses: taiki-e/install-action@' + 'a'.repeat(40) + '\n      tool: nextest\n');
    fs.writeFileSync(path.join(root, '.cargo/config.toml'), ['TEMP', 'TMP', 'TMPDIR']
      .map(name => `${name} = { value = "target", relative = true, force = true }\n`).join(''));
    const driver = path.join(root, 'controlled owners.cjs');
    fs.writeFileSync(driver, `
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const [tool, ...args] = process.argv.slice(2);
const root = process.env.RIPR_FULL_CONTROL_ROOT;
const trace = path.join(root, 'calls.jsonl');
if (process.env.RIPR_FULL_CONTROL_FAIL === 'distributed-delay') {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
}
fs.appendFileSync(trace, JSON.stringify({ tool, args, cwd: process.cwd(),
  selected: process.env.RUSTC === path.join(root, 'tool bin/rustc'),
  cargo_target: process.env.CARGO_TARGET_DIR,
  proxy_preserved: process.env.HTTPS_PROXY === 'https://private-user:private-password@example.invalid' }) + '\\n');
const receipt = () => fs.writeFileSync(path.join(process.env.RIPR_TEST_ARTIFACT_ROOT, 'setup-editor-host.json'),
  JSON.stringify({ schema_version:1,check_id:'editor_native_host',status:'pass',nonce:crypto.randomBytes(16).toString('hex'),
    platform:'linux',native_exit_code:0,smoke_cases:1,vscode_version:'1.145.0',electron_version:'43.8.3',
    full_editor_suite:'not_run',host_executable:'controlled host; not native host proof' }));
if (tool === 'git') process.stdout.write('a'.repeat(40) + '\\n');
else if (tool === 'rustc') process.stdout.write('rustc 1.95.0\\nhost: x86_64-unknown-linux-gnu\\nLLVM version: 22.1.0\\n');
else if (tool.startsWith('llvm-')) process.stdout.write('LLVM version 22.1.0\\n');
else if (tool === 'rustfmt') process.stdout.write('rustfmt 1.9.0\\n');
else if (tool === 'vsce') process.stdout.write('3.9.2\\n');
else if (tool === 'compiler') {
  if (process.env.RIPR_FULL_CONTROL_FAIL === 'compiler') process.exit(23);
} else if (tool === 'host') {
  if (process.env.RIPR_FULL_CONTROL_FAIL === 'host') process.exit(29);
  receipt();
} else if (tool === 'cargo') {
  if (args[0] === '-vV') process.stdout.write('cargo 1.95.0\\n');
  else if (args[0] === 'nextest') process.stdout.write('cargo-nextest 0.9.100\\n');
  else if (args[0] === 'clippy') process.stdout.write('clippy 0.1.95\\n');
  else if (args[0] === 'test') {
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'profile') process.stdout.write('0 passed; 0 failed; 0 ignored\\n');
    else {
      process.stdout.write('test ${profileTest} ... ok\\n1 passed; 0 failed; 0 ignored\\n');
      process.stderr.write('real LLVM association: raw_version=11, layout=llvm_v11, build-id=1234abcd, retained=128, mapping="matched", original=failure, replay=1\\n');
    }
  } else if (args.join(' ') === 'xtask ci-full') {
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'log-write') {
      const result = JSON.parse(fs.readFileSync(path.join(process.env.RIPR_TEST_ARTIFACT_ROOT, 'setup-full-development.json')));
      fs.mkdirSync(path.join(result.evidence_directory, 'rust_full.log'));
      process.stderr.write('original native aggregate failure with unavailable log\\n'); process.exit(37);
    }
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'rust') { process.stderr.write('original native aggregate failure\\n'); process.exit(37); }
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'long-rust') {
      process.stdout.write('compiler progress 🙂\\n'.repeat(10_000), () => {
        process.stderr.write('original terminal native aggregate failure through ' + process.env.HTTPS_PROXY + '\\n',
          () => process.exit(37));
      });
      return;
    }
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'signal') process.kill(process.pid, 'SIGTERM');
    if (process.env.RIPR_FULL_CONTROL_FAIL?.startsWith('interrupt:')) { process.kill(process.ppid, process.env.RIPR_FULL_CONTROL_FAIL.split(':')[1]); setInterval(() => {}, 1000); }
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'proxy') { process.stderr.write('original native failure through ' + process.env.HTTPS_PROXY + '\\n'); process.exit(37); }
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'output') process.stdout.write('x'.repeat(17 * 1024 * 1024));
    if (process.env.RIPR_FULL_CONTROL_FAIL === 'timeout') setInterval(() => {}, 1000);
    process.stdout.write('controlled aggregate; full native Rust proof not supplied\\n');
  } else if (args.join(' ') === 'xtask vscode-test-e2e') {
    if (process.env.RIPR_FULL_CONTROL_FAIL !== 'stale_host') receipt();
    process.stdout.write('  3 passing (1s)\\n  1 pending\\n  1 passing (1s)\\n');
  } else if (!['policy preflight', 'xtask vscode-package'].includes(args.join(' '))) {
    throw new Error('Unexpected Cargo operation: ' + args.join(' '));
  }
} else throw new Error('Unexpected controlled owner: ' + tool);
`);
    for (const tool of ['git', 'cargo', 'rustc', 'rustfmt', 'llvm-cov', 'llvm-profdata']) {
      fs.writeFileSync(path.join(root, 'tool bin', tool), '#!/bin/sh\nexec ' + shellLiteral(process.execPath)
        + ' ' + shellLiteral(driver) + ' ' + shellLiteral(tool) + ' "$@"\n', { mode: 0o755 });
    }
    // Electron's process.execPath is an Electron executable in native Mocha.
    // Only these controlled Node children need ELECTRON_RUN_AS_NODE=1.
    fs.writeFileSync(path.join(root, 'tool bin/npm'), '#!/bin/sh\nexec ' + shellLiteral(process.execPath)
      + ' ' + shellLiteral(npmCli) + ' "$@"\n', { mode: 0o755 });
    fs.writeFileSync(path.join(editor, 'node_modules/.bin/vsce'), '#!/bin/sh\nexec ' + shellLiteral(process.execPath)
      + ' ' + shellLiteral(driver) + ' vsce "$@"\n', { mode: 0o755 });
    fs.chmodSync(path.join(editor, 'node_modules/.bin/vsce'), 0o755);
    fs.writeFileSync(path.join(editor, 'node_modules/typescript/bin/tsc'),
      `process.argv.splice(2, 0, 'compiler'); require(${JSON.stringify(driver)});\n`);
    fs.writeFileSync(path.join(editor, 'out/test/verifySetup.js'),
      `process.argv.splice(2, 0, 'host'); require(${JSON.stringify(driver)});\n`);
    fs.writeFileSync(path.join(root, 'tools/activate-rust.sh'), [
      'printf "%s\\n" "$1" > ' + shellLiteral(path.join(root, 'selection.txt')),
      'if [[ -n ${RUSTC-} && $RUSTC != ' + shellLiteral(path.join(root, 'tool bin/rustc')) + ' ]]; then printf "conflicting compiler\\n" >&2; return 41; fi',
      'export CARGO_HOME=' + shellLiteral(path.join(root, 'cargo home')),
      'export RUSTUP_HOME=' + shellLiteral(path.join(root, 'rustup home')),
      'export RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu RUSTUP_AUTO_INSTALL=0',
      'export RUSTC=' + shellLiteral(path.join(root, 'tool bin/rustc')),
      'export PATH=' + shellLiteral(path.join(root, 'tool bin')) + ':"$PATH"',
      'export LLVM_COV=' + shellLiteral(path.join(root, 'tool bin/llvm-cov')),
      'export LLVM_PROFDATA=' + shellLiteral(path.join(root, 'tool bin/llvm-profdata')),
      'printf "controlled activation diagnostic, not environment JSON\\n"',
      'return 0'
    ].join('\n'));
    env = { ...process.env };
    for (const name of Object.keys(env)) {
      if (name.startsWith('RIPR_TEST_') || ['RUSTC', 'CARGO_BUILD_RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'].includes(name)) delete env[name];
    }
    Object.assign(env, { DISPLAY: ':controlled-display', ELECTRON_RUN_AS_NODE: '1',
      PATH: path.join(root, 'tool bin') + path.delimiter + process.env.PATH,
      RIPR_TEST_ARTIFACT_ROOT: artifacts, RIPR_FULL_CONTROL_ROOT: root,
      HTTPS_PROXY: 'https://private-user:private-password@example.invalid' });
  });

  teardown(() => { if (root) fs.rmSync(root, { recursive: true, force: true }); });

  function calls(): Call[] {
    const filename = path.join(root, 'calls.jsonl');
    return fs.existsSync(filename) ? fs.readFileSync(filename, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line)) : [];
  }

  async function invoke(extra: NodeJS.ProcessEnv = {}, timeout = 20_000, extraArgs: string[] = []): Promise<{ report: FullResult; status: number }> {
    let status = 0;
    try {
      await runFixtureProducer(process.execPath, [npmCli, 'run', 'setup:verify-full', '--', '--json', '--timeout-ms', String(timeout), ...extraArgs], {
        cwd: editor, env: { ...env, ...extra }, timeoutMs: 40_000
      });
    } catch (error) {
      assert.ok(error instanceof FixtureProducerError, String(error));
      status = error.status ?? -1;
    }
    return { report: JSON.parse(fs.readFileSync(path.join(artifacts, 'setup-full-development.json'), 'utf8')), status };
  }

  test('public preset delegates serial owners, preserves activation and invokes one canonical suite before packaging', async () => {
    const { report, status } = await invoke();
    assert.strictEqual(status, 0);
    assert.strictEqual(report.full_development_ready, true);
    assert.strictEqual(report.fresh_task, 'not_proven');
    assert.strictEqual(report.source.cleanliness, 'not_proven');
    assert.ok(report.stages.every(stage => stage.status === 'pass'));
    const native = calls();
    assert.ok(native.filter(call => call.tool !== 'git').every(call => call.selected && call.proxy_preserved));
    const selectedCargo = native.filter(call => call.tool === 'cargo').map(call => call.args);
    assert.deepStrictEqual(selectedCargo, [['-vV'], ['nextest', '--version'], ['clippy', '--version'],
      ['test', '--locked', '-p', 'repo-policy', '--lib', profileTest, '--', '--exact', '--nocapture'],
      ['policy', 'preflight'], ['xtask', 'ci-full'], ['xtask', 'vscode-test-e2e'], ['xtask', 'vscode-package']]);
    assert.ok(native.findIndex(call => call.tool === 'host') < native.findIndex(call => call.args.join(' ') === 'xtask ci-full'));
    assert.ok(native.filter(call => call.tool === 'cargo').slice(0, 6).every(call => call.cwd === root));
    assert.ok(native.findIndex(call => call.tool === 'vsce') < native.findIndex(call => call.args[0] === 'test'));
    assert.strictEqual(report.stages.find(stage => stage.stage_id === 'editor_suite')?.canonical_invocations, 1);
    const retained = fs.readdirSync(report.evidence_directory).map(name => fs.readFileSync(path.join(report.evidence_directory, name), 'utf8')).join('\n');
    assert.ok(retained.includes('raw_version=11'));
    for (const secret of ['private-user', 'private-password', env.PATH ?? '', '"PATH":']) assert.ok(!retained.includes(secret), secret);
  });

  for (const [failure, expected, lastStage] of [['compiler', 23, 'editor_prerequisites'], ['host', 29, 'editor_prerequisites'],
    ['rust', 37, 'rust_full']] as const) {
    test('preserves native ' + failure + ' failure and does not start later work', async () => {
      const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: failure });
      assert.strictEqual(status, expected);
      assert.strictEqual(report.exit_code, expected);
      assert.strictEqual(report.failure?.native_status, expected);
      assert.strictEqual(report.failure?.stage_id, lastStage);
      const last = report.stages.findIndex(stage => stage.stage_id === lastStage);
      assert.ok(report.stages.slice(0, last).every(stage => stage.status === 'pass'));
      assert.ok(report.stages.slice(last + 1).every(stage => stage.status === 'not_run'));
      assert.strictEqual(report.full_development_ready, false);
      assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
    });
  }

  test('fresh explicit attempt invalidates old success before a missing compiler and preserves unrelated receipts', async () => {
    const first = await invoke();
    assert.strictEqual(first.status, 0);
    const retained = path.join(artifacts, 'unrelated.json');
    fs.writeFileSync(retained, 'keep');
    fs.rmSync(path.join(editor, 'node_modules/typescript/bin/tsc'));
    const previousCalls = calls().length;
    const next = await invoke();
    assert.strictEqual(next.status, 1);
    assert.notStrictEqual(next.report.run_id, first.report.run_id);
    assert.strictEqual(next.report.full_development_ready, false);
    assert.strictEqual(next.report.failure?.stage_id, 'preflight');
    assert.ok(next.report.stages.slice(1).every(stage => stage.status === 'not_run'));
    assert.strictEqual(calls().length, previousCalls);
    assert.strictEqual(fs.readFileSync(retained, 'utf8'), 'keep');
    assert.strictEqual(JSON.parse(fs.readFileSync(path.join(first.report.evidence_directory, 'result.json'), 'utf8')).full_development_ready, true);
  });

  test('relative Cargo target keeps its invocation-directory meaning across all serial owners', async () => {
    const relative = 'full target with spaces';
    const expected = path.join(editor, relative);
    const { report, status } = await invoke({ CARGO_TARGET_DIR: relative });
    assert.strictEqual(status, 0);
    assert.strictEqual(report.full_development_ready, true);
    assert.ok(fs.statSync(expected).isDirectory());
    assert.ok(!fs.existsSync(path.join(root, relative)));
    assert.ok(calls().length > 0);
    assert.ok(calls().every(call => call.cargo_target === expected));
  });

  test('scope-changing inherited editor selections refuse the actual public preset before native work', async () => {
    for (const [name, value] of [['RIPR_TEST_SERVER_PATH', '/supplied/server'], ['RIPR_TEST_GREP', 'only one case'],
      ['RIPR_TEST_ACTIONABLE_QUEUE_ROOT', '/supplied/queue'], ['RIPR_TEST_MULTIROOT_ONLY', '1'],
      ['RIPR_TEST_WORKSPACE_TRUST', 'untrusted'], ['RIPR_TEST_WORKSPACE_TRUST', ''],
      ['RIPR_TEST_EXTENSION_PATH', ''], ['RIPR_TEST_WORKSPACE_PATH', '']]) {
      const { report, status } = await invoke({ [name]: value });
      assert.strictEqual(status, 1, name);
      assert.strictEqual(report.failure?.stage_id, 'preflight', name);
      assert.strictEqual(report.full_development_ready, false, name);
      assert.strictEqual(calls().length, 0, name);
    }
  });

  for (const [name, args] of [['oversized timeout', ['--timeout-ms', '43200001']],
    ['oversized selector', ['--toolchain', 'x'.repeat(129)]]] as const) {
    test('parsed ' + name + ' refusal replaces a prior successful result before native work', async () => {
      const first = await invoke();
      assert.strictEqual(first.status, 0);
      const before = calls().length;
      const refused = await invoke({}, 20_000, [...args]);
      assert.strictEqual(refused.status, 1);
      assert.notStrictEqual(refused.report.run_id, first.report.run_id);
      assert.strictEqual(refused.report.full_development_ready, false);
      assert.strictEqual(refused.report.failure?.stage_id, 'preflight');
      assert.strictEqual(calls().length, before);
    });
  }

  test('explicit selection overrides the inherited frozen selection and otherwise preserves it', async () => {
    const selected = '1.95.0-x86_64-unknown-linux-gnu';
    const inherited = { RUSTUP_TOOLCHAIN: selected, RUSTC: path.join(root, 'tool bin/rustc') };
    let result = await invoke(inherited);
    assert.strictEqual(result.status, 0);
    assert.strictEqual(fs.readFileSync(path.join(root, 'selection.txt'), 'utf8').trim(), selected);
    assert.strictEqual(result.report.compiler_selection.source, 'inherited_frozen_selector');
    result = await invoke(inherited, 20_000, ['--toolchain', 'stable']);
    assert.strictEqual(result.status, 0);
    assert.strictEqual(fs.readFileSync(path.join(root, 'selection.txt'), 'utf8').trim(), 'stable');
    assert.strictEqual(result.report.compiler_selection.source, 'explicit_cli');
  });

  test('ordinary npm setup report works without TypeScript source owners and preserves prior evidence', async () => {
    fs.rmSync(path.join(editor, 'src/lspCompatibility.ts'));
    fs.rmSync(path.join(editor, 'test/hostDiagnostics.ts'));
    const prior = JSON.stringify({ status: 'pass', full_development_ready: true, run_id: 'old' });
    fs.writeFileSync(path.join(artifacts, 'setup-full-development.json'), prior);
    await runFixtureProducer(process.execPath, [npmCli, 'run', 'setup:check', '--', '--json'], {
      cwd: editor, env, timeoutMs: 5000
    });
    assert.strictEqual(fs.readFileSync(path.join(artifacts, 'setup-full-development.json'), 'utf8'), prior);
    assert.deepStrictEqual(fs.readdirSync(artifacts), ['setup-full-development.json']);
    assert.strictEqual(calls().length, 0);
  });

  test('activation refuses a conflicting inherited compiler rather than clearing it', async () => {
    const { report, status } = await invoke({ RUSTC: '/wrong/compiler' });
    assert.strictEqual(status, 41);
    assert.strictEqual(report.failure?.stage_id, 'activation');
    assert.deepStrictEqual(calls().map(call => call.tool), ['git']);
  });

  test('zero selected native profile cases stop before editor or broad Rust work', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'profile' });
    assert.strictEqual(status, 1);
    assert.strictEqual(report.failure?.stage_id, 'profile_diagnostic');
    assert.ok(!calls().some(call => call.tool === 'compiler' || call.args[0] === 'xtask'));
  });

  test('canonical editor success cannot reuse the earlier host receipt', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'stale_host' });
    assert.strictEqual(status, 1);
    assert.strictEqual(report.failure?.stage_id, 'editor_suite');
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-package'));
  });

  test('capture wrapper preserves a native signal and never starts the editor suite', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'signal' });
    assert.strictEqual(status, 143);
    assert.strictEqual(report.failure?.native_status, null);
    assert.strictEqual(report.failure?.native_signal, 'SIGTERM');
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
  });

  test('capture wrapper bounds native output and retains incomplete evidence', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'output' });
    assert.notStrictEqual(status, 0);
    assert.strictEqual(report.failure?.kind, 'output_limit');
    assert.strictEqual(report.failure?.stage_id, 'rust_full');
    assert.strictEqual(report.full_development_ready, false);
    for (const stage of report.stages) if (stage.log) assert.ok(fs.statSync(stage.log).size <= 64 * 1024);
  });

  test('native failure status survives a refused stage-log write', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'log-write' });
    assert.strictEqual(status, 37);
    assert.strictEqual(report.failure?.native_status, 37);
    assert.strictEqual(report.failure?.stage_id, 'rust_full');
    assert.strictEqual(report.full_development_ready, false);
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
  });

  test('terminal error remains actionable after a long native build while logs stay bounded and redacted', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'long-rust' });
    assert.strictEqual(status, 37);
    assert.strictEqual(report.failure?.native_status, 37);
    const failedStage = report.stages.find(stage => stage.stage_id === 'rust_full');
    assert.ok(failedStage?.log);
    const bytes = fs.readFileSync(failedStage.log);
    assert.ok(bytes.length <= 64 * 1024);
    const text = bytes.toString('utf8');
    assert.ok(text.includes('original terminal native aggregate failure'));
    assert.ok(text.includes('example.invalid'));
    assert.ok(!text.includes('private-user'));
    assert.ok(!text.includes('private-password'));
    assert.strictEqual(Buffer.from(text).compare(bytes), 0);
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
  });

  for (const signal of ['SIGINT', 'SIGTERM']) {
    test('public coordinator ' + signal + ' cancels its actual owned stage and preserves earlier evidence', async () => {
      const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'interrupt:' + signal });
      assert.strictEqual(status, signal === 'SIGINT' ? 130 : 143);
      assert.strictEqual(report.failure?.kind, 'cancelled');
      assert.strictEqual(report.failure?.stage_id, 'rust_full');
      assert.strictEqual(report.stages.find(stage => stage.stage_id === 'profile_diagnostic')?.status, 'pass');
      assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
    });
  }

  test('native proxy failure redacts URL credentials before retaining bounded transcripts', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'proxy' });
    assert.strictEqual(status, 37);
    assert.strictEqual(report.failure?.native_status, 37);
    const retained = fs.readdirSync(report.evidence_directory).map(name => fs.readFileSync(path.join(report.evidence_directory, name), 'utf8')).join('\n');
    assert.ok(retained.includes('original native failure'));
    assert.ok(retained.includes('example.invalid'));
    assert.ok(!retained.includes('private-user'));
    assert.ok(!retained.includes('private-password'));
  });

  test('attempt deadline terminates the current native stage before subsequent work', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'timeout' }, 6000);
    assert.notStrictEqual(status, 0);
    assert.strictEqual(report.failure?.kind, 'timeout');
    assert.strictEqual(report.failure?.stage_id, 'rust_full');
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
  });

  test('one attempt deadline spans several successful commands instead of resetting for each command', async () => {
    const { report, status } = await invoke({ RIPR_FULL_CONTROL_FAIL: 'distributed-delay' }, 6000);
    assert.notStrictEqual(status, 0);
    assert.strictEqual(report.full_development_ready, false);
    assert.strictEqual(report.stages.find(stage => stage.stage_id === 'activation')?.status, 'pass');
    assert.ok(calls().length >= 3);
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-test-e2e'));
    assert.ok(!calls().some(call => call.args.join(' ') === 'xtask vscode-package'));
  });

  test('unsupported native platform refuses before loading a process owner', async () => {
    const api = require(diagnosticSource) as { runFullDevelopment: (options: Record<string, unknown>) => Promise<{ report: FullResult; exitCode: number }> };
    const result = await api.runFullDevelopment({ root, env, platform: 'win32' });
    assert.strictEqual(result.exitCode, 1);
    assert.strictEqual(result.report.failure?.stage_id, 'preflight');
    assert.strictEqual(calls().length, 0);
  });
});
