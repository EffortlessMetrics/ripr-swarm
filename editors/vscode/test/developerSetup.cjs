// Default source-checkout diagnosis uses Node built-ins with no writes or subprocesses.
// Only the explicit verification preflight resets its two owned evidence files.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const MAX_SOURCE_BYTES = 256 * 1024;
const MAX_TEXT = 512;
const bounded = value => String(value).replace(/[\x00-\x1f\x7f]/g, ' ').slice(0, MAX_TEXT);

function readAuthority(root, relative) {
  const filename = path.join(root, relative);
  const stat = fs.lstatSync(filename);
  if (!stat.isFile() || stat.size > MAX_SOURCE_BYTES) {
    throw new Error(`Expected a regular source file of at most ${MAX_SOURCE_BYTES} bytes: ${relative}`);
  }
  const descriptor = fs.openSync(filename, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW ?? 0));
  try {
    const opened = fs.fstatSync(descriptor);
    if (!opened.isFile() || opened.size > MAX_SOURCE_BYTES) throw new Error(`Source changed: ${relative}`);
    const bytes = Buffer.alloc(MAX_SOURCE_BYTES + 1);
    const count = fs.readSync(descriptor, bytes, 0, bytes.length, 0);
    if (count > MAX_SOURCE_BYTES) throw new Error(`Source grew beyond the read budget: ${relative}`);
    if (count !== opened.size) throw new Error(`Source changed or was not read completely: ${relative}`);
    const content = bytes.subarray(0, count);
    return { path: relative, sha256: crypto.createHash('sha256').update(content).digest('hex'),
      text: content.toString('utf8') };
  } finally {
    fs.closeSync(descriptor);
  }
}

function directoryObservation(filename) {
  try {
    if (!fs.statSync(filename).isDirectory()) return 'not_a_directory';
    fs.accessSync(filename, fs.constants.W_OK);
    return 'access_check_passed';
  } catch (error) {
    return error && error.code === 'ENOENT' ? 'absent' : 'access_not_established';
  }
}

// Shared by the explicit npm preflight and direct TS entrypoints. Never remove
// a directory or any other artifact, cache, workspace, or caller-owned file.
function resetEditorSetupEvidence(artifactRoot = path.resolve(process.env.RIPR_TEST_ARTIFACT_ROOT
  ?? path.join(__dirname, '../../../target/ripr'))) {
  const publishedReceipt = path.join(artifactRoot, 'setup-editor-host.json');
  const failureLog = path.join(artifactRoot, 'setup-editor-host.log');
  fs.rmSync(publishedReceipt, { force: true });
  fs.rmSync(failureLog, { force: true });
  return { publishedReceipt, failureLog };
}

