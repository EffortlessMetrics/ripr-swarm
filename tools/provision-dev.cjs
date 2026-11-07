// Explicit online provisioning before Cargo/npm dependencies exist (#13437).
// Ordinary activation and setup:check remain separate, non-installing owners.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const cp = require('node:child_process');
const { collectDeveloperSetup } = require('../editors/vscode/test/developerSetup.cjs');

const ACTIVATION_FIELDS = ['CARGO_HOME', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN',
  'RUSTUP_AUTO_INSTALL', 'RUSTC', 'PATH', 'LLVM_COV', 'LLVM_PROFDATA'];
const EDITOR_LIBRARIES = ['libgtk-3.so.0', 'libnss3.so', 'libasound.so.2', 'libgbm.so.1',
  'libX11.so.6', 'libX11-xcb.so.1', 'libXss.so.1', 'libxkbfile.so.1', 'libsecret-1.so.0'];
const ACTIVATE = 'source "$1" --pinned 1>&2 || exit "$?"\nexec "$2" -e "$3"';
const EMIT_ACTIVATION = 'process.stdout.write(JSON.stringify(Object.fromEntries('
  + JSON.stringify(ACTIVATION_FIELDS) + '.map(k => [k, process.env[k]]))))';
const HELP = `Usage: node tools/provision-dev.cjs [--ensure] [--rust-only]
       [--install-system-deps] [--nextest-version X.Y.Z]

Requires the checked Node major, Bash, curl, Git, tar and GNU timeout on glibc Linux.
Default: provision the checkout Rust pin and stable, matching components, Nextest,
locked editor dependencies, and run the existing native editor prerequisite smoke.
--ensure reuses installed stable and matching npm dependencies; it still installs
a newly checked Rust pin and missing components. This is an explicit online action.
--rust-only omits editor dependencies and editor verification.
--install-system-deps explicitly installs Debian/Ubuntu build and editor packages
with apt-get as root or sudo -n. It never prompts or changes system package settings.
Full tests, coverage and saved-task restoration require separate verification.`;

function failure(message, exitCode = 1) {
  return Object.assign(new Error(message), { exitCode });
}

function parseArgs(args) {
  const options = { ensure: false, rustOnly: false, installSystemDeps: false,
    nextestVersion: null, help: false };
  const seen = new Set();
  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (seen.has(argument)) throw failure(`Repeated option: ${argument}`, 2);
    seen.add(argument);
    if (argument === '--ensure') options.ensure = true;
    else if (argument === '--rust-only') options.rustOnly = true;
    else if (argument === '--install-system-deps') options.installSystemDeps = true;
    else if (argument === '--help') options.help = true;
    else if (argument === '--nextest-version') {
      const version = args[++index];
      if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
        throw failure('--nextest-version requires an exact stable X.Y.Z release.', 2);
      }
      options.nextestVersion = version;
    } else throw failure(`Unknown option: ${argument}`, 2);
  }
  return options;
}

function findCommand(name, env) {
  for (const entry of (env.PATH ?? '').split(path.delimiter)) {
    const candidate = path.resolve(entry || '.', name);
    try {
      if (fs.statSync(candidate).isFile()) {
        fs.accessSync(candidate, fs.constants.X_OK);
        return candidate;
      }
    } catch { /* Continue through PATH, including a missing directory. */ }
  }
  return null;
}

function procGroupFor(childPid) {
  // /proc can be mounted from an ancestor PID namespace. Map this thread's
  // actual direct child before comparing proc stat's visible process-group IDs.
  const namespaceIds = (status, key) => {
    const row = new RegExp(`^${key}:\\s*([0-9 \\t]+)$`, 'm').exec(status)?.[1];
    return row?.trim().split(/\s+/).map(Number);
  };
  try {
    const ownIds = namespaceIds(fs.readFileSync('/proc/self/status', 'utf8'), 'NSpid');
    if (!ownIds?.length || ownIds.at(-1) !== process.pid) return null;
    const index = ownIds.length - 1;
    const children = fs.readFileSync('/proc/thread-self/children', 'utf8').trim().split(/\s+/);
    for (const visiblePid of children) {
      if (!/^\d+$/.test(visiblePid)) continue;
      try {
        const status = fs.readFileSync(`/proc/${visiblePid}/status`, 'utf8');
        const pids = namespaceIds(status, 'NSpid');
        const groups = namespaceIds(status, 'NSpgid');
        if (pids?.length === ownIds.length && pids[index] === childPid
          && groups?.length === ownIds.length && groups[index] === childPid && groups[0] > 0) return groups[0];
      } catch { /* The child can finish before its proc identity is observed. */ }
    }
  } catch { /* Unobservable group membership must remain conservative below. */ }
  return null;
}

