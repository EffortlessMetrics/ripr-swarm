// Offline behavior controls for explicit provisioning. Tool executables are
// fixtures; passing this suite does not establish a usable Rust/editor runtime.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const { test } = require('node:test');
const { parseArgs, provision, compilerIdentity, nativeRun } = require('./provision-dev.cjs');

const SOURCE_ROOT = path.resolve(__dirname, '..');
const FIXTURE_ARCH = process.arch === 'arm64' ? 'arm64' : 'x64';
const HOST = (FIXTURE_ARCH === 'arm64' ? 'aarch64' : 'x86_64') + '-unknown-linux-gnu';
const PIN = '1.95.0';
const STABLE = '1.99.1';
const COMPONENTS = ['clippy', 'llvm-tools-preview', 'rust-src', 'rustfmt'];
const NATIVE = process.platform === 'linux';
const UNIX = process.platform !== 'win32';
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

// One small native tool model owns independent installed state. Unknown commands
// fail so a new build/download path cannot silently become a successful no-op.
function nativeToolMain() {
  const fs = require('node:fs');
  const path = require('node:path');
  const crypto = require('node:crypto');
  const { spawnSync } = require('node:child_process');
  const statePath = process.env.RIPR_PROVISION_TEST_STATE;
  const state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  const args = process.argv.slice(2);
  const command = path.basename(process.argv[1]);
  const save = () => fs.writeFileSync(statePath, JSON.stringify(state));
  const fail = (message, code = 91) => {
    process.stderr.write(message + '\n');
    save();
    process.exit(code);
  };
  const output = text => { save(); process.stdout.write(text + '\n'); };
  const write = (name, text, mode) => {
    fs.mkdirSync(path.dirname(name), { recursive: true });
    fs.writeFileSync(name, text, mode === undefined ? undefined : { mode });
  };
  const executable = name => write(name, state.toolScript, 0o755);
  const exact = version => version + '-' + state.host;
  const selectedKey = selector => selector === 'stable' ? 'stable' :
    (/^\d+\.\d+\.\d+$/.test(selector) ? exact(selector) : selector);
  const sdk = () => {
    const key = path.basename(path.dirname(path.dirname(process.argv[1])));
    if (!state.sdks[key]) fail('fixture SDK is not installed: ' + key);
    return { key, value: state.sdks[key],
      root: path.join(process.env.RUSTUP_HOME, 'toolchains', key) };
  };
  const install = (selector, components) => {
    const key = selectedKey(selector);
    const release = key === 'stable' ? state.remoteStable : key.split('-')[0];
    const current = state.sdks[key] ?? { release, components: [] };
    current.release = release;
    current.components = [...new Set([...current.components, ...components])].sort();
    state.sdks[key] = current;
    const root = path.join(process.env.RUSTUP_HOME, 'toolchains', key);
    for (const name of ['rustc', 'cargo']) executable(path.join(root, 'bin', name));
    if (current.components.includes('rust-src')) {
      write(path.join(root, 'lib/rustlib/src/rust/library/core/src/lib.rs'), '// fixture source\n');
    }
    if (current.components.includes('llvm-tools-preview')) {
      for (const name of ['llvm-cov', 'llvm-profdata']) {
        executable(path.join(root, 'lib/rustlib', state.host, 'bin', name));
      }
    }
  };
  state.events.push({ command, args, cwd: process.cwd(),
    env: Object.fromEntries(['CARGO_HOME', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN',
      'RUSTUP_AUTO_INSTALL', 'RUSTC', 'CARGO_BUILD_RUSTC', 'RUSTC_WRAPPER',
      'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC_WRAPPER',
      'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'TAR_OPTIONS'].map(key => [key, process.env[key] ?? null])) });
  save();
  if (state.failure?.command === command
    && (!state.failure.includes || args.includes(state.failure.includes))) {
    fail(state.failure.message ?? ('controlled native failure: ' + command), state.failure.status);
  }

  if (command === 'rustup') {
    if (args[0] === '--version') return output('rustup 1.29.1 (fixture)');
    if (args[0] === 'which') {
      const selector = args[args.indexOf('--toolchain') + 1];
      const key = selectedKey(selector);
      if (!state.sdks[key]) fail('toolchain is not installed: ' + selector, 1);
      const tool = args[args.length - 1];
      if (!['rustc', 'cargo'].includes(tool)) fail('unexpected which tool: ' + tool);
      return output(path.join(process.env.RUSTUP_HOME, 'toolchains', key, 'bin', tool));
    }
    if (args[0] === 'toolchain' && args[1] === 'install') {
      const selector = args[2];
      const components = [];
      args.forEach((arg, index) => {
        if (arg === '--component') components.push(...args[index + 1].split(','));
      });
      install(selector, components);
      if (selector === 'stable') state.stableRefreshes++;
      return output('fixture toolchain installed: ' + selector);
    }
    if (args[0] === 'component' && args[1] === 'add') {
      const at = args.indexOf('--toolchain');
      const selector = args[at + 1];
      if (!state.sdks[selectedKey(selector)]) fail('components need an installed SDK');
      const components = args.slice(2).filter((arg, index) =>
        index + 2 !== at && index + 2 !== at + 1);
      install(selector, components.flatMap(value => value.split(',')));
      return output('fixture components installed');
    }
    if (args[0] === 'component' && args[1] === 'list') {
      const selector = args[args.indexOf('--toolchain') + 1];
      const installed = state.sdks[selectedKey(selector)];
      if (!installed) fail('components need an installed SDK');
      return output(installed.components.map(value => value + ' (installed)').join('\n'));
    }
  }
  if (command === 'rustc') {
    const selected = sdk();
    if (args.join(' ') === '--print sysroot') return output(selected.root);
    if (args[0] === '-vV') return output('rustc ' + selected.value.release + ' (fixture)\n'
      + 'binary: rustc\ncommit-hash: same-release-' + selected.value.release + '\n'
      + 'host: ' + state.host + '\nrelease: ' + selected.value.release + '\nLLVM version: 21.1.0');
  }
  if (command === 'cargo') {
    const selected = sdk();
    if (args[0] === '-vV' || args[0] === '--version') {
      return output('cargo ' + selected.value.release + ' (fixture)');
    }
    if (['fmt', 'clippy'].includes(args[0]) && args.includes('--version')) {
      const component = args[0] === 'fmt' ? 'rustfmt' : 'clippy';
      if (!selected.value.components.includes(component)) fail('component absent: ' + component, 65);
      return output(component + ' 1.0.0 (fixture)');
    }
    if (args[0] === 'fetch' && args.includes('--locked')) {
      if (args.includes('--offline') && !state.cargoCached) {
        fail('locked dependency is unavailable while using offline mode', 101);
      }
      const manifest = args[args.indexOf('--manifest-path') + 1];
      if (!fs.existsSync(manifest)) fail('manifest was not passed literally: ' + manifest);
      state.cargoCached = true;
      state.fetches++;
      return output('fixture locked dependencies available');
    }
    if (args[0] === 'nextest' && args.includes('--version')) {
      const child = spawnSync(path.join(process.env.CARGO_HOME, 'bin/cargo-nextest'),
        ['nextest', '--version'], { env: process.env, encoding: 'utf8' });
      process.stdout.write(child.stdout ?? '');
      process.stderr.write(child.stderr ?? '');
      process.exit(child.status ?? 1);
    }
  }
  if (command === 'llvm-cov' || command === 'llvm-profdata') {
    if (args[0] === '--version') return output('LLVM (fixture):\n  LLVM version 21.1.0');
  }
  if (command === 'cargo-nextest' && args.includes('--version')) {
    return output('cargo-nextest ' + process.env.RIPR_FIXTURE_NEXTEST_VERSION + ' (fixture)');
  }
  if (command === 'curl') {
    const url = args[args.length - 1];
    const destination = args[args.indexOf('--output') + 1];
    let bytes;
    if (url === 'https://api.github.com/repos/nextest-rs/nextest/releases/latest') {
      bytes = JSON.stringify({ tag_name: 'cargo-nextest-' + state.nextest });
    } else {
      const asset = /\/cargo-nextest-(\d+\.\d+\.\d+)-[^/]+\.(tar\.gz|sha256)$/.exec(url);
      if (!asset) fail('unexpected network destination in offline control: ' + url);
      const archive = JSON.stringify({ nextest: asset[1] });
      bytes = asset[2] === 'tar.gz' ? archive
        : (state.badChecksum ? '0'.repeat(64) : crypto.createHash('sha256').update(archive).digest('hex'));
    }
    write(destination, bytes);
    return output('fixture download completed');
  }
  if (command === 'tar') {
    const archive = args[args.indexOf('--file') + 1];
    const directory = args[args.indexOf('--directory') + 1];
    const version = JSON.parse(fs.readFileSync(archive, 'utf8')).nextest;
    if (args[args.length - 1] !== 'cargo-nextest') fail('unexpected archive member');
    write(path.join(directory, 'cargo-nextest'),
      state.toolScript.replace('\n', '\nprocess.env.RIPR_FIXTURE_NEXTEST_VERSION = '
        + JSON.stringify(version) + ';\n'), 0o755);
    return output('fixture verified archive extracted');
  }
  if (command === 'npm') {
    if (args[0] === '--version') return output('11.6.0');
    if (args[0] === 'ci') {
      const lockBytes = fs.readFileSync(path.join(process.cwd(), 'package-lock.json'));
      const lock = JSON.parse(lockBytes);
      const modules = path.join(process.cwd(), 'node_modules');
      fs.rmSync(modules, { recursive: true, force: true });
      for (const [location, metadata] of Object.entries(lock.packages)) {
        write(path.join(process.cwd(), location, 'package.json'), JSON.stringify(metadata));
      }
      write(path.join(modules, 'typescript/bin/tsc'), '// fixture compiler\n');
      write(path.join(modules, '.bin/tsc'), '// fixture tsc entrypoint\n', 0o755);
      write(path.join(modules, '.bin/vsce'), '// fixture vsce entrypoint\n', 0o755);
      state.npmInstalls++;
      state.installedLock = crypto.createHash('sha256').update(lockBytes).digest('hex');
      if (state.changeLockDuringInstall) {
        fs.appendFileSync(path.join(process.cwd(), 'package-lock.json'), '\n');
      }
      return output('fixture locked editor dependencies installed');
    }
    if (args[0] === 'ls' && args.includes('--offline')) {
      const lock = JSON.parse(fs.readFileSync(path.join(process.cwd(), 'package-lock.json')));
      for (const [location, expected] of Object.entries(lock.packages)) {
        let actual;
        try { actual = JSON.parse(fs.readFileSync(path.join(process.cwd(), location, 'package.json'))); }
        catch { fail('missing dependency: ' + location, 1); }
        if (actual.version !== expected.version) fail('invalid dependency: ' + location, 1);
      }
      return output('fixture locked dependency tree complete');
    }
    if (args.join(' ') === 'run setup:verify-editor') {
      if (!fs.existsSync(path.join(process.cwd(), 'node_modules/typescript/bin/tsc'))) {
        fail('editor smoke started without its compiler');
      }
      state.editorSmokes++;
      return output('fixture editor owner completed');
    }
  }
  if (command === 'xvfb-run' && args[0] === '-a') {
    const commandAt = args[1] === '-e' ? 3 : 1;
    if (commandAt === 3 && args[2] !== '/dev/stderr') fail('unexpected Xvfb diagnostic destination');
    if (state.xvfbFailure) {
      if (commandAt === 3) process.stderr.write(state.xvfbFailure.message + '\n');
      fail('xvfb-run: error: Xvfb failed to start', state.xvfbFailure.status);
    }
    const child = spawnSync(args[commandAt], args.slice(commandAt + 1), {
      env: { ...process.env, DISPLAY: ':87' }, encoding: 'utf8' });
    process.stdout.write(child.stdout ?? '');
    process.stderr.write(child.stderr ?? '');
    process.exit(child.status ?? 1);
  }
  if (command === 'ldconfig' && args[0] === '-p') {
    return output(state.libraries.map(name => '\t' + name + ' (libc6,'
      + state.libraryArch + ') => /fixture/lib/' + name).join('\n'));
  }
  if (['git', 'cc', 'make', 'pkg-config'].includes(command) && args[0] === '--version') {
    return output(command + ' (fixture)');
  }
  fail('unexpected fixture command: ' + command + ' ' + JSON.stringify(args));
}

function writeFile(filename, content, mode) {
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  fs.writeFileSync(filename, content, mode === undefined ? undefined : { mode });
}

function makeFixture(t, options = {}) {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-provision-test-'));
  t.after(() => fs.rmSync(temp, { recursive: true, force: true }));
  const root = path.join(temp, 'checkout with spaces');
  const cargoHome = path.join(temp, 'cargo home');
  const rustupHome = path.join(temp, 'rustup home');
  const bin = path.join(temp, 'native tools');
  const statePath = path.join(temp, 'tool-state.json');
  const toolScript = '#!' + process.execPath + '\n(' + nativeToolMain.toString() + ')();\n';
  const state = { host: HOST, remoteStable: STABLE, nextest: '0.9.121', toolScript,
    sdks: {}, stableRefreshes: 0, npmInstalls: 0, editorSmokes: 0,
    cargoCached: false, fetches: 0, events: [], failure: null,
    libraryArch: FIXTURE_ARCH === 'arm64' ? 'AArch64' : 'x86-64',
    libraries: ['libgtk-3.so.0', 'libnss3.so', 'libasound.so.2', 'libgbm.so.1',
      'libX11.so.6', 'libX11-xcb.so.1', 'libXss.so.1', 'libxkbfile.so.1', 'libsecret-1.so.0'] };
  writeFile(statePath, JSON.stringify(state));
  const put = (relative, content) => writeFile(path.join(root, relative), content);
  put('rust-toolchain.toml', '[toolchain]\nprofile = "minimal"\nchannel = "' + PIN
    + '"\ncomponents = ["rustfmt", "clippy"]\n');
  put('Cargo.toml', '[package]\nname = "provision-control"\nversion = "0.0.0"\nedition = "2024"\n');
  put('Cargo.lock', 'version = 4\n');
  put('.cargo/config.toml', '[env]\n' + ['TEMP', 'TMP', 'TMPDIR']
    .map(name => name + ' = { value = "target", relative = true, force = true }\n').join(''));
  put('.github/workflows/vscode-e2e.yml', 'jobs:\n  editor:\n    steps:\n'
    + '      - uses: actions/setup-node@' + 'b'.repeat(40) + '\n'
    + '        with:\n          node-version: ' + process.versions.node.split('.')[0] + '\n');
  put('.github/workflows/rust-gates.yml', 'jobs:\n  rust:\n    steps:\n'
    + '      - uses: taiki-e/install-action@' + 'a'.repeat(40) + '\n'
    + '        with:\n          tool: nextest\n'
    + '      - with:\n          components: llvm-tools-preview\n');
  put('editors/vscode/package.json', JSON.stringify({ name: 'provision-control',
    version: '0.0.0', engines: { vscode: '^1.105.0' } }));
  put('editors/vscode/package-lock.json', JSON.stringify({ lockfileVersion: 3,
    packages: { 'node_modules/typescript': { version: '6.0.3' },
      'node_modules/@vscode/test-electron': { version: '2.5.2' },
      'node_modules/@vscode/vsce': { version: '3.7.1' },
      'node_modules/fixture-transitive-dependency': { version: '1.0.0' } } }));
  for (const relative of ['tools/activate-rust.sh', 'tools/provision-dev.cjs',
    'editors/vscode/test/developerSetup.cjs']) {
    put(relative, fs.readFileSync(path.join(SOURCE_ROOT, relative)));
  }
  writeFile(path.join(cargoHome, 'bin/rustup'), toolScript, 0o755);
  writeFile(path.join(cargoHome, 'bin/cargo-nextest'),
    toolScript.replace('\n', '\nprocess.env.RIPR_FIXTURE_NEXTEST_VERSION = "0.9.121";\n'), 0o755);
  for (const name of ['curl', 'git', 'tar', 'cc', 'make', 'pkg-config', 'ldconfig',
    'npm', 'xvfb-run', 'Xvfb', 'xauth']) {
    if (name !== options.missingCommand) writeFile(path.join(bin, name), toolScript, 0o755);
  }
  for (const [name, actual] of [['bash', '/bin/bash'], ['timeout', NATIVE ? '/usr/bin/timeout' : null],
    ['node', process.execPath]]) {
    if (name !== options.missingCommand) {
      if (actual) fs.symlinkSync(actual, path.join(bin, name));
      else writeFile(path.join(bin, name), toolScript, 0o755);
    }
  }
  const env = {
    HOME: path.join(temp, 'home'),
    CARGO_HOME: cargoHome,
    RUSTUP_HOME: rustupHome,
    PATH: cargoHome + '/bin' + path.delimiter + bin,
    RIPR_PROVISION_TEST_STATE: statePath,
  };
  const deps = { root, env, platform: 'linux', arch: FIXTURE_ARCH,
    nodeVersion: process.versions.node, uid: 1000, glibcVersion: '2.39',
    print: () => {},
    run: (command, args, settings) => spawnSync(command, args, {
      cwd: settings.cwd, env: settings.env, encoding: 'utf8',
      timeout: Math.min(settings.timeoutMs ?? 10000, 10000), maxBuffer: 1024 * 1024,
    }) };
  const read = () => JSON.parse(fs.readFileSync(statePath, 'utf8'));
  const update = mutate => { const next = read(); mutate(next); fs.writeFileSync(statePath, JSON.stringify(next)); };
  const run = argv => provision(parseArgs(argv), deps);
  const cli = argv => spawnSync(process.execPath,
    [path.join(root, 'tools/provision-dev.cjs'), ...argv],
    { cwd: temp, env, encoding: 'utf8', timeout: 15000, maxBuffer: 1024 * 1024 });
  return { root, cargoHome, rustupHome, bin, env, deps, put, read, update, run, cli,
    manifest: path.join(root, 'Cargo.toml'),
    lock: path.join(root, 'editors/vscode/package-lock.json'),
    modules: path.join(root, 'editors/vscode/node_modules') };
}

function assertComponents(state, selectors, additional = []) {
  for (const selector of selectors) {
    assert.ok(state.sdks[selector], 'SDK should be installed: ' + selector);
    assert.deepEqual(state.sdks[selector].components, [...COMPONENTS, ...additional].sort(),
      'every selected SDK needs formatting, lint, source, and matching LLVM tools');
  }
}

function ownedProcessMain() {
  const fs = require('node:fs');
  const path = require('node:path');
  const { spawn } = require('node:child_process');
  const [role, directory, behavior = 'ignore-term'] = process.argv.slice(2);
  process.on('SIGTERM', () => {
    fs.writeFileSync(path.join(directory, role + '.term'), 'observed');
    if (role === 'parent' && behavior === 'parent-exits') process.exit(0);
  });
  process.on('SIGINT', () => fs.writeFileSync(path.join(directory, role + '.int'), 'observed'));
  const stat = fs.readFileSync('/proc/self/stat', 'utf8');
  const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
  // Read our actual visible proc identity while alive; it can differ from the
  // namespace PID used by Node and kill(2). Do not guess /proc/<process.pid>.
  const identityFile = path.join(directory, role + '.pid');
  fs.writeFileSync(identityFile + '.partial', JSON.stringify({
    pid: process.pid, procPid: Number(fs.readlinkSync('/proc/self')),
    startTime: fields[19], procGroup: Number(fields[2]),
  }));
  fs.renameSync(identityFile + '.partial', identityFile);
  if (role === 'parent') {
    spawn(process.execPath, [process.argv[1], 'descendant', directory, behavior], { stdio: 'ignore' });
  }
  const pulse = path.join(directory, role + '.pulse');
  setInterval(() => {
    fs.writeFileSync(pulse + '.partial', String(Date.now()));
    fs.renameSync(pulse + '.partial', pulse);
  }, 20);
}

function terminalState(identity) {
  try { process.kill(identity.pid, 0); }
  catch (error) {
    if (error.code === 'ESRCH') return { terminal: true, state: 'gone' };
    return { terminal: false, state: 'native existence unobserved: ' + error.code };
  }
  try {
    const stat = fs.readFileSync('/proc/' + identity.procPid + '/stat', 'utf8');
    const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
    if (fields[19] !== identity.startTime) return { terminal: false, state: 'identity changed' };
    const state = fields[0];
    return { terminal: state === 'Z' || state === 'X', state };
  } catch (error) {
    // kill(2) observed a native PID. A missing/hidden proc entry is unknown,
    // not proof of death, including when the proc mount uses ancestor PIDs.
    return { terminal: false, state: 'proc state unobserved: ' + error.code };
  }
}

async function until(predicate, message, budget = 5000) {
  const deadline = Date.now() + budget;
  while (!predicate()) {
    if (Date.now() >= deadline) assert.fail(message);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
}

function makeProcessFixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-provision-process-'));
  const program = path.join(root, 'owned-process.cjs');
  writeFile(program, '(' + ownedProcessMain.toString() + ')();\n');
  const pids = () => ['parent', 'descendant'].map(role => {
    const filename = path.join(root, role + '.pid');
    return { role, ...(fs.existsSync(filename)
      ? JSON.parse(fs.readFileSync(filename, 'utf8')) : { pid: null }) };
  });
  t.after(async () => {
    for (const identity of pids()) {
      if (identity.pid && !terminalState(identity).terminal) {
        assert.notEqual(terminalState(identity).state, 'identity changed',
          'do not terminate a reused PID');
        // These native IDs came from our fresh, owned fixture, never from a
        // pathname guess. Cleanup must work even when proc visibility is unknown.
        try { process.kill(identity.pid, 'SIGKILL'); }
        catch (error) { if (error.code !== 'ESRCH') throw error; }
      }
    }
    await until(() => pids().every(identity => !identity.pid || terminalState(identity).terminal),
      'owned fixture process remained live after cleanup', 2000);
    fs.rmSync(root, { recursive: true, force: true });
  });
  const observe = () => {
    const observations = pids().map(identity => {
      const { role, pid, procPid } = identity;
      assert.ok(pid > 0 && procPid > 0, role + ' never reached the intended fixture');
      assert.equal(fs.existsSync(path.join(root, role + '.term')), true,
        role + ' did not observe the owned group TERM');
      const observation = terminalState(identity);
      assert.equal(observation.terminal, true, role + ' remains live after nativeRun returned');
      return role + ': ' + observation.state + ' (native PID ' + pid + ', proc PID ' + procPid + ')';
    });
    // Linux init may retain an orphan zombie. Distinguish that terminal state
    // from native reaping; neither is a running descendant.
    t.diagnostic('Owned process terminal states: ' + observations.join(', '));
  };
  return { root, program, pids, observe,
    env: { PATH: '/usr/bin:/bin', HOME: root } };
}

async function cancelProcessFixture(t, behavior = 'ignore-term') {
  const f = makeProcessFixture(t);
  const driver = path.join(f.root, 'driver.cjs');
  const resultFile = path.join(f.root, 'result.json');
  writeFile(driver, "'use strict';\nconst fs = require('node:fs');\n"
    + 'const { nativeRun } = require(' + JSON.stringify(path.join(__dirname, 'provision-dev.cjs')) + ');\n'
    + 'nativeRun(process.execPath, ' + JSON.stringify([f.program, 'parent', f.root, behavior]) + ', '
    + JSON.stringify({ cwd: f.root, env: f.env, capture: true, timeoutMs: 8000, killAfterMs: 150 })
    + ').then(result => { fs.writeFileSync(' + JSON.stringify(resultFile)
    + ', JSON.stringify(result)); process.exitCode = result.status; })'
    + '.catch(error => { console.error(error); process.exitCode = 1; });\n');
  const child = spawn(process.execPath, [driver], { cwd: f.root, env: f.env, stdio: 'ignore' });
  let closed = false;
  const exit = new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('close', (status, signal) => { closed = true; resolve({ status, signal }); });
  });
  t.after(async () => {
    if (!closed) child.kill('SIGKILL');
    await exit;
  });
  const pulseFile = path.join(f.root, 'descendant.pulse');
  await until(() => f.pids().every(({ pid }) => pid) && fs.existsSync(pulseFile),
    'descendant never reached cancellation control');
  const initialPulse = fs.readFileSync(pulseFile, 'utf8');
  await until(() => fs.readFileSync(pulseFile, 'utf8') !== initialPulse,
    'descendant heartbeat never advanced before cancellation');
  const [parent, descendant] = f.pids();
  assert.equal(descendant.procGroup, parent.procGroup,
    'the counterexample must stay in the command process group');
  assert.equal(terminalState(descendant).terminal, false,
    'the liveness oracle must observe the running descendant');
  assert.equal(child.kill('SIGTERM'), true);
  await until(() => closed, 'cancellation driver did not exit after group cleanup');
  const nativeExit = await exit;
  assert.equal(nativeExit.status, 143);
  assert.equal(nativeExit.signal, null);
  const result = JSON.parse(fs.readFileSync(resultFile, 'utf8'));
  assert.equal(result.cancelled, true);
  assert.equal(result.signal, 'SIGTERM');
  assert.equal(result.status, 143);
  return { ...f, pulseFile };
}

test('Given an invalid option, parsing refuses instead of silently selecting a partial setup', () => {
  for (const args of [['--unknown'], ['--ensure', '--ensure'], ['--rust-only', '--rust-only'],
    ['--nextest-version'], ['--nextest-version', 'latest'],
    ['--nextest-version', '1.2'], ['--nextest-version', '1.2.3; touch surprise']]) {
    assert.throws(() => parseArgs(args), 'expected refusal: ' + JSON.stringify(args));
  }
});

test('Given an exact compiler identity, freezing excludes nightly, partial, and absent identities', () => {
  assert.deepEqual(compilerIdentity('rustc 1.99.1 (fixture)\nhost: ' + HOST + '\nrelease: 1.99.1\n'),
    { release: STABLE, host: HOST, exact: STABLE + '-' + HOST });
  for (const text of ['', 'release: 1.99.1\n', 'host: ' + HOST + '\nrelease: 1.99\n',
    'host: ' + HOST + '\nrelease: 1.99.1-nightly\n']) {
    assert.throws(() => compilerIdentity(text));
  }
});

test('Given a wrong Node major, setup refuses before any installation', { skip: !UNIX }, async t => {
  const f = makeFixture(t);
  f.deps.nodeVersion = (Number(process.versions.node.split('.')[0]) + 1) + '.0.0';
  await assert.rejects(f.run([]), /Node|node/i);
  assert.equal(f.read().events.length, 0);
  assert.deepEqual(f.read().sdks, {});
  assert.equal(fs.existsSync(f.modules), false);
});

test('Given a rootless headless host without Xvfb, full setup fails before Rust or npm changes',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t, { missingCommand: 'Xvfb' });
    await assert.rejects(f.run([]), /Xvfb|display|native|system/i);
    assert.deepEqual(f.read().sdks, {});
    assert.equal(f.read().npmInstalls, 0);
    assert.equal(fs.existsSync(f.modules), false);
  });

