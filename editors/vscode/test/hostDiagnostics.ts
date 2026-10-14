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

export function redactHostDiagnosticsText(text: string): string {
  let redacted = text.replace(COMPLETE_PEM, '[redacted-pem]');
  // PEM-tail matching is only safe when an END marker exists; otherwise a
  // long base64-looking log tail can backtrack without bound.
  if (redacted.includes('-----END ')) {
    redacted = redacted
      .replace(PEM_TAIL, '[redacted-pem-tail]')
      .replace(PEM_END_ONLY, '[redacted-pem-end]');
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
  let bytesWritten = 0;
  const logBudget = MAX_HOST_DIAGNOSTICS_BYTES - MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES;
  const logsRoot = path.join(diagnosticsDir, HOST_LOG_DIR_NAME);

  if (fs.existsSync(logDir) && fs.statSync(logDir).isDirectory()) {
    for (const relative of listRelativeFiles(logDir)) {
      const remaining = logBudget - bytesWritten;
      if (remaining <= 0) {
        break;
      }
      const source = path.join(logDir, relative);
      const copied = copyLogTail(
        source,
        path.join(logsRoot, relative),
        Math.min(MAX_HOST_DIAGNOSTICS_FILE_BYTES, remaining)
      );
      if (copied > 0) {
        copiedFiles.push(relative.split(path.sep).join('/'));
        bytesWritten += copied;
      }
    }
  }

  const env = input.env ?? process.env;
  let identity = redactHostDiagnosticsText(
    `${JSON.stringify(
      {
        schema_version: '0.1',
        retained_because: 'test_host_failure',
        user_data_dir: input.userDataDir,
        log_dir: logDir,
        max_total_bytes: MAX_HOST_DIAGNOSTICS_BYTES,
        max_file_bytes: MAX_HOST_DIAGNOSTICS_FILE_BYTES,
        retention_days: HOST_DIAGNOSTICS_RETENTION_DAYS,
        copied_files: copiedFiles,
        error: serializeError(input.error),
        github_run_id: env.GITHUB_RUN_ID ?? null,
        github_run_attempt: env.GITHUB_RUN_ATTEMPT ?? null,
        github_sha: env.GITHUB_SHA ?? null,
        github_job: env.GITHUB_JOB ?? null,
        runner_os: env.RUNNER_OS ?? null,
        claim:
          'failure-only host log retention; does not change the original test exit status'
      },
      null,
      2
    )}\n`
  );
  if (Buffer.byteLength(identity, 'utf8') > MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES) {
    identity = Buffer.from(identity, 'utf8')
      .subarray(0, MAX_HOST_DIAGNOSTICS_IDENTITY_BYTES)
      .toString('utf8');
  }
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
}): { exitCode: number; retention: RetainHostDiagnosticsResult } {
  let retention: RetainHostDiagnosticsResult;
  try {
    retention = retainHostDiagnostics({
      artifactRoot: input.artifactRoot,
      userDataDir: input.userDataDir,
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

function serializeError(error: unknown): { message: string; stack?: string } | null {
  if (error === undefined) {
    return null;
  }
  if (error instanceof Error) {
    return {
      message: clipErrorField(error.message),
      stack: error.stack === undefined ? undefined : clipErrorField(error.stack)
    };
  }
  return { message: clipErrorField(String(error)) };
}

function clipErrorField(text: string): string {
  if (text.length <= MAX_ERROR_FIELD_CHARS) {
    return text;
  }
  return `${text.slice(0, MAX_ERROR_FIELD_CHARS)}...[truncated]`;
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

function copyLogTail(source: string, destination: string, cap: number): number {
  if (cap <= 0) {
    return 0;
  }
  const stat = fs.statSync(source);
  if (!stat.isFile() || stat.size <= 0) {
    return 0;
  }
  const readBytes = Math.min(cap, stat.size);
  const start = stat.size > readBytes ? stat.size - readBytes : 0;
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
  let text = redactHostDiagnosticsText(buffer.toString('utf8'));
  if (Buffer.byteLength(text, 'utf8') > cap) {
    const encoded = Buffer.from(text, 'utf8');
    text = encoded.subarray(encoded.length - cap).toString('utf8');
  }
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.writeFileSync(destination, text, 'utf8');
  return Buffer.byteLength(text, 'utf8');
}
