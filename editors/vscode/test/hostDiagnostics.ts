import * as fs from 'fs';
import * as path from 'path';

export const USER_DATA_DIR_NAME = 'vscode-test-user-data';
export const HOST_DIAGNOSTICS_DIR_NAME = 'vscode-test-host-diagnostics';
export const HOST_LOG_DIR_NAME = 'logs';
export const HOST_DIAGNOSTICS_IDENTITY_FILE = 'retention.json';

/** Total retained diagnostics copy, including identity. Matches the CI upload cap. */
export const MAX_HOST_DIAGNOSTICS_BYTES = 5 * 1024 * 1024;
/** Per-file tail retained from the VS Code host log directory. */
export const MAX_HOST_DIAGNOSTICS_FILE_BYTES = 256 * 1024;
/** Reserved identity-file budget inside the total cap. */
export const MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES = 16 * 1024;
/** GitHub Actions retention for the failure-only upload in `.github/workflows/ci.yml`. */
export const HOST_DIAGNOSTICS_RETENTION_DAYS = 5;
const MAX_ERROR_FIELD_CHARS = 2048;

const COMPLETE_PEM =
  /-----BEGIN [A-Z0-9 ]+-----[\s\S]*?-----END [A-Z0-9 ]+-----/g;
// Applied after complete PEM replacement. Linear to end-of-string; no
// backtracking. Fail-closed: an unmatched BEGIN drops the rest of this text.
const PEM_UNTERMINATED = /-----BEGIN [A-Z0-9 ]+-----[\s\S]*$/g;
// Bounded repeats: an unbounded `{32,}+` over a long base64-looking log
// backtracks without bound when no PEM follows (see host hang diagnosis).
const PEM_TAIL =
  /(?:[A-Za-z0-9+/]{32,512}={0,2}(?:\r?\n)){0,64}[A-Za-z0-9+/]{32,512}={0,2}\r?\n?-----END [A-Z0-9 ]+-----/g;
const PEM_END_ONLY = /-----END [A-Z0-9 ]+-----/g;
const GITHUB_TOKEN = /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b/g;
const GITHUB_PAT = /\bgithub_pat_[A-Za-z0-9_]{20,}\b/g;
const ACCESS_TOKEN_URL = /x-access-token:[^@\s]+/gi;

export interface RetainHostDiagnosticsInput {
  artifactRoot: string;
  userDataDir: string;
  failed: boolean;
  error?: unknown;
  env?: NodeJS.ProcessEnv;
  /** Only the matrix's named receipt; never a workspace or profile directory. */
  matrixReceiptPath?: string;
}

interface MatrixReceiptRetention {
  status: 'retained' | 'absent' | 'refused' | 'unreadable' | 'empty_or_binary';
  bytes: number;
  truncated?: boolean;
}

export interface RetainHostDiagnosticsResult {
  retained: boolean;
  diagnosticsDir: string;
  reason: string;
  bytesWritten: number;
  copiedFiles: string[];
}

export function resolveUserDataDir(artifactRoot: string): string {
  return path.join(artifactRoot, USER_DATA_DIR_NAME);
}

export function resolveLogDir(userDataDir: string): string {
  return path.join(userDataDir, HOST_LOG_DIR_NAME);
}

export function resolveHostDiagnosticsDir(artifactRoot: string): string {
  return path.join(artifactRoot, HOST_DIAGNOSTICS_DIR_NAME);
}

export function resetHostLogDir(userDataDir: string): void {
  fs.rmSync(resolveLogDir(userDataDir), { force: true, recursive: true });
}

export function redactHostDiagnosticsText(text: string): string {
  let redacted = text.replace(COMPLETE_PEM, '[redacted-pem]');
  // PEM-tail matching is only safe when an END marker exists; otherwise a
  // long base64-looking log tail can backtrack without bound.
  if (redacted.includes('-----END ')) {
    redacted = redacted
      .replace(PEM_TAIL, '[redacted-pem-tail]')
      .replace(PEM_END_ONLY, '[redacted-pem-end]');
  }
  if (redacted.includes('-----BEGIN ')) {
    redacted = redacted.replace(PEM_UNTERMINATED, '[redacted-pem]');
  }
  return redacted
    .replace(GITHUB_TOKEN, '[redacted-token]')
    .replace(GITHUB_PAT, '[redacted-token]')
    .replace(ACCESS_TOKEN_URL, 'x-access-token:[redacted]');
}