test('Given a missing native library, full setup fails before downloads even with a display configured',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.env.DISPLAY = ':1';
    f.update(state => { state.libraries = state.libraries.filter(name => name !== 'libnss3.so'); });
    await assert.rejects(f.run([]), /libnss3|native librar/i);
    assert.deepEqual(f.read().sdks, {});
    assert.equal(f.read().npmInstalls, 0);
    assert.equal(f.read().events.some(event => event.command === 'curl'), false);
  });

test('Given native libraries for another architecture, their names do not establish editor prerequisites',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.update(state => { state.libraryArch = FIXTURE_ARCH === 'arm64' ? 'x86-64' : 'AArch64'; });
    await assert.rejects(f.run([]), /native librar|libgtk|architecture/i);
    assert.deepEqual(f.read().sdks, {});
    assert.equal(f.read().npmInstalls, 0);
  });

test('Given unreadable requirement authority, setup does not guess a Rust version',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    fs.rmSync(path.join(f.root, 'rust-toolchain.toml'));
    await assert.rejects(f.run(['--rust-only']), /source|requirement|toolchain|authority/i);
    assert.equal(f.read().events.length, 0);
    assert.deepEqual(f.read().sdks, {});
  });

test('Given a new image, provisioning installs pinned and frozen stable SDKs with every required component',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    const report = await f.run(['--rust-only']);
    assertComponents(f.read(), [PIN + '-' + HOST, 'stable', STABLE + '-' + HOST]);
    assert.equal(report.rust_pin, PIN + '-' + HOST);
    assert.equal(report.stable_exact, STABLE + '-' + HOST);
    assert.equal(report.full_development_ready, false);
    assert.equal(report.fresh_task, 'not_proven');
    assert.equal(report.editor_host, 'not_run');
    assert.equal(f.read().npmInstalls, 0);
    assert.equal(f.read().cargoCached, true);
  });

