import * as fs from 'fs';
import * as path from 'path';
import { randomBytes } from 'crypto';
import { downloadAndUnzipVSCode } from '@vscode/test-electron';
import { runFixtureProducer, FixtureProducerError } from './actionableQueueFixture';
import { redactHostDiagnosticsText } from './hostDiagnostics';

const DOWNLOAD_ATTEMPTS = 3;
const HOST_TIMEOUT_MS = 60_000;
const MAX_RECEIPT_BYTES = 4096;

// Retain the existing downloader, version selection, cache and retry policy.
// Its network timeout is the test-electron owner's idle timeout, not a claim
// that host acquisition or a whole setup run completed within the smoke budget.
export async function downloadVSCodeWithRetry(
  cachePath: string, extensionDevelopmentPath: string
): Promise<string> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= DOWNLOAD_ATTEMPTS; attempt += 1) {
    try {
      return await downloadAndUnzipVSCode({ cachePath, extensionDevelopmentPath });
    } catch (caught) {
      lastError = caught;
      console.warn(`VS Code download attempt ${attempt}/${DOWNLOAD_ATTEMPTS} failed: ${String(caught)}`);
      if (attempt < DOWNLOAD_ATTEMPTS) {
        await new Promise<void>((resolve) => setTimeout(resolve, attempt * 5000));
      }
    }
  }
  throw lastError;
}

export function requireEditorDisplay(platform: NodeJS.Platform, env: NodeJS.ProcessEnv): void {
  if (platform === 'linux' && !env.DISPLAY && !env.WAYLAND_DISPLAY) {
    throw new Error('VS Code setup needs a live display; use an existing display or run setup:verify-editor under an already installed xvfb-run. Producer and editor suites were not run.');
  }
}

export interface HostSetupReceipt {
  schema_version: 1;
  check_id: 'editor_native_host';
  status: 'pass';
  nonce: string;
  vscode_version: string;
  electron_version: string;
  platform: string;
  native_exit_code: 0;
  smoke_cases: 1;
  host_executable: string;
  full_editor_suite: 'not_run';
}

export interface HostPrerequisiteOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  env?: NodeJS.ProcessEnv;
  // The retained process owner remains the production runtime. Controls can
  // substitute a host command without downloading or starting a real editor.
  runProcess?: typeof runFixtureProducer;
}

function readHostSentinel(filename: string, nonce: string): Record<string, unknown> {
  const stat = fs.lstatSync(filename);
  if (!stat.isFile() || stat.size === 0 || stat.size > MAX_RECEIPT_BYTES) {
    throw new Error('VS Code setup did not produce a bounded regular host sentinel');
  }
  const value: unknown = JSON.parse(fs.readFileSync(filename, 'utf8'));
  if (!value || typeof value !== 'object') throw new Error('Invalid VS Code setup sentinel');
  const receipt = value as Record<string, unknown>;
  if (receipt.schema_version !== 1 || receipt.nonce !== nonce || receipt.smoke_cases !== 1
    || typeof receipt.vscode_version !== 'string' || !/^\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/.test(receipt.vscode_version)
    || typeof receipt.electron_version !== 'string' || !/^\d+\.\d+\.\d+(?:[-+][\w.-]+)?$/.test(receipt.electron_version)
    || receipt.platform !== process.platform) {
    throw new Error('VS Code setup sentinel is missing current native host identity');
  }
  return receipt;
}