export function retainHostDiagnostics(
  input: RetainHostDiagnosticsInput
): RetainHostDiagnosticsResult {
  const diagnosticsDir = resolveHostDiagnosticsDir(input.artifactRoot);
  if (!input.failed) {
    fs.rmSync(diagnosticsDir, { force: true, recursive: true });
    return {
      retained: false,
      diagnosticsDir,
      reason: 'success_no_retention',
      bytesWritten: 0,
      copiedFiles: []
    };
  }

  fs.rmSync(diagnosticsDir, { force: true, recursive: true });
  fs.mkdirSync(diagnosticsDir, { recursive: true });

  const logDir = resolveLogDir(input.userDataDir);
  const copiedFiles: string[] = [];
  // Reserve the bounded receipt before filling the remaining log budget.
  const matrixReceipt = retainMatrixReceipt(input, diagnosticsDir);
  let bytesWritten = matrixReceipt?.bytes ?? 0;
  const logBudget = MAX_HOST_DIAGNOSTICS_BYTES - MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES;
  const logsRoot = path.join(diagnosticsDir, HOST_LOG_DIR_NAME);

  if (fs.existsSync(logDir) && fs.statSync(logDir).isDirectory()) {
    for (const relative of listRelativeFiles(logDir)) {
      const remaining = logBudget - bytesWritten;
      if (remaining <= 0) {
        break;
      }
      const source = path.join(logDir, relative);
      const copied = copyTextSlice(
        source,
        path.join(logsRoot, relative),
        Math.min(MAX_HOST_DIAGNOSTICS_FILE_BYTES, remaining),
        'tail'
      );
      if (copied > 0) {
        copiedFiles.push(relative.split(path.sep).join('/'));
        bytesWritten += copied;
      }
    }
  }

  const identity = serializeIdentity({
    userDataDir: input.userDataDir,
    logDir,
    copiedFiles,
    error: input.error,
    env: input.env ?? process.env,
    matrixReceipt
  });
  const identityPath = path.join(diagnosticsDir, HOST_DIAGNOSTICS_IDENTITY_FILE);
  fs.writeFileSync(identityPath, identity, 'utf8');
  bytesWritten += Buffer.byteLength(identity, 'utf8');

  return {
    retained: true,
    diagnosticsDir,
    reason: 'test_host_failure',
    bytesWritten,
    copiedFiles
  };
}

export function finalizeHostRun(input: {
  artifactRoot: string;
  userDataDir: string;
  error?: unknown;
  /** Set immediately before launching the isolated real-matrix host. */
  matrix?: { userDataDir: string; receiptPath: string };
}): { exitCode: number; retention: RetainHostDiagnosticsResult } {
  let retention: RetainHostDiagnosticsResult;
  try {
    retention = retainHostDiagnostics({
      artifactRoot: input.artifactRoot,
      userDataDir: input.matrix?.userDataDir ?? input.userDataDir,
      matrixReceiptPath: input.matrix?.receiptPath,
      failed: input.error !== undefined,
      error: input.error
    });
  } catch (retainErr) {
    console.error('Failed to retain VS Code test host diagnostics:', retainErr);
    retention = {
      retained: false,
      diagnosticsDir: resolveHostDiagnosticsDir(input.artifactRoot),
      reason: 'retention_failed',
      bytesWritten: 0,
      copiedFiles: []
    };
  }
  if (input.error !== undefined) {
    console.error('Failed to run tests:', input.error);
    return { exitCode: 1, retention };
  }
  return { exitCode: 0, retention };
}

const MAX_IDENTITY_PATH_CHARS = 256;
const MAX_COPIED_FILE_ENTRIES = 64;