test('Given a changed repository pin, ensure installs that SDK and fetches with its exact compiler',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run(['--rust-only']);
    f.put('rust-toolchain.toml', '[toolchain]\nprofile = "minimal"\nchannel = "1.99.0"\n'
      + 'components = ["rustfmt", "clippy", "rust-analyzer"]\n');
    const report = await f.run(['--rust-only', '--ensure']);
    assert.equal(report.rust_pin, '1.99.0-' + HOST);
    assertComponents(f.read(), ['1.99.0-' + HOST, 'stable', STABLE + '-' + HOST], ['rust-analyzer']);
    const fetches = f.read().events.filter(event => event.command === 'cargo' && event.args[0] === 'fetch');
    assert.ok(fetches.length > 0, 'the dependency consumer must run');
    const current = fetches[fetches.length - 1];
    assert.equal(current.env.RUSTUP_TOOLCHAIN, '1.99.0-' + HOST);
    assert.equal(current.env.RUSTC, path.join(f.rustupHome, 'toolchains', '1.99.0-' + HOST, 'bin/rustc'));
    assert.equal(current.args[current.args.indexOf('--manifest-path') + 1], f.manifest);
  });

test('Given a later upstream stable release, ensure preserves the image release until explicit provisioning',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run(['--rust-only']);
    f.update(state => { state.remoteStable = '1.100.0'; });
    const reused = await f.run(['--rust-only', '--ensure']);
    assert.equal(reused.stable_exact, STABLE + '-' + HOST);
    assert.equal(f.read().stableRefreshes, 1);
    assert.equal(f.read().sdks['1.100.0-' + HOST], undefined);
    const refreshed = await f.run(['--rust-only']);
    assert.equal(refreshed.stable_exact, '1.100.0-' + HOST);
    assertComponents(f.read(), ['1.100.0-' + HOST]);
    assert.equal(f.read().stableRefreshes, 2);
  });

