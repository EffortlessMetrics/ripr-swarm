// Default source-checkout diagnosis uses Node built-ins with no writes or subprocesses.
// Explicit verification delegates to existing owners; ordinary reporting never loads them.
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

const FULL_STAGES = ['preflight', 'source_identity', 'activation', 'tool_versions',
  'profile_diagnostic', 'editor_prerequisites', 'repository_policy', 'rust_full',
  'editor_suite', 'editor_package'];
const ACTIVATION_FIELDS = ['CARGO_HOME', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN',
  'RUSTUP_AUTO_INSTALL', 'RUSTC', 'PATH', 'LLVM_COV', 'LLVM_PROFDATA'];
const PROFILE_TEST = 'coverage_diagnostics::linux::binary_ids::tests::real_rust_llvm_binary_id_matches_sparse_instrumented_elf_metadata';
const MAX_FULL_LOG_BYTES = 64 * 1024;
const MAX_FULL_RESULT_BYTES = 128 * 1024;
const FULL_CAPTURE = 'exec "$@" 2>&1';
// Source the existing helper, preserving its refusal and native diagnostic stderr.
// Only the allowlisted JSON emitter writes stdout. Values never enter shell source.
const ACTIVATE = 'source "$1" "$2" 1>&2 || exit "$?"\nexec "$3" -e "$4"';
const EMIT_ACTIVATION = 'process.stdout.write(JSON.stringify(Object.fromEntries('
  + JSON.stringify(ACTIVATION_FIELDS) + '.map(k => [k, process.env[k]]))))';

function fullScopeRefusals(env) {
  // These inputs select substitutes or subsets in xtask, runTest, or suite/index.
  // Artifact locations and time budgets remain usable. Do not clear caller inputs.
  const selectors = ['RIPR_TEST_SERVER_PATH', 'RIPR_TEST_PACKAGED_SERVER_PATH',
    'RIPR_TEST_ACTIONABLE_QUEUE_ROOT', 'RIPR_TEST_MULTIROOT_ONLY', 'RIPR_TEST_MULTIROOT',
    'RIPR_TEST_GREP', 'RIPR_TEST_EXTENSION_PATH', 'RIPR_TEST_WORKSPACE_PATH',
    'RIPR_TEST_MULTIROOT_PATH', 'RIPR_TEST_MULTIROOT_RECEIPT', 'RIPR_TEST_REQUIRE_RAN'];
  const refused = selectors.filter(name => env[name] !== undefined);
  if (env.RIPR_TEST_WORKSPACE_TRUST !== undefined && env.RIPR_TEST_WORKSPACE_TRUST !== 'trusted') {
    refused.push('RIPR_TEST_WORKSPACE_TRUST');
  }
  return refused;
}

function fullPreflight(root, env, platform, nodeVersion) {
  const report = collectDeveloperSetup({ root, env, platform, nodeVersion });
  const refused = fullScopeRefusals(env);
  if (platform !== 'linux') throw new Error('Full validation currently supports Linux source checkouts only.');
  if (report.known_blockers.length) throw new Error(`Setup blockers: ${report.known_blockers.join(', ')}`);
  if (Number(nodeVersion.split('.')[0]) < 24) throw new Error('Full validation needs the source Node TypeScript runtime (Node 24 or later).');
  if (refused.length) throw new Error(`Full validation refuses reduced-scope editor inputs: ${refused.join(', ')}; unset them explicitly.`);
  if (report.requirements.rust_components.some(component => !['rustfmt', 'clippy'].includes(component))) {
    throw new Error('The current declared Rust components need a supported setup version probe.');
  }
  const editor = path.join(root, 'editors/vscode');
  for (const [name, expected] of [['typescript', report.requirements.typescript],
    ['@vscode/test-electron', report.requirements.test_electron]]) {
    const installed = JSON.parse(readAuthority(root, `editors/vscode/node_modules/${name}/package.json`).text);
    if (installed.version !== expected) throw new Error(`Installed ${name} does not match package-lock.json; run npm ci explicitly.`);
  }
  for (const filename of [path.join(root, 'tools/activate-rust.sh'),
    path.join(editor, 'node_modules/typescript/bin/tsc'), path.join(editor, 'node_modules/.bin/vsce')]) {
    if (!fs.statSync(filename).isFile()) throw new Error(`Missing required setup owner: ${filename}`);
  }
  return report;
}