function serializeIdentity(input: {
  userDataDir: string;
  logDir: string;
  copiedFiles: string[];
  error: unknown;
  env: NodeJS.ProcessEnv;
  matrixReceipt?: MatrixReceiptRetention;
}): string {
  const copied = input.copiedFiles
    .slice(0, MAX_COPIED_FILE_ENTRIES)
    .map((name) => clipField(name, MAX_IDENTITY_PATH_CHARS));
  const body: Record<string, unknown> = {
    schema_version: '0.1',
    retained_because: 'test_host_failure',
    user_data_dir: clipField(input.userDataDir, MAX_IDENTITY_PATH_CHARS),
    log_dir: clipField(input.logDir, MAX_IDENTITY_PATH_CHARS),
    max_total_bytes: MAX_HOST_DIAGNOSTICS_BYTES,
    max_file_bytes: MAX_HOST_DIAGNOSTICS_FILE_BYTES,
    retention_days: HOST_DIAGNOSTICS_RETENTION_DAYS,
    copied_files: copied,
    copied_files_omitted: Math.max(0, input.copiedFiles.length - copied.length),
    error: serializeError(input.error),
    github_run_id: clipEnv(input.env.GITHUB_RUN_ID),
    github_run_attempt: clipEnv(input.env.GITHUB_RUN_ATTEMPT),
    github_sha: clipEnv(input.env.GITHUB_SHA),
    github_job: clipEnv(input.env.GITHUB_JOB),
    runner_os: clipEnv(input.env.RUNNER_OS),
    ...(input.matrixReceipt ? { matrix_receipt: input.matrixReceipt } : {}),
    claim:
      'failure-only host log retention; does not change the original test exit status'
  };
  let identity = redactHostDiagnosticsText(`${JSON.stringify(body, null, 2)}\n`);
  if (Buffer.byteLength(identity, 'utf8') <= MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES) {
    return identity;
  }
  body.copied_files = [];
  body.copied_files_omitted = input.copiedFiles.length;
  identity = redactHostDiagnosticsText(`${JSON.stringify(body, null, 2)}\n`);
  if (Buffer.byteLength(identity, 'utf8') <= MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES) {
    return identity;
  }
  return `${JSON.stringify({
    schema_version: '0.1',
    retained_because: 'test_host_failure',
    copied_file_count: input.copiedFiles.length,
    identity_truncated: true
  })}\n`;
}

function retainMatrixReceipt(
  input: RetainHostDiagnosticsInput,
  diagnosticsDir: string
): MatrixReceiptRetention | undefined {
  if (input.matrixReceiptPath === undefined) {
    return undefined;
  }
  const source = path.resolve(input.matrixReceiptPath);
  const root = path.resolve(input.artifactRoot);
  const relative = path.relative(root, source);
  if (path.basename(source) !== 'acceptance.json' || !relative
    || relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)
    || source.startsWith(diagnosticsDir + path.sep)) {
    return { status: 'refused', bytes: 0 };
  }
  try {
    // Reject symlink/junction traversal, including a link to an otherwise
    // allowed filename. Read just this named file, not its directory siblings.
    let current = root;
    for (const part of relative.split(path.sep)) {
      current = path.join(current, part);
      if (fs.lstatSync(current).isSymbolicLink()) {
        return { status: 'refused', bytes: 0 };
      }
    }
    const stat = fs.lstatSync(source);
    if (!stat.isFile()) {
      return { status: 'refused', bytes: 0 };
    }
    // Keep a bounded prefix, so truncation cannot drop a PEM's opening marker
    // before the existing sanitizer sees it. Partial JSON remains useful evidence.
    const bytes = copyTextSlice(source, path.join(diagnosticsDir, 'acceptance.json'),
      MAX_HOST_DIAGNOSTICS_FILE_BYTES, 'prefix');
    return { status: bytes > 0 ? 'retained' : 'empty_or_binary', bytes,
      truncated: stat.size > MAX_HOST_DIAGNOSTICS_FILE_BYTES };
  } catch (error) {
    return { status: (error as NodeJS.ErrnoException).code === 'ENOENT' ? 'absent' : 'unreadable', bytes: 0 };
  }
}

function serializeError(error: unknown): { message: string; stack?: string } | null {
  if (error === undefined) {
    return null;
  }
  if (error instanceof Error) {
    return {
      message: clipRedactedField(error.message),
      stack: error.stack === undefined ? undefined : clipRedactedField(error.stack)
    };
  }
  return { message: clipRedactedField(String(error)) };
}

function clipRedactedField(text: string): string {
  return clipField(redactHostDiagnosticsText(text), MAX_ERROR_FIELD_CHARS);
}