function nativeRun(command, args, options) {
  // Keep Node responsive to cancellation while GNU timeout bounds the command.
  // Own only this detached process group; independently escaped groups are not contained.
  const watchdog = findCommand('timeout', options.env);
  if (!watchdog) throw failure('GNU timeout is required before provisioning.');
  const budget = options.timeoutMs ?? 900_000;
  const grace = options.killAfterMs ?? 10_000;
  return new Promise(resolve => {
    const child = cp.spawn(watchdog, ['--signal=TERM', `--kill-after=${grace / 1000}s`,
      `${budget / 1000}s`, command, ...args], {
      cwd: options.cwd, env: options.env, detached: true,
      stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : ['ignore', 'inherit', 'inherit'],
    });
    const visibleGroup = child.pid ? procGroupFor(child.pid) : null;
    let stdout = Buffer.alloc(0), stderr = Buffer.alloc(0), cause, closed, spawnError;
    let cancelled = false, timedOut = false, stopping = false, settled = false;
    let escalation, closureDeadline, groupCheck;
    const signalGroup = signal => {
      if (!child.pid) return;
      try { process.kill(-child.pid, signal); }
      catch (error) { if (error.code !== 'ESRCH') spawnError ??= error; }
    };
    const liveGroup = () => {
      if (!child.pid) return false;
      try { process.kill(-child.pid, 0); } catch (error) { return error.code !== 'ESRCH'; }
      if (visibleGroup === null) return true;
      // A host init may retain orphan zombies; they are terminal, not running workers.
      try {
        let observed = false;
        for (const pid of fs.readdirSync('/proc')) {
          if (!/^\d+$/.test(pid)) continue;
          try {
            const stat = fs.readFileSync(`/proc/${pid}/stat`, 'utf8');
            const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
            if (Number(fields[2]) !== visibleGroup) continue;
            observed = true;
            if (!['Z', 'X'].includes(fields[0])) return true;
          } catch (error) { if (error.code !== 'ENOENT' && error.code !== 'ESRCH') return true; }
        }
        // A successful native group probe followed by no observable member is
        // not evidence of absence; retain the forced-cleanup deadline.
        return !observed;
      } catch { return true; }
    };
    const finish = () => {
      if (settled) return;
      settled = true;
      for (const timer of [deadline, escalation, closureDeadline, groupCheck]) clearTimeout(timer);
      process.removeListener('SIGINT', interrupted);
      process.removeListener('SIGTERM', terminated);
      resolve({ status: cause?.status ?? closed?.status ?? 1,
        signal: cause?.signal ?? closed?.signal ?? null, error: cause?.error ?? spawnError,
        stdout: stdout.toString('utf8'), stderr: stderr.toString('utf8'), cancelled, timedOut });
    };
    const afterClose = () => {
      if (settled || !closed) return;
      if (!liveGroup()) { finish(); return; }
      stop(cause ?? { status: 1, error: failure('The command exited with live processes in its owned group.') });
      groupCheck = setTimeout(afterClose, 50);
    };
    const stop = reason => {
      cause ??= reason;
      if (stopping) return;
      stopping = true;
      // GNU timeout forwards this signal and maintains its own kill-after timer.
      try { child.kill(reason.signal ?? 'SIGTERM'); }
      catch (error) { if (error.code !== 'ESRCH') spawnError ??= error; }
      escalation = setTimeout(() => {
        signalGroup('SIGKILL');
        // Do not cancel escalation merely because the timeout wrapper has closed.
        closureDeadline = setTimeout(() => {
          if (!closed || liveGroup()) {
            spawnError ??= failure('Owned process-group cleanup did not finish after forced termination.');
            child.stdout?.destroy(); child.stderr?.destroy(); child.unref();
          }
          finish();
        }, 1000);
        afterClose();
      }, grace);
    };
    const interrupted = () => { cancelled = true; stop({ status: 130, signal: 'SIGINT' }); };
    const terminated = () => { cancelled = true; stop({ status: 143, signal: 'SIGTERM' }); };
    process.on('SIGINT', interrupted);
    process.on('SIGTERM', terminated);
    const deadline = setTimeout(() => {
      timedOut = true;
      stop({ status: 124, error: Object.assign(failure('Native command exceeded its deadline.', 124), { code: 'ETIMEDOUT' }) });
    }, budget);
    const capture = (stream, bytes) => {
      const available = 2 * 1024 * 1024 - stdout.length - stderr.length;
      const kept = bytes.subarray(0, Math.max(0, available));
      if (stream === 'stdout') stdout = Buffer.concat([stdout, kept]);
      else stderr = Buffer.concat([stderr, kept]);
      if (bytes.length > available) stop({ status: 1,
        error: Object.assign(failure('Native command exceeded its output limit.'), { code: 'ENOBUFS' }) });
    };
    child.stdout?.on('data', bytes => capture('stdout', bytes));
    child.stderr?.on('data', bytes => capture('stderr', bytes));
    child.on('error', error => { spawnError = error; });
    child.on('exit', (status, signal) => {
      if (status === 124) timedOut = true;
      if (status !== 0 || signal) cause ??= { status, signal };
    });
    child.on('close', (status, signal) => { closed = { status, signal }; afterClose(); });
  });
}