test('Given missing components on installed SDKs, ensure repairs them without advancing stable',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run(['--rust-only']);
    f.update(state => {
      state.sdks[PIN + '-' + HOST].components = ['rustfmt'];
      state.sdks[STABLE + '-' + HOST].components = ['clippy', 'rust-src', 'rustfmt'];
    });
    await f.run(['--ensure', '--rust-only']);
    assertComponents(f.read(), [PIN + '-' + HOST, 'stable', STABLE + '-' + HOST]);
    assert.equal(f.read().stableRefreshes, 1);
  });

test('Given matching locked editor dependencies, ensure reuses them without rerunning npm ci',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    const installed = await f.run([]);
    assert.equal(f.read().npmInstalls, 1);
    assert.equal(f.read().editorSmokes, 1);
    assert.equal(installed.editor_host, 'passed');
    assert.equal(installed.full_development_ready, false);
    assert.equal(installed.fresh_task, 'not_proven');
    const lockHash = sha256(fs.readFileSync(f.lock));
    const before = f.read().events.length;
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 1);
    assert.equal(f.read().installedLock, lockHash);
    assert.equal(f.read().stableRefreshes, 1);
    const writes = f.read().events.slice(before).filter(event => event.command === 'rustup'
      && (event.args[0] === 'toolchain' || event.args.join(' ').startsWith('component add')));
    assert.deepEqual(writes, [], 'healthy ensure must not mutate Rust installations');
  });