function collectDeveloperSetup(options = {}) {
  const root = path.resolve(options.root ?? path.join(__dirname, '../../..'));
  const env = options.env ?? process.env;
  const platform = options.platform ?? process.platform;
  const nodeVersion = options.nodeVersion ?? process.versions.node;
  const sources = [];
  const checks = [];
  const add = (id, status, fact, next) => checks.push({ check_id: id, status,
    observed_fact: fact, next_action: next });
  let requirements;
  let linkerTempPaths = [];
  try {
    const read = relative => {
      const source = readAuthority(root, relative);
      sources.push({ path: source.path, sha256: source.sha256 });
      return source.text;
    };
    const toolchain = read('rust-toolchain.toml');
    const editor = JSON.parse(read('editors/vscode/package.json'));
    const lock = JSON.parse(read('editors/vscode/package-lock.json'));
    const workflow = read('.github/workflows/vscode-e2e.yml');
    const gates = read('.github/workflows/rust-gates.yml');
    const cargoConfig = read('.cargo/config.toml');
    const rustChannel = /^channel\s*=\s*"([^"\r\n]+)"\s*$/m.exec(toolchain)?.[1];
    const components = /^components\s*=\s*(\[[^\r\n]+\])\s*$/m.exec(toolchain)?.[1];
    const nodeMajors = [...workflow.matchAll(/^\s+node-version:\s*['"]?(\d+)['"]?\s*$/gm)];
    const llvmComponents = [...gates.matchAll(/^\s+components:\s*([^\r\n]+)$/gm)]
      .map(match => match[1].trim()).filter(value => value.includes('llvm-tools'));
    const nextestAction = /uses:\s*(taiki-e\/install-action@[a-f0-9]{40})/.exec(gates)?.[1];
    if (!rustChannel || !components || nodeMajors.length !== 1 || !nextestAction
      || llvmComponents.length !== 1 || !gates.includes('nextest')
      || !editor.engines?.vscode || !lock.packages?.['node_modules/typescript']?.version
      || !lock.packages?.['node_modules/@vscode/test-electron']?.version) {
      throw new Error('The current requirement source shape is unsupported; inspect the named authorities.');
    }
    const linkerTemp = ['TEMP', 'TMP', 'TMPDIR'].map(name => {
      const match = new RegExp(`^${name}\\s*=\\s*\\{\\s*value\\s*=\\s*"([^"\\r\\n]+)",\\s*relative\\s*=\\s*true,\\s*force\\s*=\\s*true\\s*\\}\\s*$`, 'm').exec(cargoConfig);
      if (!match) throw new Error(`Cannot derive ${name} from .cargo/config.toml`);
      return { variable: name, path: path.resolve(root, match[1]) };
    });
    const rustComponents = JSON.parse(components);
    if (!Array.isArray(rustComponents) || rustComponents.length > 16
      || rustComponents.some(value => typeof value !== 'string' || !/^[a-z0-9-]{1,64}$/.test(value))
      || typeof editor.engines.vscode !== 'string' || editor.engines.vscode.length > 128
      || ![lock.packages['node_modules/typescript'].version,
        lock.packages['node_modules/@vscode/test-electron'].version].every(value =>
        typeof value === 'string' && /^[0-9][a-zA-Z0-9.+-]{0,127}$/.test(value))) {
      throw new Error('Requirement values exceed the supported bounded source shape.');
    }
    requirements = { rust_channel: bounded(rustChannel), rust_components: rustComponents,
      node_major: Number(nodeMajors[0][1]), vscode_engine: bounded(editor.engines.vscode),
      typescript: lock.packages['node_modules/typescript'].version,
      test_electron: lock.packages['node_modules/@vscode/test-electron'].version,
      llvm_components: llvmComponents.map(bounded), nextest_install_action: nextestAction,
      nextest_version: 'not_pinned_by_current_install_action_input',
      linker_temp: linkerTemp.map(entry => ({ ...entry, path: bounded(entry.path) })) };
    linkerTempPaths = linkerTemp;
    add('source_requirements', 'pass', 'Read the checked requirement files; hashes identify these inputs only.',
      { instruction: 'Provision from these authorities; this command never installs tools.' });
  } catch (error) {
    add('source_requirements', 'fail', bounded(error instanceof Error ? error.message : error),
      { instruction: 'Use a complete checkout and inspect the named requirement source.' });
  }
  const nodeMatches = requirements && Number(nodeVersion.split('.')[0]) === requirements.node_major;
  add('node_runtime', requirements ? (nodeMatches ? 'pass' : 'fail') : 'not_proven',
    { executed_version: bounded(nodeVersion), expected_major: requirements?.node_major ?? null },
    { instruction: 'Use the Node major selected by .github/workflows/vscode-e2e.yml.' });
  const editorRoot = path.join(root, 'editors/vscode');
  const installed = ['typescript/bin/tsc', '@vscode/test-electron/package.json']
    .every(relative => fs.existsSync(path.join(editorRoot, 'node_modules', relative)));
  add('locked_editor_dependencies', installed ? 'not_proven' : 'warn',
    installed ? 'Compiler/downloader files exist; their locked installation was not executed or authenticated.'
      : 'The local editor compiler/downloader files were not both observed.',
    { cwd: 'editors/vscode', command: ['npm', 'ci'] });
  add('rust_activation', 'not_proven', { toolchain_selector: bounded(env.RUSTUP_TOOLCHAIN ?? 'unset'),
    explicit_rustc: env.RUSTC !== undefined, cargo_build_rustc: env.CARGO_BUILD_RUSTC !== undefined },
    { cwd: '.', command: ['source', 'tools/activate-rust.sh', '--pinned'],
      instruction: 'Activate once in the current shell; stable selection is an explicit alternative. No compiler was executed here.' });
  add('source_build_doctor', 'not_run', 'Effective compiler selection and product diagnosis belong to the existing doctor.',
    { cwd: '.', command: ['ripr', 'doctor', '--profile', 'source-build', '--json'],
      instruction: 'Use an already built ripr from this checkout; do not compile it just for this cheap report.' });
  add('compiler_matched_llvm_profile', 'not_run', 'Neither selected LLVM tools nor a real instrumented profile were executed.',
    { instruction: 'Run the small selected-compiler profile control documented in CONTRIBUTING.md before a workspace build.' });
  add('nextest_runtime', 'not_run', 'The workflow install action is identified above; no installed Nextest version was executed.',
    { cwd: '.', command: ['cargo', 'nextest', '--version'] });
  const displayConfigured = Boolean(env.DISPLAY || env.WAYLAND_DISPLAY);
  add('editor_display', platform === 'linux' ? (displayConfigured ? 'not_proven' : 'fail') : 'not_proven',
    { platform, display_configured: displayConfigured,
      limitation: 'Configuration is not a live display or native-library test.' },
    { cwd: 'editors/vscode', command: platform === 'linux'
      ? ['xvfb-run', '-a', 'npm', 'run', 'setup:verify-editor'] : ['npm', 'run', 'setup:verify-editor'],
      instruction: 'Use a supported live display, or an already provisioned Xvfb; this report starts neither.' });
  const artifactRoot = path.resolve(env.RIPR_TEST_ARTIFACT_ROOT ?? path.join(root, 'target/ripr'));
  const paths = { cargo_target: env.CARGO_TARGET_DIR === undefined
    ? path.join(root, 'target') : path.resolve(env.CARGO_TARGET_DIR),
    editor_artifacts: artifactRoot,
    editor_host_cache: path.join(artifactRoot, 'vscode-test-cache') };
  const locations = Object.fromEntries(Object.entries(paths)
    .map(([name, filename]) => [name, { path: bounded(filename), observation: directoryObservation(filename) }]));
  locations.workspace_linker_temp = linkerTempPaths.map(entry => ({ variable: entry.variable,
    path: bounded(entry.path), observation: directoryObservation(entry.path) }));
  add('work_locations', 'not_proven', locations,
    { instruction: 'Prepare the observed Cargo target and each TEMP/TMP/TMPDIR directory derived from .cargo/config.toml before Cargo work. Access checks do not prove writes or a full-build resource budget.' });
  add('editor_host_acquisition', 'not_run', { proxy_configured: Boolean(env.HTTPS_PROXY || env.HTTP_PROXY || env.https_proxy || env.http_proxy),
    extra_ca_configured: Boolean(env.NODE_EXTRA_CA_CERTS),
    limitation: 'No network request, policy permission, proxy/CA validation or host download was performed.' },
    { cwd: 'editors/vscode', command: ['npm', 'run', 'setup:verify-editor'] });
  add('editor_native_host', 'not_run', 'Native libraries and a real extension-host sentinel have not been executed.',
    { cwd: 'editors/vscode', command: ['npm', 'run', 'setup:verify-editor'] });
  add('repository_policy', 'not_run', 'The existing small policy owner has not run.',
    { cwd: '.', command: ['cargo', 'policy', 'preflight'] });
  add('genuine_editor_producer', 'not_run', 'No Cargo producer, fixture or editor suite has run.',
    { cwd: 'editors/vscode', command: ['npm', 'run', 'test:e2e'] });
  add('full_development_validation', 'not_run', 'No workspace qualification or full-build resource measurement has run.',
    { cwd: '.', command: ['cargo', 'xtask', 'ci-full'],
      instruction: 'Follow the serial full-development sequence in CONTRIBUTING.md, then the canonical editor suite once.' });
  return { schema_version: 1, kind: 'ripr.developer-setup', mode: 'read_only',
    root: bounded(root), platform, requirement_sources: sources, requirements: requirements ?? null,
    checks, known_blockers: checks.filter(check => check.status === 'fail').map(check => check.check_id),
    full_development_ready: false,
    fresh_task: 'not_proven' };
}

function renderDeveloperSetup(report) {
  return [report.mode === 'explicit_editor_verification_preflight'
    ? 'Explicit editor verification preflight (prior setup evidence invalidated)'
    : 'Developer setup observations (read-only)',
    ...report.checks.map(check => `${check.check_id}: ${check.status}\n  ${bounded(typeof check.observed_fact === 'string'
      ? check.observed_fact : JSON.stringify(check.observed_fact))}\n  Next: ${[
      check.next_action.command ? `${check.next_action.cwd}: ${check.next_action.command.join(' ')}` : undefined,
      check.next_action.instruction].filter(Boolean).join('\n  ')}`),
    'Full development and fresh-task readiness: not established.'].join('\n');
}

function main(args = process.argv.slice(2)) {
  if (args.some(arg => !['--json', '--help', '--begin-editor-verification'].includes(arg))) {
    process.stderr.write('Usage: node editors/vscode/test/developerSetup.cjs [--json|--help]\n');
    return 2;
  }
  if (args.includes('--help')) {
    process.stdout.write('Read-only developer observations; no builds, writes, subprocesses or downloads.\nExplicit editor proof: cd editors/vscode && npm run setup:verify-editor\n');
    return 0;
  }
  const explicitVerification = args.includes('--begin-editor-verification');
  if (explicitVerification) resetEditorSetupEvidence();
  const report = collectDeveloperSetup();
  if (explicitVerification) {
    report.mode = 'explicit_editor_verification_preflight';
    report.prior_setup_evidence = 'invalidated';
  }
  process.stdout.write((args.includes('--json') ? JSON.stringify(report, null, 2) : renderDeveloperSetup(report)) + '\n');
  return report.known_blockers.length ? 1 : 0;
}

module.exports = { collectDeveloperSetup, renderDeveloperSetup, resetEditorSetupEvidence, main };
if (require.main === module) process.exitCode = main();