/** Only observations already available to the matrix; absent fields stay absent. */
export interface HostRequestObservation {
  method: string;
  document_uri?: string;
  workspace_root?: string;
  controller_status?: string;
  sampled_effective_root?: string;
  sampled_root_state?: string;
  sampled_attempt_id?: string;
  sampled_run_status?: string;
  client_id?: number;
  current_client_id?: number;
  client_pid?: number;
  current_client_pid?: number;
  document_version?: number;
  range_start_line?: number;
  range_start_character?: number;
  range_end_line?: number;
  range_end_character?: number;
  same_client?: boolean;
}

export function boundedRequestEvidence(observation: HostRequestObservation, error?: unknown): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const key of ['method', 'document_uri', 'workspace_root', 'controller_status', 'sampled_effective_root', 'sampled_root_state',
    'sampled_attempt_id', 'sampled_run_status'] as const) {
    const value = observation[key];
    if (typeof value === 'string') {
      result[key] = clipField(redactHostDiagnosticsText(value), MAX_IDENTITY_PATH_CHARS);
    }
  }
  for (const key of ['client_id', 'current_client_id', 'client_pid', 'current_client_pid',
    'document_version', 'range_start_line', 'range_start_character', 'range_end_line',
    'range_end_character'] as const) {
    const value = observation[key];
    if (typeof value === 'number' && Number.isFinite(value)) {
      result[key] = value;
    }
  }
  if (typeof observation.same_client === 'boolean') {
    result.same_client = observation.same_client;
  }
  if (error !== undefined) {
    const code = (error as { code?: unknown } | null)?.code;
    result.error = { ...serializeError(error),
      ...(error instanceof Error ? { name: clipRedactedField(error.name) } : {}),
      ...(typeof code === 'string' ? { code: clipRedactedField(code) }
        : typeof code === 'number' && Number.isFinite(code) ? { code } : {}) };
  }
  return result;
}

function clipEnv(value: string | undefined): string | null {
  if (value === undefined || value.length === 0) {
    return null;
  }
  return clipField(value, MAX_IDENTITY_PATH_CHARS);
}

function clipField(text: string, maxChars: number): string {
  if (text.length <= maxChars) {
    return text;
  }
  return `${text.slice(0, maxChars)}...[truncated]`;
}

function listRelativeFiles(root: string, relative = ''): string[] {
  const dir = relative ? path.join(root, relative) : root;
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const next = relative ? path.join(relative, entry.name) : entry.name;
    if (entry.isDirectory()) {
      files.push(...listRelativeFiles(root, next));
    } else if (entry.isFile()) {
      files.push(next);
    }
  }
  return files;
}

function copyTextSlice(source: string, destination: string, cap: number, keep: 'prefix' | 'tail'): number {
  if (cap <= 0) {
    return 0;
  }
  const stat = fs.statSync(source);
  if (!stat.isFile() || stat.size <= 0) {
    return 0;
  }
  const readBytes = Math.min(cap, stat.size);
  const start = keep === 'tail' && stat.size > readBytes ? stat.size - readBytes : 0;
  const buffer = Buffer.alloc(readBytes);
  const fd = fs.openSync(source, 'r');
  try {
    fs.readSync(fd, buffer, 0, readBytes, start);
  } finally {
    fs.closeSync(fd);
  }
  if (buffer.includes(0)) {
    return 0;
  }
  const complete = keep === 'prefix' ? utf8Prefix(buffer, buffer.length) : buffer;
  const redacted = Buffer.from(redactHostDiagnosticsText(complete.toString('utf8')), 'utf8');
  const encoded = redacted.length > cap
    ? keep === 'prefix' ? utf8Prefix(redacted, cap) : utf8Suffix(redacted, cap)
    : redacted;
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.writeFileSync(destination, encoded);
  return encoded.length;
}

function utf8Prefix(buffer: Buffer, cap: number): Buffer {
  let end = Math.min(buffer.length, cap);
  let leading = end - 1;
  while (leading >= 0 && (buffer[leading] & 0xc0) === 0x80) { leading--; }
  const byte = buffer[leading];
  const width = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1;
  if (leading >= 0 && leading + width > end) { end = leading; }
  return buffer.subarray(0, end);
}

function utf8Suffix(buffer: Buffer, cap: number): Buffer {
  if (buffer.length <= cap) {
    return buffer;
  }
  let start = buffer.length - cap;
  while (start < buffer.length) {
    const byte = buffer[start];
    if (byte === undefined || (byte & 0xc0) !== 0x80) {
      break;
    }
    start += 1;
  }
  return buffer.subarray(start);
}