test('Given caller tar options, only the editor smoke gains ownership-safe extraction and the caller stays unchanged',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.env.TAR_OPTIONS = '--warning=no-unknown-keyword';
    const original = { ...f.env };
    await f.run([]);
    assert.deepEqual(f.env, original);
    const smoke = f.read().events.find(event => event.command === 'npm'
      && event.args.join(' ') === 'run setup:verify-editor');
    assert.ok(smoke, 'the delegated editor smoke must execute');
    assert.equal(smoke.env.TAR_OPTIONS, '--warning=no-unknown-keyword --no-same-owner');
    const install = f.read().events.find(event => event.command === 'npm' && event.args[0] === 'ci');
    assert.ok(install);
    assert.equal(install.env.TAR_OPTIONS, original.TAR_OPTIONS);
  });

test('Given a failing Xvfb or canonical editor smoke, setup retains native diagnostics and the original status',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.update(state => {
      state.xvfbFailure = { status: 78, message: 'fixture native X socket could not create a listener' };
    });
    await assert.rejects(f.run([]), error => error.exitCode === 78
      && /native X socket could not create a listener/.test(error.message));
    assert.equal(f.read().editorSmokes, 0);
    assert.equal(f.read().fetches, 0);
    f.update(state => {
      state.xvfbFailure = null;
      state.failure = { command: 'npm', includes: 'run', status: 79,
        message: 'fixture canonical editor prerequisite failed' };
    });
    await assert.rejects(f.run(['--ensure']), error => error.exitCode === 79
      && /canonical editor prerequisite failed/.test(error.message));
    assert.equal(f.read().npmInstalls, 1, 'the failed smoke must not invalidate a complete locked install');
    assert.equal(f.read().fetches, 0);
  });