/** Explicit Linux orchestration. A completed owner command is not cold/fresh-task proof. */
async function runFullDevelopment(options = {}) {
  const root = path.resolve(options.root ?? path.join(__dirname, '../../..'));
  const inherited = options.env ?? process.env;
  const platform = options.platform ?? process.platform;
  const nodeVersion = options.nodeVersion ?? process.versions.node;
  const timeoutMs = options.timeoutMs ?? 2 * 60 * 60 * 1000;
  const selection = options.toolchain ?? (inherited.RUSTUP_TOOLCHAIN || '--pinned');
  const selectionSource = options.toolchain !== undefined ? 'explicit_cli'
    : inherited.RUSTUP_TOOLCHAIN ? 'inherited_frozen_selector' : 'repository_pin';
  const started = Date.now();
  const artifactRoot = path.resolve(inherited.RIPR_TEST_ARTIFACT_ROOT ?? path.join(root, 'target/ripr'));
  const latest = path.join(artifactRoot, 'setup-full-development.json');
  const report = { schema_version: 1, kind: 'ripr.full-development', mode: 'explicit_full_validation',
    status: 'incomplete', readiness_scope: 'linux_source_checkout_workflows',
    run_id: crypto.randomBytes(16).toString('hex'), started_at: new Date(started).toISOString(),
    root: bounded(root), source: { head: null, cleanliness: 'not_proven' },
    compiler_selection: { selector: bounded(selection), source: selectionSource },
    tools: { node: bounded(nodeVersion) }, requirement_sources: [],
    limits: { timeout_ms: timeoutMs, process_output_bytes: 16 * 1024 * 1024,
      stage_log_bytes: MAX_FULL_LOG_BYTES, result_bytes: MAX_FULL_RESULT_BYTES },
    stages: FULL_STAGES.map(stage_id => ({ stage_id, status: 'not_run', commands: [] })),
    full_development_ready: false, cold_provisioning: 'not_proven', repeated_setup_reuse: 'not_proven',
    fresh_task: 'not_proven', measured_build_resource_envelope: 'not_proven',
    dependency_install_provenance: 'not_proven', hosted_pr_qualification: 'not_run' };
  let evidenceRoot;
  let current = report.stages[0];
  let runner;
  let redact = text => Buffer.from(bounded(text));
  let redactTail = redact;
  let exitCode = 1;
  let interruption;
  const cancellation = new AbortController();
  const onInterrupt = () => { interruption = 'SIGINT'; cancellation.abort(new Error('interrupted by SIGINT')); };
  const onTerminate = () => { interruption = 'SIGTERM'; cancellation.abort(new Error('interrupted by SIGTERM')); };
  const logs = new Map();
  const env = { ...inherited, RIPR_TEST_ARTIFACT_ROOT: artifactRoot,
    CARGO_TARGET_DIR: inherited.CARGO_TARGET_DIR === undefined
      ? path.join(root, 'target') : path.resolve(inherited.CARGO_TARGET_DIR) };
  function publish() {
    const text = JSON.stringify(report, null, 2) + '\n';
    if (Buffer.byteLength(text) > MAX_FULL_RESULT_BYTES) throw new Error('Full validation result exceeded its byte budget');
    fs.writeFileSync(path.join(evidenceRoot, 'result.json'), text);
    fs.writeFileSync(latest, text);
  }
  function retain(text, failureMessage) {
    const previous = logs.get(current.stage_id) ?? '';
    const summary = failureMessage === undefined ? undefined : redact(failureMessage + '\n', 2048);
    const bytes = summary === undefined ? redact(previous + text, MAX_FULL_LOG_BYTES)
      : Buffer.concat([summary, redactTail(previous + text, MAX_FULL_LOG_BYTES - summary.length)]);
    logs.set(current.stage_id, bytes.toString('utf8'));
    const filename = path.join(evidenceRoot, current.stage_id + '.log');
    fs.writeFileSync(filename, bytes);
    current.log = filename;
    current.log_bytes = bytes.length;
    current.log_capture = summary === undefined ? 'output_prefix' : 'error_summary_and_output_tail';
  }
  async function command(executable, args, cwd, structured = false) {
    const remaining = timeoutMs - (Date.now() - started);
    if (remaining < 1) throw new Error('Full validation attempt deadline reached before the next command');
    const record = { argv: [executable, ...args].map(bounded), cwd: bounded(cwd),
      capture: structured ? 'activation_allowlist_stdout' : 'merged_stdout_stderr',
      native_status: null, native_signal: null };
    // The activation script/emitter are fixed source, not environment data.
    if (structured) record.argv = ['bash', '--noprofile', '--norc', '<sourced activation helper>', selection];
    current.commands.push(record);
    try {
      const text = await runner(structured ? executable : 'bash', structured ? args
        : ['--noprofile', '--norc', '-c', FULL_CAPTURE, 'ripr-full-stage', executable, ...args], {
        cwd, env: { ...env, BASH_ENV: '', ...(structured ? { ELECTRON_RUN_AS_NODE: '1' } : {}) },
        timeoutMs: remaining, signal: cancellation.signal,
        onProgress: () => process.stderr.write(`Full validation: ${current.stage_id} (${Math.floor((Date.now() - started) / 1000)}s)\n`)
      });
      record.native_status = 0;
      if (!structured) retain(text);
      return text;
    } catch (error) {
      record.native_status = Number.isInteger(error?.status) ? error.status : null;
      record.native_signal = error?.signal ?? null;
      try { retain(`${structured ? '' : error?.stdout ?? ''}\n${error?.stderr ?? ''}`, String(error)); }
      catch (diagnosticError) { record.log_error = bounded(String(diagnosticError)); }
      throw error;
    }
  }
  async function stage(id, body) {
    current = report.stages.find(entry => entry.stage_id === id);
    current.status = 'running';
    publish();
    const began = Date.now();
    await body();
    current.status = 'pass';
    current.duration_ms = Date.now() - began;
    publish();
  }
  function hostReceipt(name, previousNonce) {
    const source = path.join(artifactRoot, 'setup-editor-host.json');
    const stat = fs.lstatSync(source);
    if (!stat.isFile() || stat.size > 4096) throw new Error('Expected the current bounded editor host receipt');
    const bytes = fs.readFileSync(source);
    if (bytes.length > 4096) throw new Error('Editor host receipt grew beyond its budget');
    const receipt = JSON.parse(bytes.toString('utf8'));
    if (receipt.schema_version !== 1 || receipt.check_id !== 'editor_native_host' || receipt.status !== 'pass'
      || receipt.platform !== 'linux' || receipt.native_exit_code !== 0 || receipt.smoke_cases !== 1
      || receipt.full_editor_suite !== 'not_run' || !/^[a-f0-9]{32}$/.test(receipt.nonce)
      || receipt.nonce === previousNonce) throw new Error('Editor owner did not provide a fresh passing host receipt');
    const destination = path.join(evidenceRoot, name + '.json');
    fs.writeFileSync(destination, bytes);
    current.receipts = [{ path: destination, sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
      scope: 'native_host_only', smoke_cases: 1 }];
    return receipt.nonce;
  }
  try {
    // Invalidate only our latest result before source, runtime or dependency checks.
    fs.rmSync(latest, { force: true });
    fs.mkdirSync(artifactRoot, { recursive: true });
    evidenceRoot = fs.mkdtempSync(path.join(artifactRoot, 'setup-full-'));
    report.evidence_directory = evidenceRoot;
    publish();
    await stage('preflight', async () => {
      if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 12 * 60 * 60 * 1000) {
        throw new Error('--timeout-ms must be an integer from 1 to 43200000');
      }
      if (typeof selection !== 'string' || selection.length > 128 || /[\x00-\x1f\x7f]/.test(selection)) {
        throw new Error('--toolchain needs one bounded selector accepted by tools/activate-rust.sh');
      }
      const observation = fullPreflight(root, inherited, platform, nodeVersion);
      report.requirement_sources = observation.requirement_sources;
      report.requirements = observation.requirements;
      // Lazy, after the checked Node/dependency preflight. No default-report imports.
      runner = require('../src/lspCompatibility.ts').runFixtureProducer;
      redact = require('./hostDiagnostics.ts').redactHostDiagnosticsPrefix;
      redactTail = require('./hostDiagnostics.ts').redactHostDiagnosticsTail;
      for (const directory of new Set([env.CARGO_TARGET_DIR, artifactRoot,
        ...observation.requirements.linker_temp.map(entry => entry.path)])) {
        if (directory.length >= MAX_TEXT) throw new Error('A work location exceeds the supported path-report budget');
        fs.mkdirSync(directory, { recursive: true });
        const probe = path.join(directory, `.ripr-setup-${report.run_id}`);
        const descriptor = fs.openSync(probe, 'wx');
        fs.closeSync(descriptor);
        fs.unlinkSync(probe);
      }
      const space = fs.statfsSync(env.CARGO_TARGET_DIR);
      current.cargo_target_directory = bounded(env.CARGO_TARGET_DIR);
      current.cargo_target_available_bytes = space.bavail * space.bsize;
      current.resource_limit = 'Writable paths and free bytes do not establish the full-build memory/storage envelope.';
    });
    process.once('SIGINT', onInterrupt);
    process.once('SIGTERM', onTerminate);
    await stage('source_identity', async () => {
      const head = (await command('git', ['rev-parse', 'HEAD'], root)).trim();
      if (!/^[a-f0-9]{40,64}$/.test(head)) throw new Error('Git did not report a bounded source identity');
      report.source.head = head;
      report.source.executed_sources = ['editors/vscode/test/developerSetup.cjs',
        'editors/vscode/src/lspCompatibility.ts', 'editors/vscode/test/hostDiagnostics.ts',
        'tools/activate-rust.sh'].map(relative => {
        const source = readAuthority(root, relative);
        return { path: relative, sha256: source.sha256 };
      });
    });
    await stage('activation', async () => {
      const text = await command('bash', ['--noprofile', '--norc', '-c', ACTIVATE,
        'ripr-full-activation', path.join(root, 'tools/activate-rust.sh'), selection,
        process.execPath, EMIT_ACTIVATION], root, true);
      if (Buffer.byteLength(text) > 64 * 1024) throw new Error('Activation metadata exceeded its byte budget');
      const selected = JSON.parse(text);
      if (!selected || Object.keys(selected).length !== ACTIVATION_FIELDS.length
        || ACTIVATION_FIELDS.some(key => typeof selected[key] !== 'string' || !selected[key])
        || selected.RUSTUP_AUTO_INSTALL !== '0') throw new Error('Activation did not return its complete allowlisted result');
      Object.assign(env, selected);
      current.fields_transferred = ACTIVATION_FIELDS;
      current.resolved_toolchain = bounded(selected.RUSTUP_TOOLCHAIN);
      retain('Existing helper succeeded; only its eight activation fields were transferred in memory.\n');
    });
    await stage('tool_versions', async () => {
      for (const [name, executable, args] of [['rustc', env.RUSTC, ['-vV']],
        ['cargo', 'cargo', ['-vV']], ['llvm_cov', env.LLVM_COV, ['--version']],
        ['llvm_profdata', env.LLVM_PROFDATA, ['--version']],
        ['nextest', 'cargo', ['nextest', '--version']], ['npm', 'npm', ['--version']]]) {
        report.tools[name] = redact(await command(executable, args, root), 4096).toString('utf8').trim();
      }
      for (const component of report.requirements.rust_components) {
        const executable = component === 'rustfmt' ? 'rustfmt' : 'cargo';
        const args = component === 'rustfmt' ? ['--version'] : ['clippy', '--version'];
        report.tools[component] = redact(await command(executable, args, root), 4096).toString('utf8').trim();
      }
      report.tools.vsce = redact(await command(path.join(root, 'editors/vscode/node_modules/.bin/vsce'),
        ['--version'], path.join(root, 'editors/vscode')), 4096).toString('utf8').trim();
    });
    await stage('profile_diagnostic', async () => {
      const output = await command('cargo', ['test', '--locked', '-p', 'repo-policy', '--lib', PROFILE_TEST,
        '--', '--exact', '--nocapture'], root);
      const association = /^real LLVM association: raw_version=(\d{1,20}), layout=([^,\r\n]{1,128}), build-id=([a-f0-9]{2,128}), retained=(\d{1,20}), mapping="matched", original=failure, replay=1$/m.exec(output);
      if (!output.includes(`test ${PROFILE_TEST} ... ok`) || !/^(?:test result: ok\. )?1 passed; 0 failed; 0 ignored(?:;[^\r\n]*)?$/m.test(output)
        || !association) throw new Error('The selected real-profile diagnostic did not establish its one intended case and native association');
      current.counts = { passed: 1, failed: 0, ignored: 0 };
      current.profile_association = { raw_version: association[1], layout: association[2], build_id: association[3],
        retained_bytes: association[4], expected_truncated_replay_exit: 1 };
    });
    let earlyNonce;
    const editor = path.join(root, 'editors/vscode');
    await stage('editor_prerequisites', async () => {
      await command('npm', ['run', 'setup:verify-editor'], editor);
      earlyNonce = hostReceipt('early-editor-host');
    });
    await stage('repository_policy', async () => { await command('cargo', ['policy', 'preflight'], root); });
    await stage('rust_full', async () => {
      await command('cargo', ['xtask', 'ci-full'], root);
      current.counts = { status: 'not_summarized', owner: 'cargo xtask ci-full', transcript: current.log,
        scope: 'The existing owner determines its denominators; exit zero does not invent counts.' };
    });
    await stage('editor_suite', async () => {
      const output = await command('npm', ['run', 'test:e2e'], editor);
      hostReceipt('canonical-editor-host', earlyNonce);
      const plain = output.replace(/\x1b\[[0-9;]*m/g, '');
      current.counts = [...plain.matchAll(/^\s*(\d+) (passing|pending|failing)\b.*$/gm)]
        .slice(0, 12).map(match => ({ count: Number(match[1]), outcome: match[2], scope: 'owner_transcript_observation' }));
      current.canonical_invocations = 1;
    });
    await stage('editor_package', async () => { await command('npm', ['run', 'package'], editor); });
    report.status = 'pass';
    report.full_development_ready = true;
    exitCode = 0;
  } catch (error) {
    current.status = 'fail';
    report.failure = { stage_id: current.stage_id, kind: bounded(error?.kind ?? 'setup_failure'),
      message: redact(String(error), 2048).toString('utf8'),
      native_status: Number.isInteger(error?.status) ? error.status : null,
      native_signal: error?.signal ?? null, interruption: interruption ?? null };
    if (Number.isInteger(error?.status) && error.status > 0 && error.status <= 255) exitCode = error.status;
    else if (interruption) exitCode = interruption === 'SIGINT' ? 130 : 143;
    else if (error?.signal) exitCode = 128 + (require('node:os').constants.signals[error.signal] ?? 0);
  } finally {
    process.removeListener('SIGINT', onInterrupt);
    process.removeListener('SIGTERM', onTerminate);
    report.finished_at = new Date().toISOString();
    report.duration_ms = Date.now() - started;
    report.exit_code = exitCode;
    if (evidenceRoot) {
      try { publish(); }
      catch (error) {
        report.status = 'incomplete'; report.full_development_ready = false;
        report.evidence_error = bounded(String(error));
        if (exitCode === 0) { exitCode = 1; report.exit_code = 1; }
      }
    }
  }
  return { report, exitCode };
}