function sha256(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }

function readSmall(filename, limit = 1024 * 1024) {
  const info = fs.lstatSync(filename);
  if (!info.isFile() || info.size > limit) throw failure(`Expected a regular file within the read budget: ${filename}`);
  const bytes = fs.readFileSync(filename);
  if (bytes.length > limit) throw failure(`File exceeds its read budget: ${filename}`);
  return bytes;
}

function compilerIdentity(text) {
  const release = /^release: (\d+\.\d+\.\d+)$/m.exec(text)?.[1];
  const host = /^host: ([a-zA-Z0-9_-]+)$/m.exec(text)?.[1];
  if (!release || !host) throw failure('rustc -vV did not identify an exact stable release and host.');
  return { release, host, exact: `${release}-${host}` };
}

function selectNextestRelease(metadata, host) {
  const project = metadata?.projects?.['cargo-nextest'];
  const selector = project?.latest;
  const range = project?.ranges?.[selector];
  const version = range?.latest;
  const release = range?.versions?.[version];
  if (typeof selector !== 'string' || !/^(0|[1-9]\d*)(?:\.(0|[1-9]\d*))?$/.test(selector)
    || typeof version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)
    || !version.startsWith(`${selector}.`) || range.is_prerelease !== false
    || release?.status !== 'active' || release.draft || release.prerelease || release.is_prerelease) {
    throw failure('The official Nextest metadata did not identify an active, exact stable release.');
  }
  const url = `https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${version}/cargo-nextest-${version}-${host}.tar.gz`;
  const locations = Array.isArray(release.locations)
    ? release.locations.filter(location => location?.target === host && location.format === 'tar.gz') : [];
  const checksum = locations[0]?.checksums?.sha256;
  if (locations.length !== 1 || locations[0].url !== url
    || typeof checksum !== 'string' || !/^[a-fA-F0-9]{64}$/.test(checksum)) {
    throw failure(`The official Nextest metadata did not identify one verified archive for ${host}.`);
  }
  return { version, url, sha256: checksum.toLowerCase() };
}