// The sentinel loads only a disposable, empty extension and the VS Code API.
// It cannot start RIPR, run the queue producer, grant trust to a user workspace,
// or substitute for the canonical editor e2e suite.
export async function verifyHostPrerequisites(
  vscodeExecutablePath: string, artifactRoot: string, options: HostPrerequisiteOptions = {}
): Promise<HostSetupReceipt> {
  const env = options.env ?? process.env;
  const timeoutMs = options.timeoutMs ?? HOST_TIMEOUT_MS;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120_000) {
    throw new Error('VS Code host setup timeout must be an integer from 1 to 120000ms');
  }
  fs.mkdirSync(artifactRoot, { recursive: true });
  const publishedReceipt = path.join(artifactRoot, 'setup-editor-host.json');
  const failureLog = path.join(artifactRoot, 'setup-editor-host.log');
  fs.rmSync(publishedReceipt, { force: true });
  fs.rmSync(failureLog, { force: true });
  requireEditorDisplay(process.platform, env);
  const ownedRoot = fs.mkdtempSync(path.join(artifactRoot, 'setup-host-'));
  const extension = path.join(ownedRoot, 'empty extension');
  const receiptPath = path.join(ownedRoot, 'host receipt.json');
  const nonce = randomBytes(16).toString('hex');
  const cancellation = new AbortController();
  const onInterrupt = (): void => cancellation.abort(new Error('host setup interrupted by SIGINT'));
  const onTerminate = (): void => cancellation.abort(new Error('host setup interrupted by SIGTERM'));
  const onAbort = (): void => cancellation.abort(options.signal?.reason);
  process.once('SIGINT', onInterrupt);
  process.once('SIGTERM', onTerminate);
  options.signal?.addEventListener('abort', onAbort, { once: true });
  if (options.signal?.aborted) onAbort();
  try {
    fs.mkdirSync(extension);
    fs.writeFileSync(path.join(extension, 'package.json'), JSON.stringify({
      name: 'ripr-setup-smoke', publisher: 'ripr-test', version: '0.0.0',
      engines: { vscode: '*' }, main: './empty.js', activationEvents: []
    }));
    fs.writeFileSync(path.join(extension, 'empty.js'), 'exports.activate = () => {};\n');
    const args = ['--no-sandbox', '--disable-gpu-sandbox', '--disable-updates',
      '--skip-welcome', '--skip-release-notes', '--disable-extensions', '--disable-workspace-trust',
      '--extensions-dir', path.join(ownedRoot, 'extensions'),
      '--user-data-dir', path.join(ownedRoot, 'user data'),
      `--extensionDevelopmentPath=${extension}`,
      `--extensionTestsPath=${path.join(__dirname, 'setupHostSmoke.js')}`];
    await (options.runProcess ?? runFixtureProducer)(vscodeExecutablePath, args, {
      cwd: ownedRoot, timeoutMs, signal: cancellation.signal, maxOutputBytes: 1024 * 1024,
      env: { ...env, RIPR_SETUP_HOST_NONCE: nonce, RIPR_SETUP_HOST_RECEIPT: receiptPath },
      onProgress: () => console.info('Checking the native VS Code setup host before fixture production...')
    });
    const sentinel = readHostSentinel(receiptPath, nonce);
    const receipt: HostSetupReceipt = { schema_version: 1, check_id: 'editor_native_host', status: 'pass',
      nonce, vscode_version: sentinel.vscode_version as string, electron_version: sentinel.electron_version as string,
      platform: process.platform, native_exit_code: 0, smoke_cases: 1,
      host_executable: vscodeExecutablePath, full_editor_suite: 'not_run' };
    fs.writeFileSync(publishedReceipt, `${JSON.stringify(receipt, null, 2)}\n`);
    return receipt;
  } catch (error) {
    const detail = error instanceof FixtureProducerError
      ? `kind=${error.kind}; status=${error.status}; signal=${error.signal}\n${error.stdout}\n${error.stderr}`
      : String(error);
    fs.writeFileSync(failureLog, redactHostDiagnosticsText(detail.slice(-64 * 1024)));
    throw new Error(`Native VS Code setup failed before producer work; inspect ${failureLog}`, { cause: error });
  } finally {
    options.signal?.removeEventListener('abort', onAbort);
    process.removeListener('SIGINT', onInterrupt);
    process.removeListener('SIGTERM', onTerminate);
    // The bounded process owner settles only after close, including cleanup.
    fs.rmSync(ownedRoot, { recursive: true, force: true });
  }
}