function main(args = process.argv.slice(2)) {
  if (args.includes('--verify-full')) {
    const options = {};
    for (let index = 0; index < args.length; index++) {
      const arg = args[index];
      if (arg === '--verify-full' || arg === '--json') continue;
      if (arg === '--toolchain' && args[index + 1]) options.toolchain = args[++index];
      else if (arg === '--timeout-ms' && /^[1-9][0-9]*$/.test(args[index + 1] ?? '')) options.timeoutMs = Number(args[++index]);
      else { process.stderr.write('Full validation: use --verify-full [--json] [--toolchain SELECTOR] [--timeout-ms N]\n'); return 2; }
    }
    return runFullDevelopment(options).then(({ report, exitCode }) => {
      process.stdout.write(JSON.stringify(report, null, 2) + '\n');
      return exitCode;
    });
  }
  if (args.some(arg => !['--json', '--help', '--begin-editor-verification'].includes(arg))) {
    process.stderr.write('Usage: node editors/vscode/test/developerSetup.cjs [--json|--help]\n');
    return 2;
  }
  if (args.includes('--help')) {
    process.stdout.write('Read-only developer observations; no builds, writes, subprocesses or downloads.\nExplicit editor proof: cd editors/vscode && npm run setup:verify-editor\nExplicit Linux full validation: npm run setup:verify-full -- [--toolchain SELECTOR] [--timeout-ms N]\n');
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

module.exports = { collectDeveloperSetup, renderDeveloperSetup, resetEditorSetupEvidence,
  fullScopeRefusals, runFullDevelopment, main };
if (require.main === module) {
  Promise.resolve().then(() => main()).then(code => { process.exitCode = code; }, error => {
    process.stderr.write(bounded(String(error)) + '\n'); process.exitCode = 1;
  });
}