test('Given a changed locked source at the same version, ensure reinstalls before the editor smoke',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run([]);
    const lock = JSON.parse(fs.readFileSync(f.lock, 'utf8'));
    lock.packages['node_modules/typescript'].resolved = 'https://registry.invalid/reviewed-typescript-6.0.3.tgz';
    fs.writeFileSync(f.lock, JSON.stringify(lock));
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 2);
    assert.equal(f.read().installedLock, sha256(fs.readFileSync(f.lock)));
    const installed = JSON.parse(fs.readFileSync(path.join(f.modules, 'typescript/package.json')));
    assert.equal(installed.version, '6.0.3');
    assert.equal(installed.resolved, 'https://registry.invalid/reviewed-typescript-6.0.3.tgz');
  });

test('Given damaged installed dependencies, a matching stamp cannot suppress repair',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run([]);
    fs.rmSync(path.join(f.modules, 'typescript/bin/tsc'));
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 2);
    assert.equal(fs.existsSync(path.join(f.modules, 'typescript/bin/tsc')), true);
  });

test('Given different installed dependency versions, a matching stamp cannot prove a locked install',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run([]);
    writeFile(path.join(f.modules, '@vscode/test-electron/package.json'), '{"version":"0.0.0"}');
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 2);
    assert.equal(JSON.parse(fs.readFileSync(path.join(f.modules, '@vscode/test-electron/package.json'))).version, '2.5.2');
  });