async function provision(options, deps = {}) {
  const root = path.resolve(deps.root ?? path.join(__dirname, '..'));
  const env = { ...(deps.env ?? process.env) };
  const platform = deps.platform ?? process.platform;
  const arch = deps.arch ?? process.arch;
  const nodeVersion = deps.nodeVersion ?? process.versions.node;
  const glibc = deps.glibcVersion ?? process.report.getReport().header.glibcVersionRuntime;
  const uid = deps.uid ?? process.getuid?.();
  const locate = deps.findCommand ?? findCommand;
  const execute = deps.run ?? nativeRun;
  const print = deps.print ?? (message => process.stdout.write(`${message}\n`));
  const report = (deps.collectSetup ?? collectDeveloperSetup)({ root, env, platform, nodeVersion });
  const requirements = report.requirements;
  if (platform !== 'linux' || !['x64', 'arm64'].includes(arch) || !glibc) {
    throw failure('Provisioning supports glibc Linux x64/arm64; use the platform setup guide on this host.');
  }
  const failedCheck = id => {
    const check = report.checks?.find(entry => entry.check_id === id && entry.status === 'fail');
    if (!check) return '';
    const observed = typeof check.observed_fact === 'string' ? check.observed_fact : JSON.stringify(check.observed_fact);
    return `${id}: ${observed ?? 'not observed'}. ${check.next_action?.instruction ?? ''}`.slice(0, 2048);
  };
  if (!requirements) {
    const authorities = report.requirement_sources?.map(source => source.path).join(', ');
    throw failure(`Cannot read the checkout requirements. ${failedCheck('source_requirements')
      || 'Use a complete checkout and inspect the requirement sources.'}${authorities ? ` Authorities read: ${authorities}.` : ''}`);
  }
  if (Number(nodeVersion.split('.')[0]) !== requirements.node_major) {
    throw failure(`Node ${nodeVersion} does not match the required Node ${requirements.node_major} major. ${failedCheck('node_runtime')
      || 'Use the Node major selected by .github/workflows/vscode-e2e.yml.'}`);
  }
  const pin = requirements.rust_channel;
  if (!/^(stable|\d+\.\d+\.\d+)$/.test(pin)) throw failure('The checked Rust channel must be stable or an exact X.Y.Z release.');
  const components = [...new Set([...requirements.rust_components, 'rustfmt', 'clippy', 'rust-src',
    ...requirements.llvm_components.flatMap(value => value.split(/[\s,]+/))])];
  if (components.some(value => !/^[a-z0-9-]{1,64}$/.test(value))) throw failure('Unsupported Rust component requirement.');
  for (const name of ['CARGO_HOME', 'RUSTUP_HOME']) {
    const directory = env[name] ?? (env.HOME ? path.join(env.HOME, name === 'CARGO_HOME' ? '.cargo' : '.rustup') : '');
    if (!path.isAbsolute(directory) || directory.includes(':')) {
      throw failure(`${name} must be a nonempty absolute directory without a colon; choose it explicitly.`);
    }
    env[name] = directory;
  }
  for (const name of ['RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
    'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER']) {
    if (env[name]) throw failure(`${name} is set; resolve the explicit compiler wrapper before provisioning.`);
  }
  const commands = {};
  const requireCommand = name => {
    const command = locate(name, env);
    if (!command) throw failure(`Missing ${name}; provision the base image or use --install-system-deps for supported Debian/Ubuntu native packages.`);
    commands[name] = command;
    return command;
  };
  for (const name of ['bash', 'curl', 'git', 'tar', 'timeout']) requireCommand(name);
  if (!options.rustOnly) requireCommand('npm');
  const call = async (command, args, settings = {}) => {
    const result = await execute(command, args, { cwd: root, env, ...settings });
    if (result.cancelled || result.timedOut || result.signal || (result.error && result.error.code !== 'ENOENT')
      || (!settings.allowFailure && (result.error || result.status !== 0))) {
      const code = Number.isInteger(result.status) && result.status > 0 ? result.status : 1;
      const detail = String(result.stderr?.trim() || result.error?.message || result.signal || '').slice(-2048);
      throw failure(`${path.basename(command)} failed (${code})${detail ? `: ${detail}` : ''}`, code);
    }
    return result;
  };
  const bin = path.join(env.CARGO_HOME, 'bin');
  const selectedRustup = path.join(bin, 'rustup');
  let rustup = locate('rustup', env);
  try {
    fs.lstatSync(selectedRustup);
    if (!fs.statSync(selectedRustup).isFile()) throw failure('Not a regular executable.');
    fs.accessSync(selectedRustup, fs.constants.X_OK);
    rustup = selectedRustup;
  } catch (error) {
    if (error.code !== 'ENOENT') throw failure(`The selected Cargo home has an unusable rustup: ${selectedRustup}. Repair it explicitly.`);
    // A dangling symlink is a corrupt selected installation, not an absent one.
    try { fs.lstatSync(selectedRustup); throw failure(`The selected Cargo home has a dangling rustup: ${selectedRustup}.`); }
    catch (missing) { if (missing.code !== 'ENOENT') throw missing; }
  }
  const whichRustc = async selector => {
    if (!rustup) return null;
    const result = await call(rustup, ['which', '--toolchain', selector, 'rustc'],
      { capture: true, allowFailure: true, env: { ...env, RUSTUP_AUTO_INSTALL: '0' }, timeoutMs: 30_000 });
    const compiler = String(result.stdout ?? '').trim();
    return result.status === 0 && !result.error && !result.signal && path.isAbsolute(compiler) ? compiler : null;
  };
  // Do not download a new SDK just to discover an inherited conflicting override.
  if (env.RUSTC || env.CARGO_BUILD_RUSTC) {
    const existing = await whichRustc(pin);
    const canonical = value => { try { return fs.realpathSync(value); } catch { return value; } };
    for (const name of ['RUSTC', 'CARGO_BUILD_RUSTC']) {
      if (env[name] && (!existing || canonical(env[name]) !== canonical(existing))) {
        throw failure(`${name} selects another compiler; unset it explicitly or start a fresh shell before provisioning the checkout pin.`);
      }
    }
  }
  if (options.installSystemDeps) {
    const apt = requireCommand('apt-get');
    const aptCache = requireCommand('apt-cache');
    let prefix = [];
    if (uid !== 0) {
      const sudo = requireCommand('sudo');
      const permission = await call(sudo, ['-n', 'true'], { capture: true, allowFailure: true, timeoutMs: 30_000 });
      if (permission.status !== 0 || permission.error || permission.signal) {
        const detail = String(permission.stderr?.trim() || permission.error?.message || '').slice(-2048);
        throw failure(`Native packages need root or working noninteractive sudo. Prepare the base image; no rootless sysroot is assumed.${detail ? ` ${detail}` : ''}`,
          Number.isInteger(permission.status) && permission.status > 0 ? permission.status : 1);
      }
      // sudo's assignment syntax preserves the variable subject to its policy;
      // passing it only in our environment can lose it through env_reset.
      prefix = [sudo, '-n', 'DEBIAN_FRONTEND=noninteractive'];
    }
    const aptRun = args => call(prefix[0] ?? apt, [...prefix.slice(1), ...(prefix.length ? [apt] : []), ...args],
      { env: { ...env, DEBIAN_FRONTEND: 'noninteractive' } });
    await aptRun(['update']);
    const packages = ['build-essential', 'pkg-config', 'ca-certificates'];
    if (!options.rustOnly) {
      const newer = await call(aptCache, ['show', 'libasound2t64'], { capture: true, allowFailure: true, timeoutMs: 30_000 });
      packages.push('xvfb', 'xauth', 'libgtk-3-0', 'libnss3', 'libgbm1', 'libxss1', 'libsecret-1-0', 'libx11-xcb1', 'libxkbfile1',
        newer.status === 0 && String(newer.stdout).includes('Package: libasound2t64') ? 'libasound2t64' : 'libasound2');
    }
    await aptRun(['install', '-y', '--no-install-recommends', ...packages]);
  }
  for (const name of ['cc', 'make', 'pkg-config']) requireCommand(name);
  const headless = !(env.DISPLAY || env.WAYLAND_DISPLAY);
  if (!options.rustOnly && headless) for (const name of ['xvfb-run', 'Xvfb', 'xauth']) requireCommand(name);
  if (!options.rustOnly) {
    const ldconfig = locate('ldconfig', env) ?? locate('ldconfig', { ...env, PATH: '/usr/sbin:/sbin' });
    if (!ldconfig) throw failure('Missing ldconfig; prepare the base image with the native library cache tool in PATH, /usr/sbin or /sbin.');
    const cache = await call(ldconfig, ['-p'], { capture: true, timeoutMs: 30_000 });
    const architecture = arch === 'x64' ? 'x86-64' : 'AArch64';
    const libraries = new Set(String(cache.stdout).split('\n').filter(line => line.includes(architecture))
      .map(line => /^\s*(\S+)\s+\([^)]*\)\s+=>\s+\//.exec(line)?.[1]));
    const missing = EDITOR_LIBRARIES.filter(library => !libraries.has(library));
    if (missing.length) throw failure(`Missing native editor libraries: ${missing.join(', ')}. Use --install-system-deps with root/noninteractive sudo, or prepare the base image. A user-local sysroot is not supported.`);
  }
  for (const entry of requirements.linker_temp) fs.mkdirSync(entry.path, { recursive: true });
  fs.mkdirSync(path.join(root, 'target'), { recursive: true });
  fs.mkdirSync(bin, { recursive: true });
  env.PATH = `${bin}${path.delimiter}${env.PATH ?? ''}`;
  const temporary = fs.mkdtempSync(path.join(root, 'target', 'provision-'));
  const host = `${arch === 'x64' ? 'x86_64' : 'aarch64'}-unknown-linux-gnu`;
  const download = async (url, filename, maxBytes) => {
    await call(commands.curl, ['--proto', '=https', '--proto-redir', '=https', '--tlsv1.2',
      '--fail', '--silent', '--show-error', '--location', '--connect-timeout', '10',
      '--max-time', '120', '--retry', '2', '--retry-max-time', '240',
      '--max-filesize', String(maxBytes), '--output', filename, url], { timeoutMs: 360_000 });
    return readSmall(filename, maxBytes);
  };
  const verifiedDownload = async (url, digestUrl, name, maxBytes, expectedDigest) => {
    const destination = path.join(temporary, name);
    const bytes = await download(url, destination, maxBytes);
    const digest = (await download(digestUrl, `${destination}.sha256`, 1024)).toString('utf8').trim();
    const match = /^([a-fA-F0-9]{64})(?:[ \t]+\*?[^\r\n]+)?$/.exec(digest);
    const actual = sha256(bytes);
    if (!match || actual !== match[1].toLowerCase() || (expectedDigest && actual !== expectedDigest)) {
      throw failure(`SHA-256 verification failed for ${name}.`);
    }
    return destination;
  };
  try {
    if (!rustup) {
      const url = `https://static.rust-lang.org/rustup/dist/${host}/rustup-init`;
      const installer = await verifiedDownload(url, `${url}.sha256`, 'rustup-init', 40 * 1024 * 1024);
      fs.chmodSync(installer, 0o755);
      await call(installer, ['-y', '--no-modify-path', '--default-toolchain', 'none', '--profile', 'minimal']);
      rustup = path.join(bin, 'rustup');
    }
    const missingComponents = async selector => {
      const listed = await call(rustup, ['component', 'list', '--installed', '--toolchain', selector],
        { capture: true, timeoutMs: 30_000 });
      const installed = new Set(String(listed.stdout).trim().split(/\r?\n/).map(line => line.replace(/\s+\(installed\)$/, '')));
      return components.filter(component => ![component, component.replace(/-preview$/, '')]
        .some(name => installed.has(name) || installed.has(`${name}-${host}`)));
    };
    const ensureToolchain = async (selector, refresh = false) => {
      let compiler = await whichRustc(selector);
      if (!compiler || refresh) {
        await call(rustup, ['toolchain', 'install', selector, '--profile', 'minimal', '--no-self-update',
          ...components.flatMap(component => ['--component', component])]);
      }
      const missing = await missingComponents(selector);
      if (missing.length) {
        await call(rustup, ['component', 'add', '--toolchain', selector, ...missing]);
        if ((await missingComponents(selector)).length) throw failure(`Rustup did not install the required components for ${selector}.`);
      }
      compiler = await whichRustc(selector);
      if (!compiler) throw failure(`Rustup did not resolve the installed compiler for ${selector}.`);
      const compilerId = (await call(compiler, ['-vV'], { capture: true, timeoutMs: 30_000 })).stdout;
      const identity = compilerIdentity(compilerId);
      if (identity.host !== host || (selector !== 'stable' && selector !== identity.release && selector !== identity.exact)) {
        throw failure(`Installed Rust identity does not match ${selector} on ${host}.`);
      }
      return { ...identity, compilerId };
    };
    print(`Provisioning checkout Rust ${pin}, stable, and ${components.join(', ')}.`);
    const pinned = await ensureToolchain(pin, pin === 'stable' && !options.ensure);
    const stable = pin === 'stable' ? pinned : await ensureToolchain('stable', !options.ensure);
    const frozen = await ensureToolchain(stable.exact);
    if (frozen.compilerId !== stable.compilerId) throw failure('Stable and its exact release report different compiler identities.');
    const activated = await call(commands.bash, ['--noprofile', '--norc', '-c', ACTIVATE,
      'ripr-provision', path.join(root, 'tools', 'activate-rust.sh'), process.execPath, EMIT_ACTIVATION],
    { capture: true, timeoutMs: 60_000 });
    const fields = JSON.parse(activated.stdout);
    if (ACTIVATION_FIELDS.some(name => typeof fields[name] !== 'string' || !fields[name])) {
      throw failure('The existing activation helper did not return its complete environment.');
    }
    const activeEnv = { ...env, ...Object.fromEntries(ACTIVATION_FIELDS.map(name => [name, fields[name]])) };
    const cargo = path.join(path.dirname(activeEnv.RUSTC), 'cargo');
    for (const args of [['-vV'], ['fmt', '--version'], ['clippy', '--version']]) await call(cargo, args, { env: activeEnv });
    for (const tool of [activeEnv.LLVM_COV, activeEnv.LLVM_PROFDATA]) await call(tool, ['--version'], { env: activeEnv });
    const nextest = path.join(bin, 'cargo-nextest');
    const previous = await call(nextest, ['nextest', '--version'], { env: activeEnv, capture: true, allowFailure: true, timeoutMs: 30_000 });
    let nextestVersion = previous.status === 0 ? /^cargo-nextest (\d+\.\d+\.\d+)(?:\s|$)/.exec(previous.stdout ?? '')?.[1] : null;
    if (!nextestVersion || (options.nextestVersion && nextestVersion !== options.nextestVersion) || (!options.ensure && !options.nextestVersion)) {
      let wanted = options.nextestVersion;
      let release;
      if (!wanted) {
        const metaUrl = 'https://raw.githubusercontent.com/nextest-rs/release-meta/main/releases.json';
        const metaFile = path.join(temporary, 'nextest-release.json');
        const metaBytes = await download(metaUrl, metaFile, 4 * 1024 * 1024);
        let metadata;
        try {
          metadata = JSON.parse(metaBytes.toString('utf8'));
        } catch {
          throw failure(`Downloaded Nextest release metadata from ${metaUrl} is not valid JSON; the download may have been intercepted or truncated. Inspect ${metaFile} and retry.`);
        }
        release = selectNextestRelease(metadata, host);
        wanted = release.version;
      }
      const asset = `cargo-nextest-${wanted}-${host}`;
      const base = `https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${wanted}/${asset}`;
      const archive = await verifiedDownload(release?.url ?? `${base}.tar.gz`, `${base}.sha256`,
        'nextest.tar.gz', 64 * 1024 * 1024, release?.sha256);
      await call(commands.tar, ['--extract', '--gzip', '--file', archive, '--directory', temporary,
        '--no-same-owner', '--no-same-permissions', '--', 'cargo-nextest']);
      const candidate = path.join(temporary, 'cargo-nextest');
      if (!fs.lstatSync(candidate).isFile()) throw failure('The verified Nextest archive did not contain a regular cargo-nextest file.');
      fs.chmodSync(candidate, 0o755);
      const observed = await call(candidate, ['nextest', '--version'], { env: activeEnv, capture: true, timeoutMs: 30_000 });
      if (!new RegExp(`^cargo-nextest ${wanted.replaceAll('.', '\\.')}(?:\\s|$)`).test(observed.stdout ?? '')) {
        throw failure('The downloaded Nextest executable does not report the requested release.');
      }
      const replacement = path.join(bin, `.ripr-nextest-${crypto.randomUUID()}`);
      try {
        fs.copyFileSync(candidate, replacement, fs.constants.COPYFILE_EXCL);
        fs.chmodSync(replacement, 0o755);
        fs.renameSync(replacement, nextest);
      } finally {
        try { fs.rmSync(replacement, { force: true }); }
        catch { try { print(`Nextest replacement staging retained: ${replacement}`); } catch { /* Preserve the native failure. */ } }
      }
      nextestVersion = wanted;
    }
    await call(cargo, ['nextest', '--version'], { env: activeEnv, timeoutMs: 30_000 });
    if (!options.rustOnly) {
      const editor = path.join(root, 'editors', 'vscode');
      const modules = path.join(editor, 'node_modules');
      const stamp = path.join(modules, '.ripr-provision-lock.json');
      const fingerprintFor = () => JSON.stringify({ node_major: requirements.node_major, platform, arch,
        package: sha256(readSmall(path.join(editor, 'package.json'))),
        lock: sha256(readSmall(path.join(editor, 'package-lock.json'))) });
      const fingerprint = fingerprintFor();
      const installed = () => {
        try {
          return ['typescript/bin/tsc', '.bin/tsc', '.bin/vsce'].every(file => fs.statSync(path.join(modules, file)).isFile())
            && JSON.parse(readSmall(path.join(modules, 'typescript', 'package.json'))).version === requirements.typescript
            && JSON.parse(readSmall(path.join(modules, '@vscode', 'test-electron', 'package.json'))).version === requirements.test_electron;
        } catch { return false; }
      };
      let sameLock = false;
      try { sameLock = readSmall(stamp, 4096).toString('utf8') === fingerprint; } catch { /* No reusable install. */ }
      const checkGraph = () => call(commands.npm, ['ls', '--all', '--include=dev', '--offline'],
        { cwd: editor, env: activeEnv, capture: true, allowFailure: true, timeoutMs: 60_000 });
      const reusable = options.ensure && sameLock && installed() && (await checkGraph()).status === 0;
      if (!reusable) {
        fs.rmSync(stamp, { force: true });
        await call(commands.npm, ['ci', '--include=dev'], { cwd: editor, env: activeEnv });
        if (!installed()) throw failure('npm ci did not supply the checked TypeScript and editor host downloader.');
        const graph = await checkGraph();
        if (graph.status !== 0) throw failure(`npm ci left an incomplete locked dependency graph: ${String(graph.stderr ?? '').slice(-2048)}`,
          Number.isInteger(graph.status) && graph.status > 0 ? graph.status : 1);
        if (fingerprintFor() !== fingerprint) throw failure('The editor package files changed during npm ci; no reusable installation was recorded.');
        fs.writeFileSync(stamp, fingerprint, { flag: 'wx' });
      }
      await call(headless ? commands['xvfb-run'] : commands.npm,
        headless ? ['-a', '-e', '/dev/stderr', commands.npm, 'run', 'setup:verify-editor'] : ['run', 'setup:verify-editor'],
        { cwd: editor, env: { ...activeEnv,
          // Managed uid-0 namespaces may not represent archive uid/gid values.
          // Scope the extraction choice to this downloader; keep caller options.
          TAR_OPTIONS: `${activeEnv.TAR_OPTIONS ?? ''} --no-same-owner`.trim() } });
    }
    const fetchArgs = ['fetch', '--locked', '--manifest-path', path.join(root, 'Cargo.toml')];
    const cached = options.ensure && await call(cargo, [...fetchArgs, '--offline'],
      { env: activeEnv, capture: true, allowFailure: true });
    if (cached && (cached.status !== 0 || cached.error || cached.signal)) {
      const detail = `${cached.stdout ?? ''}\n${cached.stderr ?? ''}`;
      if (cached.error || cached.signal || !/--offline was specified|using offline mode/.test(detail)) {
        throw failure(`Offline dependency check failed; no online retry: ${detail.trim().slice(-2048)}`,
          Number.isInteger(cached.status) && cached.status > 0 ? cached.status : 1);
      }
    }
    if (!cached || cached.status !== 0) await call(cargo, fetchArgs, { env: activeEnv });
    const result = { requirements, requirement_sources: report.requirement_sources,
      rust_pin: pinned.exact, stable_exact: stable.exact, nextest_version: nextestVersion,
      editor_host: options.rustOnly ? 'not_run' : 'passed', full_development_ready: false, fresh_task: 'not_proven' };
    print(`Provisioned Rust ${pinned.exact}; stable ${stable.exact}; Nextest ${nextestVersion}.`);
    print('Next, source tools/activate-rust.sh --pinned in the shell that will run Cargo.');
    print(options.rustOnly ? 'Editor provisioning and verification: not run (--rust-only).'
      : 'Editor prerequisite smoke passed. Full validation: cd editors/vscode && xvfb-run -a npm run setup:verify-full');
    print('Full tests, coverage qualification and saved-task restoration: not established.');
    return result;
  } finally {
    try { fs.rmSync(temporary, { recursive: true, force: true }); }
    catch { try { print(`Provisioning temporary files retained: ${temporary}`); } catch { /* Preserve the native failure. */ } }
  }
}

async function main(args = process.argv.slice(2)) {
  const options = parseArgs(args);
  if (options.help) { process.stdout.write(`${HELP}\n`); return; }
  await provision(options);
}

module.exports = { parseArgs, provision, findCommand, compilerIdentity, selectNextestRelease, nativeRun, EDITOR_LIBRARIES, main };
if (require.main === module) main().catch(error => {
  process.stderr.write(`provision-dev: ${error.message}\n`);
  process.exitCode = Number.isInteger(error.exitCode) ? error.exitCode : 1;
});