test('Given a deleted packaging entrypoint or transitive dependency, ensure repairs the editor tree',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run([]);
    fs.rmSync(path.join(f.modules, '.bin/vsce'));
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 2);
    assert.equal(fs.existsSync(path.join(f.modules, '.bin/vsce')), true);
    fs.rmSync(path.join(f.modules, 'fixture-transitive-dependency/package.json'));
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 3);
    assert.equal(fs.existsSync(path.join(f.modules, 'fixture-transitive-dependency/package.json')), true);
  });

test('Given a lockfile changed during npm ci, no reusable install stamp or editor success is published',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.update(state => { state.changeLockDuringInstall = true; });
    await assert.rejects(f.run([]), /chang|lock/i);
    assert.equal(f.read().npmInstalls, 1, 'the lock change must occur inside the install control');
    assert.equal(f.read().editorSmokes, 0);
    assert.equal(fs.existsSync(path.join(f.modules, '.ripr-provision-lock.json')), false);
  });

test('Given a native SDK installer failure, its status survives and downstream installation never starts',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.update(state => { state.failure = { command: 'rustup', includes: 'install', status: 73 }; });
    await assert.rejects(f.run([]), error => error.exitCode === 73);
    assert.deepEqual(f.read().sdks, {});
    assert.equal(f.read().npmInstalls, 0);
    assert.equal(f.read().fetches, 0);
    assert.equal(f.read().editorSmokes, 0);
  });

test('Given a failed locked reinstall, ensure cannot report editor success or reuse the failed state',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run([]);
    f.put('editors/vscode/package.json', JSON.stringify({ name: 'changed-control',
      version: '0.0.0', engines: { vscode: '^1.105.0' } }));
    f.update(state => { state.failure = { command: 'npm', includes: 'ci', status: 74 }; });
    await assert.rejects(f.run(['--ensure']), error => error.exitCode === 74);
    assert.equal(f.read().editorSmokes, 1);
    f.update(state => { state.failure = null; });
    await f.run(['--ensure']);
    assert.equal(f.read().npmInstalls, 2);
  });

test('Given a mismatched Nextest download digest, the old executable remains intact and no archive is executed',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    const nextest = path.join(f.cargoHome, 'bin/cargo-nextest');
    const before = sha256(fs.readFileSync(nextest));
    f.update(state => { state.badChecksum = true; });
    await assert.rejects(f.run([]), /SHA-256|checksum/i);
    assert.equal(sha256(fs.readFileSync(nextest)), before);
    assert.equal(f.read().events.some(event => event.command === 'tar'), false);
    assert.equal(f.read().npmInstalls, 0);
    assert.equal(f.read().editorSmokes, 0);
  });

test('Given an empty Cargo cache, ensure fetches the locked dependencies and records usable cached state',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    await f.run(['--ensure', '--rust-only']);
    assert.equal(f.read().cargoCached, true);
    const fetches = f.read().events.filter(event => event.command === 'cargo' && event.args[0] === 'fetch');
    assert.equal(fetches.length, 2);
    assert.ok(fetches[0].args.includes('--offline'));
    assert.equal(fetches[1].args.includes('--offline'), false);
  });

test('Given an offline Cargo failure unrelated to the cache, setup preserves its status without an online retry',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.update(state => { state.failure = { command: 'cargo', includes: '--offline', status: 76 }; });
    await assert.rejects(f.run(['--ensure', '--rust-only']), error => error.exitCode === 76);
    const fetches = f.read().events.filter(event => event.command === 'cargo' && event.args[0] === 'fetch');
    assert.equal(fetches.length, 1);
    assert.ok(fetches[0].args.includes('--offline'));
    assert.equal(f.read().cargoCached, false);
  });

test('Given a cleanup error after a native failure, the first native status still determines failure',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    f.update(state => { state.failure = { command: 'rustup', includes: 'install', status: 77 }; });
    const realRemove = fs.rmSync;
    let cleanupAttempted = false;
    let warningAttempted = false;
    f.deps.print = message => {
      if (message.startsWith('Provisioning temporary files retained:')) {
        warningAttempted = true;
        throw new Error('controlled diagnostic sink failure');
      }
    };
    fs.rmSync = (filename, options) => {
      if (String(filename).startsWith(path.join(f.root, 'target', 'provision-'))) {
        cleanupAttempted = true;
        throw Object.assign(new Error('controlled cleanup refusal'), { code: 'EACCES' });
      }
      return realRemove(filename, options);
    };
    try {
      await assert.rejects(f.run(['--rust-only']), error => error.exitCode === 77);
      assert.equal(cleanupAttempted, true);
      assert.equal(warningAttempted, true);
    } finally { fs.rmSync = realRemove; }
  });

test('Given conflicting compiler overrides, setup refuses before installation and preserves the caller environment',
  { skip: !UNIX }, async t => {
    const f = makeFixture(t);
    Object.assign(f.env, { RUSTUP_TOOLCHAIN: 'nightly', RUSTC: '/missing compiler/rustc',
      CARGO_BUILD_RUSTC: '/another compiler/rustc', RUSTC_WRAPPER: '/wrapper',
      RUSTC_WORKSPACE_WRAPPER: '/workspace-wrapper', CARGO_BUILD_RUSTC_WRAPPER: '/cargo-wrapper',
      CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER: '/cargo-workspace-wrapper' });
    const original = { ...f.env };
    await assert.rejects(f.run(['--rust-only']), /wrapper|compiler|RUSTC/i);
    assert.deepEqual(f.env, original);
    assert.deepEqual(f.read().sdks, {});
    assert.equal(f.read().npmInstalls, 0);
    assert.equal(f.read().fetches, 0);
  });

test('Given an explicit different compiler without wrappers, setup refuses before downloading either SDK',
  { skip: !UNIX }, async t => {
    for (const name of ['RUSTC', 'CARGO_BUILD_RUSTC']) {
      const f = makeFixture(t);
      f.env[name] = '/missing compiler/rustc';
      const original = { ...f.env };
      await assert.rejects(f.run(['--rust-only']), /compiler|RUSTC/i);
      assert.deepEqual(f.env, original);
      assert.deepEqual(f.read().sdks, {});
      assert.equal(f.read().events.some(event => event.command === 'curl'), false);
    }
  });

test('Given CLI help or invalid arguments, native children never begin provisioning',
  { skip: !UNIX }, t => {
    const f = makeFixture(t);
    const help = f.cli(['--help']);
    assert.equal(help.status, 0, help.stderr);
    assert.match(help.stdout, /ensure/);
    const bad = f.cli(['--unknown-option']);
    assert.notEqual(bad.status, 0);
    assert.match(bad.stderr, /unknown|usage|option/i);
    assert.equal(f.read().events.length, 0);
    assert.deepEqual(f.read().sdks, {});
  });

test('Given a CLI invocation from another directory, the checked-out pin and space-containing homes drive installation',
  { skip: !NATIVE }, t => {
    const f = makeFixture(t);
    const result = f.cli(['--rust-only']);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assertComponents(f.read(), [PIN + '-' + HOST, 'stable', STABLE + '-' + HOST]);
    assert.equal(f.read().cargoCached, true);
    assert.equal(f.read().npmInstalls, 0);
  });

test('Given a native CLI installer failure, the process exits with that status without a success claim',
  { skip: !NATIVE }, t => {
    const f = makeFixture(t);
    f.update(state => { state.failure = { command: 'rustup', includes: 'install', status: 75 }; });
    const result = f.cli(['--rust-only']);
    assert.equal(result.status, 75, result.stdout + result.stderr);
    assert.deepEqual(f.read().sdks, {});
    assert.equal(f.read().fetches, 0);
    assert.doesNotMatch(result.stdout, /"full_development_ready"\s*:\s*true/);
  });

test('Given a deadline and TERM-resistant descendants, nativeRun returns after the owned group is terminal',
  { skip: !NATIVE, timeout: 8000 }, async t => {
    const f = makeProcessFixture(t);
    const termListeners = process.listenerCount('SIGTERM');
    const intListeners = process.listenerCount('SIGINT');
    const result = await nativeRun(process.execPath, [f.program, 'parent', f.root],
      { cwd: f.root, env: f.env, capture: true, timeoutMs: 1500, killAfterMs: 150 });
    assert.equal(result.timedOut, true);
    assert.notEqual(result.status, 0);
    f.observe();
    assert.equal(process.listenerCount('SIGTERM'), termListeners);
    assert.equal(process.listenerCount('SIGINT'), intListeners);
  });

test('Given SIGTERM during a native command, cancellation preserves status 143 and terminates owned descendants',
  { skip: !NATIVE, timeout: 10000 }, async t => {
    const f = await cancelProcessFixture(t);
    f.observe();
  });

test('Given a parent that exits on TERM, cancellation still stops the same-group descendant heartbeat before returning',
  { skip: !NATIVE, timeout: 10000 }, async t => {
    const f = await cancelProcessFixture(t, 'parent-exits');
    const pulseAtReturn = fs.readFileSync(f.pulseFile, 'utf8');
    await new Promise(resolve => setTimeout(resolve, 350));
    assert.equal(fs.readFileSync(f.pulseFile, 'utf8'), pulseAtReturn,
      'the descendant heartbeat continued after nativeRun returned');
    f.observe();
  });
